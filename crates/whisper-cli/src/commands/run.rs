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

    // Phase 2: Type check (validates stack effects and type consistency)
    let mut checker = whisper_typecheck::TypeChecker::new();
    let mut type_errors = Vec::new();
    for node in &ast {
        // Collect word definitions and check them
        if let whisper_parser::ast::AstNode::Def { name, body } = node {
            // Try to infer the stack effect of the word body
            // For now, just validate that all referenced words exist
            for body_node in body {
                if let whisper_parser::ast::AstNode::WordRef(ref_name) = body_node {
                    if !is_builtin(ref_name) {
                        // Check if it's defined elsewhere in the program
                        let is_defined = ast.iter().any(|n| {
                            matches!(n, whisper_parser::ast::AstNode::Def { name, .. } if name == ref_name)
                        });
                        if !is_defined {
                            type_errors.push(format!(
                                "Undefined word '{}' in definition of '{}'",
                                ref_name, name
                            ));
                        }
                    }
                }
            }
            // Mark word as defined in type env
            let _ = checker.fresh_var(); // Type inference would happen here
        }
    }

    if !type_errors.is_empty() {
        for err in &type_errors {
            eprintln!("Type error: {err}");
        }
        return Err(format!("{} type error(s) found", type_errors.len()));
    }

    // Phase 3: Compile AST to bytecode
    let mut gen = BytecodeGenerator::new();
    let (bytecode, defs) = gen.compile(&ast);

    // Phase 4: Set up VM with requested capabilities
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
        eprintln!("Note: HTTP capabilities not yet implemented");
    }

    let mut vm = Vm::with_capabilities(capability_table);

    // Register word definitions
    for (name, code) in defs {
        vm.define_word(name, code);
    }

    // Phase 5: Execute
    match vm.execute(&bytecode) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Runtime error: {e}")),
    }
}

fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "dup" | "swap" | "drop" | "rot" | "pick"
            | "add" | "sub" | "mul" | "div" | "mod"
            | "eq" | "lt" | "gt" | "neq" | "le" | "ge"
            | "and" | "or" | "not" | "nth" | "append" | "len"
            | "map" | "each" | "fold" | "times"
    ) || name.starts_with('@')
}
