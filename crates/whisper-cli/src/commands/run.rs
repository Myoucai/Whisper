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
: rev { [] { append } @fold } ;
: factorial { _ 1 > ?? _ 1 - factorial * | drop 1 ] } ;
: fib { _ 1 > ?? _ 1 - fib ` 2 - fib + | ] } ;
"#;
    let ast = Parser::parse_source(core_source)
        .map_err(|e| format!("Core word parse error at {}:{}: {}", e.token.span.line, e.token.span.column, e.message))?;
    let mut gen = BytecodeGenerator::new();
    let (_, defs) = gen.compile(&ast);
    for (name, code) in defs {
        vm.define_word(name, code);
    }
    Ok(())
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

    // Phase 1b: Type check
    let mut tc = whisper_typecheck::TypeChecker::new();
    let type_errors = tc.check(&ast);
    if !type_errors.is_empty() {
        for err in &type_errors {
            eprintln!("Type error: {} (in {})", err.message, err.context);
        }
        return Err(format!("{} type error(s) found", type_errors.len()));
    }

    // Phase 2: Compile AST to bytecode
    let mut gen = BytecodeGenerator::new();
    let (bytecode, defs) = gen.compile(&ast);

    // Phase 2b: Optimize bytecode
    let bytecode = whisper_codegen::optimize(&bytecode);
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
        capability_table.bind(Box::new(whisper_core::capability::HttpGetCap {
            id: 2,
            allowed_hosts: vec![
                "api.github.com".into(),
                "jsonplaceholder.typicode.com".into(),
            ],
        }));
        capability_table.bind(Box::new(whisper_core::capability::HttpPostCap {
            id: 3,
            allowed_hosts: vec![
                "api.github.com".into(),
                "jsonplaceholder.typicode.com".into(),
            ],
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

    // Phase 5: Execute
    match vm.execute(&bytecode) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Runtime error: {e}")),
    }
}
