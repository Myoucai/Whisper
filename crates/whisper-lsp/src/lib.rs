//! Whisper LSP Server — Language Server Protocol for Whisper .ws files.
//!
//! Provides:
//!   - Diagnostics (parse errors, type errors)
//!   - Hover (operator descriptions, word signatures)
//!   - Go-to-definition (word references)
//!   - Document symbols (word definitions)
//!   - Completion (operators, defined words)
//!
//! Can be used as a library (call `run_lsp_server()`) or as a standalone binary.

use lsp_server::{Connection, Message, Request, Response};
use lsp_types::*;
use std::cell::RefCell;
use std::collections::HashMap;
use whisper_parser::ast::AstNode;
use whisper_parser::Parser;

/// In-memory document store.
#[derive(Clone)]
struct Document {
    text: String,
    version: i32,
}

/// The LSP server state (document storage uses RefCell for interior mutability).
struct Server {
    connection: Connection,
    documents: RefCell<HashMap<Uri, Document>>,
}

impl Server {
    fn new(connection: Connection) -> Self {
        Server {
            connection,
            documents: RefCell::new(HashMap::new()),
        }
    }

    fn run(&self) -> anyhow::Result<()> {
        let server_capabilities = serde_json::to_value(ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            definition_provider: Some(OneOf::Left(true)),
            document_symbol_provider: Some(OneOf::Left(true)),
            completion_provider: Some(CompletionOptions::default()),
            ..Default::default()
        })
        .unwrap();

        let init_params = self
            .connection
            .initialize(server_capabilities)
            .map_err(|e| anyhow::anyhow!("init: {e}"))?;
        eprintln!("Whisper LSP initialized for: {init_params:?}");

