fn main() {
    let path = std::env::args().nth(1).unwrap();
    let src = std::fs::read_to_string(&path).unwrap();
    // 手工展开 import：把 std/str 拼在用户源前逐 def 查
    let (ast, _) = whisper_parser::parser::Parser::parse_source_recovering(&src);
    let mut defs: Vec<(String, Vec<whisper_parser::ast::AstNode>)> = vec![];
    for node in &ast {
        if let whisper_parser::ast::AstNode::Def { name, body } = node {
            defs.push((name.clone(), body.clone()));
        }
    }
    // 累积编译（照 compile_unit：def_effects 逐个注入）查合同
    let mut effects: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for (name, body) in &defs {
        let refs: Vec<&whisper_parser::ast::AstNode> = body.iter().collect();
        let (program, net, errs) = whisper_codegen::block_gen::compile_def_program(&refs, &effects);
        for e in errs {
            println!("{name}: {e}");
        }
        if let Some(n) = net {
            effects.insert(name.clone(), n);
        } else {
            println!("{name}: net=None (opaque)");
        }
        let _ = program;
    }
    println!("-- {} defs scanned (cumulative)", defs.len());
}
