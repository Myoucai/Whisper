//! whisper check — Type-check without executing

use whisper_parser::Parser;
use whisper_typecheck::TypeChecker;

/// Type-check a Whisper source file.
pub fn check_file(source: &str) -> Result<(), String> {
    // Phase 1: Parse
    let ast = Parser::parse_source(source).map_err(|e| {
        format!("Parse error at {}:{}: {}", e.token.span.line, e.token.span.column, e.message)
    })?;

    println!("Parsed: {} nodes", ast.len());

    // Phase 2: Type check
    let mut tc = TypeChecker::new();
    let errors = tc.check(&ast);

    if errors.is_empty() {
        println!("Type check: PASSED");
        // Show inferred stack effect
        let mut stack: Vec<&str> = Vec::new();
        for node in &ast {
            match node {
                whisper_parser::ast::AstNode::Literal(_) => stack.push("T"),
                whisper_parser::ast::AstNode::Op(_) => { stack.pop(); }
                whisper_parser::ast::AstNode::Def { name, .. } => {
                    println!("  word '{}' defined", name);
                }
                _ => {}
            }
        }
        if !stack.is_empty() {
            println!("  final stack: {} value(s)", stack.len());
        }
        Ok(())
    } else {
        for err in &errors {
            eprintln!("  Error: {} (in {})", err.message, err.context);
        }
        Err(format!("Type check failed: {} error(s)", errors.len()))
    }
}