        for msg in &self.connection.receiver {
            match msg {
                Message::Request(req) => {
                    if self.connection.handle_shutdown(&req).unwrap_or(false) {
                        return Ok(());
                    }
                    let resp = self.handle_request(&req);
                    if let Err(e) = self.connection.sender.send(Message::Response(resp)) {
                        eprintln!("Failed to send response: {e}");
                    }
                }
                Message::Notification(notif) => {
                    self.handle_notification(&notif);
                }
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    fn handle_request(&self, req: &Request) -> Response {
        let id = req.id.clone();
        match req.method.as_str() {
            "textDocument/hover" => {
                let result = self.hover(req);
                Response::new_ok(id, result)
            }
            "textDocument/definition" => {
                let result = self.definition(req);
                Response::new_ok(id, result)
            }
            "textDocument/documentSymbol" => {
                let result = self.document_symbols(req);
                Response::new_ok(id, result)
            }
            "textDocument/completion" => {
                let result = self.completion(req);
                Response::new_ok(id, result)
            }
            _ => Response::new_ok(id, ()),
        }
    }

    fn handle_notification(&self, notif: &lsp_server::Notification) {
        match notif.method.as_str() {
            "textDocument/didOpen" => {
                let params: DidOpenTextDocumentParams =
                    serde_json::from_value(notif.params.clone()).unwrap();
                self.documents.borrow_mut().insert(
                    params.text_document.uri.clone(),
                    Document {
                        text: params.text_document.text,
                        version: params.text_document.version,
                    },
                );
                self.publish_diagnostics(&params.text_document.uri);
            }
            "textDocument/didChange" => {
                let params: DidChangeTextDocumentParams =
                    serde_json::from_value(notif.params.clone()).unwrap();
                if let Some(doc) = self
                    .documents
                    .borrow_mut()
                    .get_mut(&params.text_document.uri)
                {
                    if let Some(change) = params.content_changes.into_iter().last() {
                        doc.text = change.text;
                        doc.version = params.text_document.version;
                    }
                }
                self.publish_diagnostics(&params.text_document.uri);
            }
            "textDocument/didSave" => {
                let params: DidSaveTextDocumentParams =
                    serde_json::from_value(notif.params.clone()).unwrap();
                self.publish_diagnostics(&params.text_document.uri);
            }
            "textDocument/didClose" => {
                let params: DidCloseTextDocumentParams =
                    serde_json::from_value(notif.params.clone()).unwrap();
                self.documents
                    .borrow_mut()
                    .remove(&params.text_document.uri);
            }
            _ => {}
        }
    }

    fn get_document(&self, uri: &Uri) -> Option<Document> {
        self.documents.borrow().get(uri).cloned()
    }

    /// Parse and type-check the document, publishing diagnostics.
    fn publish_diagnostics(&self, uri: &Uri) {
        let diagnostics = if let Some(doc) = self.get_document(uri) {
            let mut diags = Vec::new();

            // Parse with recovery — always get an AST + all errors
            let (ast, parse_errors) = Parser::parse_source_recovering(&doc.text);

            // Report all parse errors with source locations
            for err in &parse_errors {
                diags.push(Diagnostic {
                    range: Range {
                        start: Position::new(
                            err.token.span.line.saturating_sub(1) as u32,
                            err.token.span.column.saturating_sub(1) as u32,
                        ),
                        end: Position::new(
                            err.token.span.line.saturating_sub(1) as u32,
                            (err.token.span.column + err.token.lexeme.len().max(1))
                                .saturating_sub(1) as u32,
                        ),
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("Parse error: {}", err.message),
                    source: Some("whisper".into()),
                    ..Default::default()
                });
            }

            // Type-check the best-effort AST
            let mut tc = whisper_typecheck::TypeChecker::new();
            let type_errors = tc.check(&ast);
            for err in &type_errors {
                diags.push(Diagnostic {
                    range: Range {
                        start: Position::new(0, 0),
                        end: Position::new(0, 0),
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("Type error: {}", err.message),
                    source: Some("whisper".into()),
                    ..Default::default()
                });
            }

            diags
        } else {
            vec![]
        };

        let params = PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics,
            version: None,
        };
        let notif = lsp_server::Notification::new(
            "textDocument/publishDiagnostics".into(),
            serde_json::to_value(params).unwrap(),
        );
        let _ = self.connection.sender.send(Message::Notification(notif));
    }

    fn hover(&self, req: &Request) -> Option<Hover> {
        let params: HoverParams = serde_json::from_value(req.params.clone()).ok()?;
        let pos = params.text_document_position_params.position;
        let uri = &params.text_document_position_params.text_document.uri;
        let doc = self.get_document(uri)?;

        let word = word_at_position(&doc.text, pos)?;

        // Build enhanced hover with type signature
        let mut contents = hover_info(&word);

        // If this is a user-defined word, append its type signature
        if let Ok(ast) = Parser::parse_source(&doc.text) {
            let mut tc = whisper_typecheck::TypeChecker::new();
            let _ = tc.check(&ast);
            if let Some(sig) = tc.word_sigs.get(&word) {
                let in_str: Vec<String> = sig
                    .inputs
                    .iter()
                    .map(|t| format!("{}", tc.inferer.resolve(t)))
                    .collect();
                let out_str: Vec<String> = sig
                    .outputs
                    .iter()
                    .map(|t| format!("{}", tc.inferer.resolve(t)))
                    .collect();
                contents.push_str(&format!(
                    "\n\n**Type:** `[{}] → [{}]`",
                    in_str.join(" "),
                    out_str.join(" ")
                ));
            } else if let Some((inputs, outputs)) =
                whisper_typecheck::builtins::get_builtin_signature(&word)
            {
                let in_str: Vec<String> = inputs.iter().map(|t| format!("{t}")).collect();
                let out_str: Vec<String> = outputs.iter().map(|t| format!("{t}")).collect();
                contents.push_str(&format!(
                    "\n\n**Type:** `[{}] → [{}]`",
                    in_str.join(" "),
                    out_str.join(" ")
                ));
            }
        }

        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: contents,
            }),
            range: None,
        })
    }

    fn definition(&self, req: &Request) -> Option<GotoDefinitionResponse> {
        let params: GotoDefinitionParams = serde_json::from_value(req.params.clone()).ok()?;
        let pos = params.text_document_position_params.position;
        let uri = &params.text_document_position_params.text_document.uri;
        let doc = self.get_document(uri)?;

        let word = word_at_position(&doc.text, pos)?;

        // Search for actual source position of ': wordname'
        let def_pos = find_definition_position(&doc.text, &word)?;

        Some(GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: Range {
                start: Position::new(def_pos.line, def_pos.col),
                end: Position::new(def_pos.line, def_pos.col + word.len() as u32 + 2),
            },
        }))
    }

    fn document_symbols(&self, req: &Request) -> Option<DocumentSymbolResponse> {
        let params: DocumentSymbolParams = serde_json::from_value(req.params.clone()).ok()?;
        let doc = self.get_document(&params.text_document.uri)?;

        let mut symbols = Vec::new();
        if let Ok(ast) = Parser::parse_source(&doc.text) {
            for node in &ast {
                if let AstNode::Def { name, body } = node {
                    let def_pos = find_definition_position(&doc.text, name)
                        .unwrap_or(DefPosition { line: 0, col: 0 });
                    symbols.push(DocumentSymbol {
                        name: name.clone(),
                        detail: Some(format!("({} nodes)", body.len())),
                        kind: SymbolKind::FUNCTION,
                        range: Range {
                            start: Position::new(def_pos.line, def_pos.col),
                            end: Position::new(def_pos.line, def_pos.col + 1),
                        },
                        selection_range: Range {
                            start: Position::new(def_pos.line, def_pos.col + 2),
                            end: Position::new(def_pos.line, def_pos.col + 2 + name.len() as u32),
                        },
                        children: None,
                        tags: None,
                        #[allow(deprecated)]
                        deprecated: None,
                    });
                }
            }
        }

        Some(DocumentSymbolResponse::Nested(symbols))
    }

    fn completion(&self, req: &Request) -> Option<CompletionResponse> {
        let params: CompletionParams = serde_json::from_value(req.params.clone()).ok()?;
        let doc = self.get_document(&params.text_document_position.text_document.uri)?;
        let doc_dir = uri_to_path(&params.text_document_position.text_document.uri)
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));

        let pos = params.text_document_position.position;
        let items = completion_items(&doc.text, doc_dir.as_deref(), Some(pos));
        Some(CompletionResponse::Array(items))
    }
}

