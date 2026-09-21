fn main() {
    let src = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let (ast, _) = whisper_parser::parser::Parser::parse_source_recovering(&src);
    for node in &ast {
        if let whisper_parser::ast::AstNode::Def { name, body } = node {
            let refs: Vec<&whisper_parser::ast::AstNode> = body.iter().collect();
            let mut ctx = whisper_codegen::block_gen::BlockCtx {
                def_effects: &std::collections::HashMap::new(),
                quote_errors: Vec::new(),
            };
            let (code, errs) = whisper_codegen::block_gen::checked_flat(&refs, false, &mut ctx);
            if !errs.is_empty() {
                println!("VIOLATION '{name}':");
                for c in errs {
                    println!("  {c}");
                }
            }
            let _ = code;
        }
    }
    println!("scan done");
}
