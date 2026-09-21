fn main() {
    let path = std::env::args().nth(1).unwrap();
    let target = std::env::args().nth(2).unwrap();
    let src = std::fs::read_to_string(path).unwrap();
    let (ast, _) = whisper_parser::parser::Parser::parse_source_recovering(&src);
    if target == "<main>" {
        let refs: Vec<&whisper_parser::ast::AstNode> = ast
            .iter()
            .filter(|n| !matches!(n, whisper_parser::ast::AstNode::Def { .. }))
            .collect();
        let mut ctx = whisper_codegen::block_gen::BlockCtx {
            def_effects: &std::collections::HashMap::new(),
            quote_errors: Vec::new(),
        };
        let (code, _) = whisper_codegen::block_gen::checked_flat(&refs, false, &mut ctx);
        for (i, op) in code.iter().enumerate() {
            println!("[{:3}] {:?}", i, op);
        }
        return;
    }
    for node in &ast {
        if let whisper_parser::ast::AstNode::Def { name, body } = node {
            if name != &target {
                continue;
            }
            let refs: Vec<&whisper_parser::ast::AstNode> = body.iter().collect();
            let mut ctx = whisper_codegen::block_gen::BlockCtx {
                def_effects: &std::collections::HashMap::new(),
                quote_errors: Vec::new(),
            };
            let (code, _) = whisper_codegen::block_gen::checked_flat(&refs, false, &mut ctx);
            for (i, op) in code.iter().enumerate() {
                println!("[{:3}] {:?}", i, op);
            }
        }
    }
}