/// Minimal file:/// URI → filesystem path (percent-decodes %XX hex).
fn uri_to_path(uri: &Uri) -> Option<std::path::PathBuf> {
    let s = uri.as_str();
    let rest = s.strip_prefix("file:///")?;
    let mut out = Vec::with_capacity(rest.len());
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(std::path::PathBuf::from(
        String::from_utf8_lossy(&out).into_owned(),
    ))
}

/// Pure completion list for a document — unit-testable without a server.
/// `pos` enables import-path completion when the cursor sits inside an
/// `import "..."` string.
fn completion_items(
    text: &str,
    doc_dir: Option<&std::path::Path>,
    pos: Option<Position>,
) -> Vec<CompletionItem> {
    if let Some(pos) = pos {
        if let Some(items) = import_path_completions(text, doc_dir, pos) {
            return items;
        }
    }
    let mut items = Vec::new();

    // Builtin operators with type signatures
    let builtins: Vec<(&str, &str, Option<&str>)> = vec![
        // (name, description, optional type signature)
        ("dup", "_ — duplicate top of stack", Some("a → a a")),
        ("swap", "` — swap top two elements", Some("a b → b a")),
        ("drop", "drop — discard top of stack", Some("a →")),
        (
            "rot",
            "@ — rotate top three elements",
            Some("a b c → b c a"),
        ),
        ("+", "Add: a b → a+b", Some("num num → num")),
        ("-", "Subtract: a b → a−b", Some("num num → num")),
        ("*", "Multiply: a b → a×b", Some("num num → num")),
        ("/", "Divide: a b → a÷b", Some("num num → num")),
        ("mod", "Modulo: a b → a%b", Some("i64 i64 → i64")),
        ("=", "Equal: a b → a==b", Some("a b → bool")),
        ("<", "Less than: a b → a<b", Some("num num → bool")),
        (">", "Greater than: a b → a>b", Some("num num → bool")),
        ("!=", "Not equal: a b → a!=b", Some("a b → bool")),
        ("<=", "Less/equal: a b → a≤b", Some("num num → bool")),
        (">=", "Greater/equal: a b → a≥b", Some("num num → bool")),
        ("&", "AND: a b → a&&b", Some("bool bool → bool")),
        ("|", "OR: a b → a||b", Some("bool bool → bool")),
        ("!", "NOT: a → !a", Some("bool → bool")),
        ("@nth", "list n → element", Some("[T] i64 → T")),
        ("append", "list elem → new-list", Some("[T] T → [T]")),
        ("len", "list → count", Some("[T] → i64")),
        ("@map", "list quot → new-list", Some("[T] ref → [U]")),
        ("@each", "list quot →", Some("[T] ref →")),
        ("@fold", "list init quot → result", Some("[T] T ref → T")),
        ("@times", "n quot →", Some("i64 ref →")),
        ("strlen", "str → count", Some("str → i64")),
        ("strcat", "str1 str2 → str3", Some("str str → str")),
        (
            "strslice",
            "str start len → substr",
            Some("str i64 i64 → str"),
        ),
        ("streq", "str1 str2 → bool", Some("str str → bool")),
        ("strlt", "str1 str2 → bool", Some("str str → bool")),
        ("strfind", "str pattern → index", Some("str str → i64")),
        ("strreplace", "str old new → str", Some("str str str → str")),
        ("strtoi64", "str → i64", Some("str → i64")),
        ("i64tostr", "i64 → str", Some("i64 → str")),
        ("i64tof64", "i64 → f64", Some("i64 → f64")),
        ("f64toi64", "f64 → i64 (truncate)", Some("f64 → i64")),
        ("fsqrt", "√: f64 → f64", Some("f64 → f64")),
        ("fsin", "sin: f64 → f64", Some("f64 → f64")),
        ("fcos", "cos: f64 → f64", Some("f64 → f64")),
        ("ftan", "tan: f64 → f64", Some("f64 → f64")),
        ("json-parse", "Parse JSON: str → value", Some("str → value")),
        (
            "json-stringify",
            "To JSON: value → str",
            Some("value → str"),
        ),
        (".", "Output top of stack", Some("a →")),
        ("..", "Output entire stack", None),
        (",", "Read input", Some("→ str")),
        ("import", "Import module", None),
        ("export", "Export word", None),
        // kv / 状态
        (
            "kv-set",
            "key val kv-set — store (kv 字符串键值)",
            Some("str str →"),
        ),
        (
            "kv-get",
            "key kv-get — fetch (missing → empty str)",
            Some("str → str"),
        ),
        ("kv-del", "key kv-del", Some("str →")),
        ("kv-keys", "all keys", Some("→ [str]")),
        // map
        ("map-new", "empty map", Some("→ map")),
        (
            "map-get",
            "map key → val (missing → 0)",
            Some("map str → val"),
        ),
        ("map-set", "map key val → map", Some("map str val → map")),
        ("map-keys", "map → keys", Some("map → [str]")),
        ("map-has", "map key → bool", Some("map str → bool")),
        // 字符串/列表补遗
        ("strsplit", "str delim → parts", Some("str str → [str]")),
        ("strchars", "str → [i64]", Some("str → [i64]")),
        ("charsstr", "[i64] → str", Some("[i64] → str")),
        ("striter", "str → first rest", Some("str → i64 str")),
        ("strjoin", "[str] → str", Some("[str] → str")),
        ("ctos", "char-code → 1-char str", Some("i64 → str")),
        ("prepend", "elem list → list", Some("T [T] → [T]")),
        ("listfind", "[ [T] ] T → bool T", Some("[[T]] T → bool T")),
        ("sort", "sort list (bytewise for str)", Some("[T] → [T]")),
        ("rev", "reverse list", Some("[T] → [T]")),
        ("dedup", "remove adjacent/identity dups", Some("[T] → [T]")),
        ("take", "list n → first n", Some("[T] i64 → [T]")),
        ("drop-n", "list n → rest", Some("[T] i64 → [T]")),
        (
            "try",
            "run quote, catch error → bool result",
            Some("ref → bool T"),
        ),
        // 数值/时间/随机
        ("now", "unix ms", Some("→ i64")),
        ("sleep", "ms", Some("i64 →")),
        ("bitand", "i64 AND", Some("i64 i64 → i64")),
        ("bitor", "i64 OR", Some("i64 i64 → i64")),
        ("bitxor", "i64 XOR", Some("i64 i64 → i64")),
        ("bitnot", "i64 NOT", Some("i64 → i64")),
        ("<<", "shift left", Some("i64 i64 → i64")),
        (">>", "shift right", Some("i64 i64 → i64")),
        // 置信度（std/conf 定稿原语）
        (
            "conf-of",
            "peek confidence of top (plain → 1.0)",
            Some("x → x f64"),
        ),
        ("unwrap", "strip Signal wrapper", Some("sig → val")),
        ("seed!", "seed the RNG", Some("i64 →")),
        ("rand", "random f64 ∈ [0,1)", Some("→ f64")),
        ("randint", "lo hi → random i64", Some("i64 i64 → i64")),
        // 能力（需 --allow-* 标志）
        (
            "http-req",
            "method url headers body → status headers body (需 --allow-http)",
            Some("str str str str → i64 str str"),
        ),
    ];

    for (name, desc, type_sig) in &builtins {
        let detail = if let Some(sig) = type_sig {
            format!("{desc}  |  {sig}")
        } else {
            desc.to_string()
        };
        items.push(CompletionItem {
            label: name.to_string(),
            detail: Some(detail),
            kind: Some(if name.starts_with('@') {
                CompletionItemKind::METHOD
            } else if name.chars().all(|c| c.is_ascii_punctuation()) {
                CompletionItemKind::OPERATOR
            } else {
                CompletionItemKind::KEYWORD
            }),
            ..Default::default()
        });
    }

    // User-defined words with inferred type signatures
    if let Ok(ast) = Parser::parse_source(text) {
        let mut tc = whisper_typecheck::TypeChecker::new();
        let _ = tc.check(&ast);
        for node in &ast {
            if let AstNode::Def { name, body } = node {
                let type_info = if let Some(sig) = tc.word_sigs.get(name) {
                    let in_str: Vec<String> = sig
                        .inputs
                        .iter()
                        .map(|t| format!("{}", tc.inferer.resolve(t)))
                        .collect();
                    let out_str: Vec<String> = sig
                        .outputs
                        .iter()
                        .map(|t| format!("{}", tc.inferer.resolve(t)))
                        .collect();
                    format!(" | [{}] → [{}]", in_str.join(" "), out_str.join(" "))
                } else {
                    format!(" | ({} nodes)", body.len())
                };
                items.push(CompletionItem {
                    label: name.clone(),
                    detail: Some(type_info),
                    kind: Some(CompletionItemKind::FUNCTION),
                    ..Default::default()
                });
            }
        }
    }

    // Imported stdlib modules: offer their exported words
    for module in imported_std_modules(text) {
        for (word, from) in stdlib_module_exports(&module) {
            items.push(CompletionItem {
                label: word.clone(),
                detail: Some(format!("{word}  |  {from}")),
                kind: Some(CompletionItemKind::FUNCTION),
                ..Default::default()
            });
        }
    }

    items
}

