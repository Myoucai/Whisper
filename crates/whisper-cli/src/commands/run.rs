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
    let (_, defs) = gen
        .compile(&ast)
        .map_err(|e| format!("Compile error: {}", whisper_codegen::block_ir::contract_errors_to_string(&e)))?;
    for (name, code) in defs {
        vm.define_word(name, code);
    }
    Ok(())
}

/// Run a compiled .wbin file (v2 blocks are linearized here —
/// "loading-time linearization"; v1 files run flat as-is).
pub fn run_wbin(data: &[u8], config: &RunConfig) -> Result<(), String> {
    use whisper_codegen::wbin::WbinReader;

    let unit = WbinReader::decode_unit(data)
        .map_err(|e| format!("Invalid .wbin: {e}"))?;
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
    // Phase 1: Parse source to AST
    let ast = Parser::parse_source(source).map_err(|e| {
        format!(
            "Parse error at {}:{}: {}",
            e.token.span.line, e.token.span.column, e.message
        )
    })?;

    // Phase 1a: Resolve imports
    let resolved = whisper_parser::resolve_imports(ast, source_dir)
        .map_err(|e| format!("Import error: {e}"))?;
    let ast = resolved.ast;

    // Phase 1b: Type check (warnings only — recursive words may produce false positives)
    let mut tc = whisper_typecheck::TypeChecker::new();
    let type_errors = tc.check(&ast);
    if !type_errors.is_empty() {
        for err in &type_errors {
            eprintln!("Type warning: {} (in {})", err.message, err.context);
        }
    }

    // Phase 2: Compile AST to bytecode
    let mut gen = BytecodeGenerator::new();
    let (bytecode, defs) = gen
        .compile(&ast)
        .map_err(|e| format!("Compile error: {}", whisper_codegen::block_ir::contract_errors_to_string(&e)))?;

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
        let hosts = || {
            vec![
                "api.github.com".into(),
                "jsonplaceholder.typicode.com".into(),
                "127.0.0.1".into(),
                "localhost".into(),
            ]
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
        let jar = std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::HashMap::new(),
        ));
        let audit = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        capability_table.bind(Box::new(whisper_core::http_session::HttpSessionCap {
            id: 6,
            allowed_hosts: hosts(),
            jar: jar.clone(),
            audit: audit.clone(),
        }));
        capability_table.bind(Box::new(whisper_core::http_session::HttpAuditCap {
            id: 7,
            audit,
        }));
    }
    if config.allow_env {
        capability_table.bind(Box::new(whisper_core::capability::EnvCap { id: 4 }));
    }
    if config.allow_exec {
        capability_table.bind(Box::new(whisper_core::capability::ExecCap { id: 5 }));
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
