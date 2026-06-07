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
use whisper_parser::Parser;
use whisper_parser::ast::AstNode;

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
                if let Some(doc) = self.documents.borrow_mut().get_mut(&params.text_document.uri) {
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
                self.documents.borrow_mut().remove(&params.text_document.uri);
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

        // Find the word/operator at the cursor position
        let word = word_at_position(&doc.text, pos)?;
        let contents = hover_info(&word);

        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: contents,
            }),
            range: None,
        })
    }

    fn definition(&self, req: &Request) -> Option<GotoDefinitionResponse> {
        let params: GotoDefinitionParams =
            serde_json::from_value(req.params.clone()).ok()?;
        let pos = params.text_document_position_params.position;
        let uri = &params.text_document_position_params.text_document.uri;
        let doc = self.get_document(uri)?;

        // Find the word at cursor and locate its definition
        let word = word_at_position(&doc.text, pos)?;

        // Search for word definition in the document
        if let Ok(ast) = Parser::parse_source(&doc.text) {
            for node in &ast {
                if let AstNode::Def { name, .. } = node {
                    if name == &word {
                        // Return the same document location for now
                        return Some(GotoDefinitionResponse::Scalar(Location {
                            uri: uri.clone(),
                            range: Range {
                                start: Position::new(0, 0),
                                end: Position::new(0, 0),
                            },
                        }));
                    }
                }
            }
        }
        None
    }

    fn document_symbols(&self, req: &Request) -> Option<DocumentSymbolResponse> {
        let params: DocumentSymbolParams =
            serde_json::from_value(req.params.clone()).ok()?;
        let doc = self.get_document(&params.text_document.uri)?;

        let mut symbols = Vec::new();
        if let Ok(ast) = Parser::parse_source(&doc.text) {
            for node in &ast {
                if let AstNode::Def { name, body } = node {
                    symbols.push(DocumentSymbol {
                        name: name.clone(),
                        detail: Some(format!("Word definition ({} nodes)", body.len())),
                        kind: SymbolKind::FUNCTION,
                        range: Range {
                            start: Position::new(0, 0),
                            end: Position::new(0, 0),
                        },
                        selection_range: Range {
                            start: Position::new(0, 0),
                            end: Position::new(0, 0),
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
        let params: CompletionParams =
            serde_json::from_value(req.params.clone()).ok()?;
        let doc = self.get_document(&params.text_document_position.text_document.uri)?;

        let mut items = Vec::new();

        // Builtin operators
        let builtins = [
            ("dup", "_ — duplicate top of stack"),
            ("swap", "` — swap top two elements"),
            ("drop", "drop — discard top of stack"),
            ("rot", "@ — rotate top three elements"),
            ("+", "Add: a b → a+b"),
            ("-", "Subtract: a b → a−b"),
            ("*", "Multiply: a b → a×b"),
            ("/", "Divide: a b → a÷b"),
            ("mod", "Modulo: a b → a%b"),
            ("=", "Equal: a b → a==b"),
            ("<", "Less than: a b → a<b"),
            (">", "Greater than: a b → a>b"),
            ("!=", "Not equal: a b → a!=b"),
            ("<=", "Less/equal: a b → a≤b"),
            (">=", "Greater/equal: a b → a≥b"),
            ("&", "AND: a b → a&&b"),
            ("|", "OR: a b → a||b"),
            ("!", "NOT: a → !a"),
            ("@nth", "Take nth element: list n → elem"),
            ("append", "Append: list elem → new-list"),
            ("len", "Length: list → count"),
            ("@map", "Map: list quot → new-list"),
            ("@each", "Each: list quot →"),
            ("@fold", "Fold: list init quot → result"),
            ("@times", "Times: n quot →"),
            ("strlen", "String length: str → count"),
            ("strcat", "String concat: str1 str2 → str3"),
            ("strslice", "Substring: str start len → substr"),
            ("streq", "String equality: str1 str2 → bool"),
            ("strlt", "String less-than: str1 str2 → bool"),
            ("strfind", "Find substring: str pattern → index"),
            ("strreplace", "Replace: str old new → str"),
            ("strtoi64", "Parse string to i64: str → i64"),
            ("i64tostr", "Format i64 to string: i64 → str"),
            ("i64tof64", "Convert i64 to f64: i64 → f64"),
            ("f64toi64", "Convert f64 to i64 (truncate)"),
            ("fsqrt", "Square root: f64 → f64"),
            ("fsin", "Sine (radians): f64 → f64"),
            ("fcos", "Cosine (radians): f64 → f64"),
            ("ftan", "Tangent (radians): f64 → f64"),
            ("json-parse", "Parse JSON string → Whisper value"),
            ("json-stringify", "Serialize value → JSON string"),
            (".", "Output top of stack"),
            ("..", "Output entire stack"),
            (",", "Read input"),
            ("import", "Import module"),
            ("export", "Export word"),
        ];

        for (name, desc) in &builtins {
            items.push(CompletionItem {
                label: name.to_string(),
                detail: Some(desc.to_string()),
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

        // User-defined words
        if let Ok(ast) = Parser::parse_source(&doc.text) {
            for node in &ast {
                if let AstNode::Def { name, body } = node {
                    items.push(CompletionItem {
                        label: name.clone(),
                        detail: Some(format!("User word ({} nodes)", body.len())),
                        kind: Some(CompletionItemKind::FUNCTION),
                        ..Default::default()
                    });
                }
            }
        }

        Some(CompletionResponse::Array(items))
    }
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
