//! whisper run — Execute a Whisper source file

use std::path::Path;
use whisper_codegen::bytecode_gen::BytecodeGenerator;
use whisper_core::capability::{CapabilityTable, FileReadCap, FileWriteCap};
use whisper_core::vm::Vm;
use whisper_parser::Parser;

/// Runtime configuration for source execution.
pub struct RunConfig {
    pub allow_file_read: bool,
    pub allow_file_write: bool,
    pub allow_http: bool,
    pub allow_env: bool,
    pub allow_exec: bool,
    pub trace: bool,
    /// 类型错误默认致命（J-3 反转）；--no-strict-types 降回警告模式。
    pub strict_types: bool,
}

/// Register core built-in words that are always available without import.
fn register_core_words(vm: &mut Vm) -> Result<(), String> {
    let core_source = r#"
: abs  { _ 0 < ?? 0 ` - | ] } ;
: max  { _ $2 > ?? ` drop | drop ] } ;
: min  { _ $2 ` < ?? drop | ` drop ] } ;
: neg  { 0 ` - } ;
: sq   { _ * } ;
: cube { _ sq * } ;
: even? { _ 2 % 0 = } ;
: odd?  { _ 2 % 1 = } ;
: zero? { _ 0 = } ;
: positive? { _ 0 > } ;
: negative? { _ 0 < } ;
: inc  { 1 + } ;
: dec  { 1 - } ;
: double { _ + } ;
: halve { 2 / } ;
: sqrt { fsqrt } ;
: sum  { 0 { + } @fold } ;
: prod { 1 { * } @fold } ;
: mean { _ sum ` len / } ;
: first { 0 @nth } ;
: last { _ len 1 - @nth } ;
: gcd { _ 0 = ?? drop | ` over % gcd ] } ;
: has? { strfind 0 >= } ;
: pow { _ 0 = ?? drop 1 | 1 ` { over * } @times ] } ;
: range-to { _ 0 > ?? _ 1 - range-to ` append | drop [] ] } ;
: factorial { _ 1 > ?? _ 1 - factorial * | drop 1 ] } ;
: fib { _ 1 > ?? _ 1 - fib ` 2 - fib + | ] } ;
"#;
    let ast = Parser::parse_source(core_source).map_err(|e| {
        format!(
            "Core word parse error at {}:{}: {}",
            e.token.span.line, e.token.span.column, e.message
        )
    })?;
    let mut gen = BytecodeGenerator::new();
    let (_, defs) = gen.compile(&ast).map_err(|e| {
        format!(
            "Compile error: {}",
            whisper_codegen::block_ir::contract_errors_to_string(&e)
        )
    })?;
    for (name, code) in defs {
        vm.define_word(name, code);
    }
    Ok(())
}

/// Run a compiled .wbin file (v2 blocks are linearized here —
/// "loading-time linearization"; v1 files run flat as-is).
pub fn run_wbin(data: &[u8], config: &RunConfig) -> Result<(), String> {
    use whisper_codegen::wbin::WbinReader;

    let unit = WbinReader::decode_unit(data).map_err(|e| format!("Invalid .wbin: {e}"))?;
    let (bytecode, defs) = unit.linearized();

    let bytecode = if std::env::var("WHISPER_NO_OPT").is_ok() {
        bytecode
    } else {
        whisper_codegen::optimize(&bytecode)
    };
    let defs: Vec<_> = defs
        .into_iter()
        .map(|(k, v)| (k, whisper_codegen::optimize(&v)))
        .collect();

    let mut capability_table = CapabilityTable::new();
    if config.allow_file_read {
        capability_table.bind(Box::new(FileReadCap {
            id: 0,
            allowed_paths: vec![std::env::current_dir().unwrap_or_default()],
        }));
    }
    if config.allow_file_write {
        capability_table.bind(Box::new(FileWriteCap {
            id: 1,
            allowed_paths: vec![std::env::current_dir().unwrap_or_default()],
        }));
    }

    let mut vm = Vm::with_capabilities(capability_table);
    if config.trace {
        vm.trace = true;
    }
    register_core_words(&mut vm)?;
    for (name, code) in &defs {
        vm.define_word(name.clone(), code.clone());
    }
    vm.program_args = std::env::args()
        .map(|a| whisper_core::value::Value::Str(std::rc::Rc::new(a)))
        .collect();

    match vm.execute(&bytecode) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Runtime error: {e}")),
    }
}

