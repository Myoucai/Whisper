/// whisper bootstrap — Self-hosting compiler pipeline
///
/// Pipeline: Rust Lexer → Whisper Compiler → VM Execute
/// Demonstrates soft-bootstrapping where Whisper code compiles Whisper code.

use whisper_core::opcode::Opcode;
use whisper_core::value::Value;
use whisper_core::vm::Vm;
use whisper_codegen::bytecode_gen::BytecodeGenerator;
use whisper_parser::Parser;
use whisper_parser::ast::{AstNode, Operator};
use std::rc::Rc;

fn op_to_byte(op: Operator) -> u8 {
    match op {
        Operator::Dup => 0x00, Operator::Swap => 0x01, Operator::Drop => 0x02,
        Operator::Add => 0x10, Operator::Sub => 0x11, Operator::Mul => 0x12, Operator::Div => 0x13,
        Operator::Eq => 0x18, Operator::Lt => 0x19, Operator::Gt => 0x1A,
        Operator::And => 0x20, Operator::Or => 0x21, Operator::Not => 0x22,
        Operator::OutputTop => 0x90,
        _ => 0x00,
    }
}

fn ast_to_whisper_tokens(nodes: &[AstNode]) -> Value {
    let mut tokens = Vec::new();
    for node in nodes {
        match node {
            AstNode::Literal(val) => {
                let (ty, inner) = match val {
                    Value::I64(n) => (0i64, Value::I64(*n)),
                    Value::F64(n) => (1i64, Value::I64(n.to_bits() as i64)),
                    Value::Str(s) => (2i64, Value::Str(s.clone())),
                    Value::Bool(b) => (13i64, Value::I64(if *b { 1 } else { 0 })),
                    _ => continue,
                };
                tokens.push(Value::List(Rc::new(vec![Value::I64(ty), inner])));
            }
            AstNode::Op(op) => {
                tokens.push(Value::List(Rc::new(vec![
                    Value::I64(3),
                    Value::I64(op_to_byte(*op) as i64),
                ])));
            }
            AstNode::WordRef(name) => {
                tokens.push(Value::List(Rc::new(vec![
                    Value::I64(4),
                    Value::Str(Rc::new(name.clone())),
                ])));
            }
            _ => {}
        }
    }
    Value::List(Rc::new(tokens))
}

pub fn bootstrap_compile(source: &str) -> Result<(), String> {
    // Phase 1: Parse with Rust compiler (reference)
    let ast = Parser::parse_source(source).map_err(|e| format!("Parse: {}", e.message))?;
    let mut gen = BytecodeGenerator::new();
    let (ref_bytecode, ref_defs) = gen.compile(&ast);

    // Phase 2: Generate tokens for whisperc
    let tokens = ast_to_whisper_tokens(&ast);
    let token_count = match &tokens { Value::List(l) => l.len(), _ => 0 };
    println!("Tokens: {} items", token_count);

    // Phase 3: Load whisperc compiler
    let compiler_src = include_str!("../../../../whisperc/main.ws");
    let compiler_ast = Parser::parse_source(compiler_src)
        .map_err(|e| format!("whisperc parse: {}", e.message))?;
    let mut cgen = BytecodeGenerator::new();
    let (compiler_bc, compiler_defs) = cgen.compile(&compiler_ast);

    // Phase 4: Run whisperc on tokens
    let mut vm = Vm::new();
    for (name, code) in compiler_defs {
        vm.define_word(name, code);
    }
    vm.execute(&compiler_bc).map_err(|e| format!("whisperc init: {e}"))?;
    vm.data_stack.push(tokens);
    let call = [Opcode::Call("compile".to_string())];
    let result = vm.execute(&call).map_err(|e| format!("whisperc: {e}"))?;

    // Phase 5: Show whisperc output
    print!("whisperc output: ");
    match result {
        Some(Value::List(ref vals)) => {
            for v in vals.iter() { print!("{} ", v); }
            println!();
        }
        Some(ref v) => println!("{}", v),
        None => println!("(none)"),
    }

    // Phase 6: Execute Rust-compiled reference bytecode
    let mut vm2 = Vm::new();
    for (name, code) in ref_defs {
        vm2.define_word(name, code);
    }
    print!("Rust VM output:  ");
    vm2.execute(&ref_bytecode).map_err(|e| format!("VM: {e}"))?;

    println!("\nSelf-hosting pipeline complete.");
    Ok(())
}
