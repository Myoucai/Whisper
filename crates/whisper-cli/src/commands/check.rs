/// whisper check — Type-check without executing

use whisper_parser::Parser;

/// Type-check a Whisper source file.
pub fn check_file(source: &str) -> Result<(), String> {
    // Phase 1: Parse
    let ast = Parser::parse_source(source).map_err(|e| {
        format!("Parse error at {}:{}: {}", e.token.span.line, e.token.span.column, e.message)
    })?;

    // Phase 2: Type check (basic structural validation for now)
    println!("✓ Parsed successfully — {} nodes", ast.len());
    println!("✓ No syntax errors detected");

    // Basic validation: check for undefined word references
    let mut defined_words = std::collections::HashSet::new();
    for node in &ast {
        if let whisper_parser::ast::AstNode::Def { name, .. } = node {
            defined_words.insert(name.clone());
        }
    }

    for node in &ast {
        if let whisper_parser::ast::AstNode::WordRef(name) = node {
            if !defined_words.contains(name)
                && !is_builtin(name)
                && !is_operator(name)
            {
                println!("⚠ Warning: undefined word reference: '{name}'");
            }
        }
    }

    println!("✓ Check complete");
    Ok(())
}

fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "dup" | "swap" | "drop" | "rot" | "pick"
            | "add" | "sub" | "mul" | "div" | "mod"
            | "eq" | "lt" | "gt" | "neq" | "le" | "ge"
            | "and" | "or" | "not"
    )
}

fn is_operator(name: &str) -> bool {
    matches!(
        name,
        "_" | "`" | "%" | "@" | "+" | "-" | "*" | "/"
            | "=" | "<" | ">" | "!=" | "<=" | ">="
            | "&" | "|" | "!" | "." | "," | ".."
            | "@nth" | "@map" | "@each" | "@fold" | "@times"
            | "append" | "len" | "#" | "!!"
    )
}