/// Run a Whisper source file with optional capability bindings.
pub fn run_source(source: &str, source_dir: &Path, config: &RunConfig) -> Result<(), String> {
    // Phase 1: Parse source to AST (warnings: def/keyword collisions etc.)
    let (ast, parse_warnings) = {
        let (nodes, errors, warnings) = Parser::parse_source_recovering_full(source);
        if let Some(e) = errors.into_iter().next() {
            return Err(format!(
                "Parse error at {}:{}: {}",
                e.token.span.line, e.token.span.column, e.message
            ));
        }
        (nodes, warnings)
    };
    for w in &parse_warnings {
        eprintln!("Parse warning: {w}");
    }

    // Phase 1a: Resolve imports
    let resolved = whisper_parser::resolve_imports(ast, source_dir)
        .map_err(|e| format!("Import error: {e}"))?;
    let ast = resolved.ast;

    // Phase 1b: Type check（默认警告；--strict-types 致命化。
    // J-3 目标为默认致命，前置条件是全套零误报——本轮已修复系统性
    // 误报族，但 kv 态词 + @fold 引号栈的近似仍产生少量误报，
    // 翻转推迟至 checker 重设计（见 docs/hot-word-design.md 同级的
    // typecheck 设计注），如实记录于 CHANGELOG。）
    let mut tc = whisper_typecheck::TypeChecker::new();
    let type_errors = tc.check(&ast);
    if !type_errors.is_empty() {
        for err in &type_errors {
            eprintln!("Type error: {} (in {})", err.message, err.context);
        }
        if config.strict_types {
            return Err(format!("{} type error(s)", type_errors.len()));
        }
    }

    // Phase 2: Compile AST to bytecode
    let mut gen = BytecodeGenerator::new();
    let (bytecode, defs) = gen.compile(&ast).map_err(|e| {
        format!(
            "Compile error: {}",
            whisper_codegen::block_ir::contract_errors_to_string(&e)
        )
    })?;

    // Phase 2b: Optimize bytecode (WHISPER_NO_OPT=1 skips — debugging aid)
    let bytecode = if std::env::var("WHISPER_NO_OPT").is_ok() {
        bytecode
    } else {
        whisper_codegen::optimize(&bytecode)
    };
    let defs: Vec<_> = defs
        .into_iter()
        .map(|(k, v)| (k, whisper_codegen::optimize(&v)))
        .collect();

    // Phase 4: Set up VM with requested capabilities
    let mut capability_table = CapabilityTable::new();

    if config.allow_file_read {
        capability_table.bind(Box::new(FileReadCap {
            id: 0,
            allowed_paths: vec![std::env::current_dir().unwrap_or_default()],
        }));
    }
    if config.allow_file_write {
        capability_table.bind(Box::new(FileWriteCap {
            id: 1,
            allowed_paths: vec![std::env::current_dir().unwrap_or_default()],
        }));
    }
    if config.allow_http {
        // WHISPER_HTTP_HOSTS=host1,host2 附加授权靶场（逗号分隔；
        // 自建靶场在线验收用，白名单机制本身不变）。
        let extra = std::env::var("WHISPER_HTTP_HOSTS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .map(String::from)
            .collect::<Vec<_>>();
        let hosts = || {
            let mut v = vec![
                "api.github.com".into(),
                "jsonplaceholder.typicode.com".into(),
                "127.0.0.1".into(),
                "localhost".into(),
            ];
            v.extend(extra.iter().cloned());
            v
        };
        capability_table.bind(Box::new(whisper_core::capability::HttpGetCap {
            id: 2,
            allowed_hosts: hosts(),
        }));
        capability_table.bind(Box::new(whisper_core::capability::HttpPostCap {
            id: 3,
            allowed_hosts: hosts(),
        }));
        // L-06 会话族：共享 cookie 罐与审计日志
        let jar = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let audit = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        capability_table.bind(Box::new(whisper_core::http_session::HttpSessionCap {
            id: 6,
            allowed_hosts: hosts(),
            jar: jar.clone(),
            audit: audit.clone(),
            tls_override: None,
            raw_body: false,
        }));
        // L-12 raw 通道：hex body → 原始字节（multipart/二进制），共用罐与审计
        capability_table.bind(Box::new(whisper_core::http_session::HttpSessionCap {
            id: 8,
            allowed_hosts: hosts(),
            jar: jar.clone(),
            audit: audit.clone(),
            tls_override: None,
            raw_body: true,
        }));
        capability_table.bind(Box::new(
            whisper_core::http_session::HttpSessionParallelCap {
                id: 9,
                inner: whisper_core::http_session::HttpSessionCap {
                    id: 9,
                    allowed_hosts: hosts(),
                    jar: jar.clone(),
                    audit: audit.clone(),
                    tls_override: None,
                    raw_body: false,
                },
            },
        ));
        capability_table.bind(Box::new(whisper_core::http_session::HttpAuditCap {
            id: 7,
            audit,
        }));
    }
    if config.allow_env {
        capability_table.bind(Box::new(whisper_core::capability::EnvCap { id: 4 }));
    }
    if config.allow_exec {
        capability_table.bind(Box::new(whisper_core::capability::ExecCap {
            id: 5,
            trim: true,
        }));
        // W-6: same authority, verbatim streams. Bound together on purpose —
        // see the doc comment on ExecCap.
        capability_table.bind(Box::new(whisper_core::capability::ExecCap {
            id: 10,
            trim: false,
        }));
    }

    let mut vm = Vm::with_capabilities(capability_table);
    if config.trace {
        vm.trace = true;
        eprintln!("[trace] VM execution trace enabled");
    }

    // Register core built-in words (available without import)
    register_core_words(&mut vm)?;

    // Register optimized word definitions
    for (name, code) in &defs {
        vm.define_word(name.clone(), code.clone());
    }

    // Set program arguments
    vm.program_args = std::env::args()
        .map(|a| whisper_core::value::Value::Str(std::rc::Rc::new(a)))
        .collect();

    // Phase 5: Execute
    match vm.execute(&bytecode) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Runtime error: {e}")),
    }
}