/// `import "std/X"` module names referenced by the document.
fn imported_std_modules(text: &str) -> Vec<String> {
    let mut mods = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("import ") {
            let name = rest.trim().trim_matches('"');
            if let Some(m) = name.strip_prefix("std/") {
                if !m.is_empty() && !mods.iter().any(|x| x == m) {
                    mods.push(m.to_string());
                }
            }
        }
    }
    mods
}

/// stdlib root: <repo>/stdlib (two levels up from crates/whisper-lsp).
fn stdlib_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("stdlib")
}

/// If the cursor is inside an `import "..."` string, complete module paths:
/// std/<module> plus sibling .ws files of the document.
fn import_path_completions(
    text: &str,
    doc_dir: Option<&std::path::Path>,
    pos: Position,
) -> Option<Vec<CompletionItem>> {
    let line = text.lines().nth(pos.line as usize)?;
    let col = pos.character as usize;
    let upto: String = line.chars().take(col).collect();
    let str_start = upto.rfind('"')?;
    let prefix_with_quote = &upto[str_start..];
    // must be on an import line: text before the opening quote is `import ` + optional whitespace
    let before = &upto[..str_start];
    if !before.trim_end().ends_with("import") {
        return None;
    }
    let typed: String = prefix_with_quote.chars().skip(1).collect(); // drop the quote

    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(stdlib_dir()) {
        let mut mods: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "ws"))
            .filter_map(|e| e.file_name().into_string().ok())
            .map(|n| format!("std/{}", n.trim_end_matches(".ws")))
            .collect();
        mods.sort();
        for m in mods {
            if m.starts_with(&typed) {
                items.push(CompletionItem {
                    label: m.clone(),
                    detail: Some("stdlib module".into()),
                    kind: Some(CompletionItemKind::MODULE),
                    insert_text: Some(m),
                    ..Default::default()
                });
            }
        }
    }
    if let Some(dir) = doc_dir {
        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut files: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|x| x == "ws"))
                .filter_map(|e| e.file_name().into_string().ok())
                .collect();
            files.sort();
            for f in files {
                if f.starts_with(&typed) && f != typed {
                    items.push(CompletionItem {
                        label: f.clone(),
                        detail: Some("module (same directory)".into()),
                        kind: Some(CompletionItemKind::MODULE),
                        insert_text: Some(f.trim_end_matches(".ws").to_string()),
                        ..Default::default()
                    });
                }
            }
        }
    }
    Some(items)
}

