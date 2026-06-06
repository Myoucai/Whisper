/// whisper run — Execute a Whisper source file

use whisper_core::capability::{CapabilityTable, FileReadCap, FileWriteCap};
use whisper_core::vm::Vm;
use whisper_codegen::bytecode_gen::BytecodeGenerator;
use whisper_parser::Parser;

/// Run a Whisper source file with optional capability bindings.
pub fn run_file(
    source: &str,
    allow_file_read: bool,
    allow_file_write: bool,
    allow_http: bool,
) -> Result<(), String> {
    // Phase 1: Parse source to AST
    let ast = Parser::parse_source(source).map_err(|e| {
        format!("Parse error at {}:{}: {}", e.token.span.line, e.token.span.column, e.message)
    })?;

    // Phase 2: Compile AST to bytecode
    let mut gen = BytecodeGenerator::new();
    let bytecode = gen.compile(&ast);

    // Phase 3: Set up VM with requested capabilities
    let mut capability_table = CapabilityTable::new();

    if allow_file_read {
        capability_table.bind(Box::new(FileReadCap {
            id: 0,
            allowed_paths: vec![std::env::current_dir().unwrap_or_default()],
        }));
    }
    if allow_file_write {
        capability_table.bind(Box::new(FileWriteCap {
            id: 1,
            allowed_paths: vec![std::env::current_dir().unwrap_or_default()],
        }));
    }
    if allow_http {
        // In a full implementation, bind HTTP capabilities
        eprintln!("Warning: HTTP capabilities not yet implemented");
    }

    let mut vm = Vm::with_capabilities(capability_table);

    // Phase 4: Execute
    match vm.execute(&bytecode) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Runtime error: {e}")),
    }
}