/// Exported words of a stdlib module (parsed from source, export list wins).
fn stdlib_module_exports(module: &str) -> Vec<(String, String)> {
    let path = stdlib_dir().join(format!("{module}.ws"));
    let Ok(source) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(ast) = Parser::parse_source(&source) else {
        return Vec::new();
    };
    let defs: Vec<String> = ast
        .iter()
        .filter_map(|n| match n {
            AstNode::Def { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    let exports: Vec<String> = ast
        .iter()
        .filter_map(|n| match n {
            AstNode::Export(name) => Some(name.clone()),
            _ => None,
        })
        .collect();
    let names: Vec<String> = if exports.is_empty() { defs } else { exports };
    names
        .into_iter()
        .map(|n| (n, format!("std/{module}")))
        .collect()
}

/// Extract the word/operator at a given position.
fn word_at_position(text: &str, pos: Position) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = pos.line as usize;
    if line_idx >= lines.len() {
        return None;
    }
    let line = lines[line_idx];
    let col = pos.character as usize;
    if col >= line.len() {
        return None;
    }

    let chars: Vec<char> = line.chars().collect();
    let col = col.min(chars.len().saturating_sub(1));

    // Find word boundaries
    let mut start = col;
    while start > 0 && chars[start - 1].is_alphanumeric() {
        start -= 1;
    }
    let mut end = col;
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '@') {
        end += 1;
    }

    if start < end {
        Some(chars[start..end].iter().collect())
    } else if col < chars.len() {
        // Check for single-char operators
        let ch = chars[col];
        if "+-*/_`%.<>=&|!?,;:[]{}#".contains(ch) {
            Some(ch.to_string())
        } else {
            None
        }
    } else {
        None
    }
}

/// Get hover documentation for a word or operator.
fn hover_info(word: &str) -> String {
    match word {
        // Stack
        "_" | "dup" => "**dup** — duplicate top of stack\n\nStack: `a → a a`".into(),
        "`" | "swap" => "**swap** — swap top two elements\n\nStack: `a b → b a`".into(),
        "drop" => "**drop** — discard top of stack\n\nStack: `a →`".into(),
        "@" | "rot" => "**rot** — rotate top three elements\n\nStack: `a b c → b c a`".into(),

        // Arithmetic
        "+" => "**Add** — integer addition\n\nStack: `a b → a+b`".into(),
        "-" => "**Subtract** — integer subtraction\n\nStack: `a b → a−b`".into(),
        "*" => "**Multiply** — integer multiplication\n\nStack: `a b → a×b`".into(),
        "/" => "**Divide** — integer division\n\nStack: `a b → a÷b`".into(),
        "mod" => "**Modulo** — remainder\n\nStack: `a b → a%b`".into(),

        // Comparison
        "=" => "**Equal** — compare two values\n\nStack: `a b → bool`".into(),
        "<" => "**Less than**\n\nStack: `a b → bool`".into(),
        ">" => "**Greater than**\n\nStack: `a b → bool`".into(),
        "!=" => "**Not equal**\n\nStack: `a b → bool`".into(),
        "<=" => "**Less or equal**\n\nStack: `a b → bool`".into(),
        ">=" => "**Greater or equal**\n\nStack: `a b → bool`".into(),

        // Logic
        "&" => "**Logical AND**\n\nStack: `a b → a&&b`".into(),
        "|" => "**Logical OR**\n\nStack: `a b → a||b`".into(),
        "!" => "**Logical NOT**\n\nStack: `a → !a`".into(),

        // List
        "@nth" => "**@nth** — take element at index\n\nStack: `list n → element`".into(),
        "append" => "**append** — append element to list\n\nStack: `list elem → new-list`".into(),
        "len" => "**len** — list length\n\nStack: `list → count`".into(),
        "@map" => "**@map** — transform each element\n\nStack: `list quot → new-list`".into(),
        "@each" => "**@each** — iterate with side effects\n\nStack: `list quot →`".into(),
        "@fold" => "**@fold** — reduce list to value\n\nStack: `list init quot → result`".into(),
        "@times" => "**@times** — repeat N times\n\nStack: `n quot →`".into(),

        // String
        "strlen" => "**strlen** — string length\n\nStack: `str → count`".into(),
        "strcat" => "**strcat** — concatenate strings\n\nStack: `str1 str2 → str3`".into(),
        "strslice" => "**strslice** — substring\n\nStack: `str start len → substr`".into(),
        "streq" => "**streq** — string equality\n\nStack: `str1 str2 → bool`".into(),
        "strlt" => "**strlt** — lexicographic less-than\n\nStack: `str1 str2 → bool`".into(),
        "strfind" => "**strfind** — find first occurrence\n\nStack: `haystack pattern → index` (-1 if not found)".into(),
        "strreplace" => "**strreplace** — replace all\n\nStack: `str old new → result`".into(),
        "strtoi64" => "**strtoi64** — parse string to integer\n\nStack: `str → i64`".into(),
        "i64tostr" => "**i64tostr** — format integer to string\n\nStack: `i64 → str`".into(),

        // Float
        "i64tof64" => "**i64tof64** — convert i64 to f64\n\nStack: `i64 → f64`".into(),
        "f64toi64" => "**f64toi64** — truncate f64 to i64\n\nStack: `f64 → i64`".into(),
        "fsqrt" => "**fsqrt** — square root\n\nStack: `f64 → f64`".into(),
        "fsin" => "**fsin** — sine (radians)\n\nStack: `f64 → f64`".into(),
        "fcos" => "**fcos** — cosine (radians)\n\nStack: `f64 → f64`".into(),
        "ftan" => "**ftan** — tangent (radians)\n\nStack: `f64 → f64`".into(),

        // JSON
        "json-parse" => "**json-parse** — parse JSON string to value\n\nStack: `str → value`".into(),
        "json-stringify" => "**json-stringify** — serialize value to JSON\n\nStack: `value → str`".into(),

        // IO
        "." => "**Output** — print top of stack\n\nStack: `a →`".into(),
        ".." => "**OutputAll** — print entire stack".into(),
        "," => "**Read** — read input line → string".into(),
        "print" => "**print** — write top string to stdout verbatim (no newline)\n\nStack: `str →`".into(),
        "exit" => "**exit** — terminate the process with a real exit code\n\nStack: `i64 →`".into(),

        // Definition
        ":" => "**Word definition**\n\n`: name { body } ;`".into(),
        "import" => "**Import** — load module\n\n`import \"path\"`".into(),
        "export" => "**Export** — export word".into(),

        // Control flow
        "??" => "**Conditional**\n\n`cond ??then|else]`\nNested: `a ??b|c]`".into(),
        "?->" => "**Single-branch conditional**\n\n`cond {then} ?->`".into(),
        "#" => "**Loop**\n\n`{body} {cond} #`".into(),

        _ => format!("**{word}** — no documentation available"),
    }
}

/// Source position of a definition.
struct DefPosition {
    line: u32,
    col: u32,
}

/// Find the source position of `: wordname` in the document text.
fn find_definition_position(text: &str, word: &str) -> Option<DefPosition> {
    let pattern = format!(": {}", word);
    for (line_idx, line) in text.lines().enumerate() {
        if let Some(col) = line.find(&pattern) {
            return Some(DefPosition {
                line: line_idx as u32,
                col: col as u32,
            });
        }
    }
    None
}

/// Run the LSP server over stdio.
///
/// This is the main entry point — call it from the CLI or a standalone binary.
/// Blocks until the client disconnects or shuts down.
pub fn run_lsp_server() -> anyhow::Result<()> {
    eprintln!("Whisper LSP Server v1.0.0");
    let (connection, _io_threads) = Connection::stdio();
    let server = Server::new(connection);
    server.run()
}

#[cfg(test)]
mod completion_tests {
    use super::*;

    #[test]
    fn stdlib_words_offered_after_import() {
        let items = completion_items(
            "import \"std/str\"\n\"x\" ",
            None,
            Some(Position::new(1, 4)),
        );
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // std/str exports (spot-check a few from its export list)
        for expect in ["strsplit", "str-rev", "upper"] {
            assert!(
                labels.contains(&expect),
                "missing std/str export {expect}: {labels:?}"
            );
        }
        // builtin still present alongside
        assert!(labels.contains(&"dup"));
    }

    #[test]
    fn no_stdlib_words_without_import() {
        let items = completion_items("1 2 + .", None, Some(Position::new(0, 3)));
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            !labels.contains(&"str-rev"),
            "std word leaked without import"
        );
    }

    #[test]
    fn import_path_completion_inside_string() {
        // cursor right after `import "std/` on line 0 (empty module prefix)
        let text = "import \"std/";
        let items = completion_items(text, None, Some(Position::new(0, text.len() as u32)));
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"std/str"), "expected std/str: {labels:?}");
        assert!(
            labels.contains(&"std/list"),
            "expected std/list: {labels:?}"
        );
        assert!(
            labels.iter().all(|l| l.starts_with("std/")),
            "non-std entries leaked into import completion: {labels:?}"
        );
    }

    #[test]
    fn plain_code_does_not_trigger_import_completion() {
        let items = completion_items("\"std/something\" .", None, Some(Position::new(0, 6)));
        // falls back to word completion
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"dup"));
    }

    #[test]
    fn new_core_words_present() {
        let items = completion_items("", None, Some(Position::new(0, 0)));
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        for expect in ["kv-set", "conf-of", "unwrap", "map-set", "http-req"] {
            assert!(labels.contains(&expect), "missing {expect}");
        }
    }
}
