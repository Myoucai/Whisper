/// whisper bootstrap — Run the Whisper self-hosting compiler pipeline
///
/// Pipeline: Rust Lexer → Whisper Compiler → VM Execution
/// This demonstrates soft-bootstrapping:
///   1. Rust tokenizes the source
///   2. Whisper compiler processes tokens into bytecode
///   3. Rust VM executes the result

use whisper_core::opcode::Opcode;
use whisper_core::value::Value;
use whisper_core::vm::Vm;
use whisper_codegen::bytecode_gen::BytecodeGenerator;
use whisper_parser::Parser;
use whisper_parser::ast::{AstNode, Operator};
use std::rc::Rc;

/// Convert a parsed AST into Whisper-compatible tokens.
/// Returns a Value::List of tokens, where each token is [type, val].
fn ast_to_whisper_tokens(nodes: &[AstNode]) -> Value {
    let mut tokens = Vec::new();
    for node in nodes {
        match node {
            AstNode::Literal(val) => {
                let ty = match val {
                    Value::I64(_) => 0i64,
                    Value::F64(_) => 1i64,
                    Value::Str(_) => 2i64,
                    Value::Bool(b) => {
                        tokens.push(Value::List(Rc::new(vec![
                            Value::I64(13),
                            Value::I64(if *b { 1 } else { 0 }),
                        ])));
                        continue;
                    }
                    _ => continue,
                };
                let inner = match val {
                    Value::I64(n) => Value::I64(*n),
                    Value::F64(n) => Value::I64(n.to_bits() as i64), // encode f64 as bits
                    Value::Str(s) => Value::Str(s.clone()),
                    _ => continue,
                };
                tokens.push(Value::List(Rc::new(vec![Value::I64(ty), inner])));
            }
            AstNode::Op(op) => {
                let op_byte = op_to_byte(*op);
                tokens.push(Value::List(Rc::new(vec![
                    Value::I64(3), // type=operator
                    Value::I64(op_byte as i64),
                ])));
            }
            AstNode::WordRef(name) => {
                tokens.push(Value::List(Rc::new(vec![
                    Value::I64(4), // type=word
                    Value::Str(Rc::new(name.clone())),
                ])));
            }
            _ => {} // Skip complex nodes for now
        }
    }
    Value::List(Rc::new(tokens))
}

fn op_to_byte(op: Operator) -> u8 {
    match op {
        Operator::Dup => 0x00,
        Operator::Swap => 0x01,
        Operator::Drop => 0x02,
        Operator::Add => 0x10,
        Operator::Sub => 0x11,
        Operator::Mul => 0x12,
        Operator::Div => 0x13,
        Operator::Eq => 0x18,
        Operator::Lt => 0x19,
        Operator::Gt => 0x1A,
        Operator::And => 0x20,
        Operator::Or => 0x21,
        Operator::Not => 0x22,
        Operator::OutputTop => 0x90,
        _ => 0x00,
    }
}

/// Run the self-hosting compile pipeline.
pub fn bootstrap_compile(source: &str) -> Result<(), String> {
    // Phase 1: Rust lexer + parser → AST
    let ast = Parser::parse_source(source).map_err(|e| {
        format!("Parse error: {}", e.message)
    })?;

    // Phase 2: AST → Whisper tokens
    let tokens = ast_to_whisper_tokens(&ast);

    // Phase 3: Load whisperc/main.ws compiler
    let compiler_src = include_str!("../../../../whisperc/main.ws");
    let compiler_ast = Parser::parse_source(compiler_src).map_err(|e| {
        format!("Compiler parse error: {}", e.message)
    })?;
    let mut gen = BytecodeGenerator::new();
    let (compiler_bc, compiler_defs) = gen.compile(&compiler_ast);

    // Phase 4: Run Whisper compiler on tokens
    let mut vm = Vm::new();
    for (name, code) in compiler_defs {
        vm.define_word(name, code);
    }
    vm.execute(&compiler_bc).map_err(|e| format!("Compiler init error: {e}"))?;

    // Phase 5: Execute compile and get result
    println!("whisperc: {} source tokens, {} compiler words",
        match &tokens { Value::List(l) => l.len(), _ => 0 },
        vm.word_dict.len());
    vm.data_stack.push(tokens);
    let call_compile = [Opcode::Call("compile".to_string())];
    let compile_result = vm.execute(&call_compile)
        .map_err(|e| format!("Compile error: {e}"))?;

    // Phase 6: Show result
    match compile_result {
        Some(Value::List(ops)) => {
            println!("Compiled: {} opcodes", ops.len());
            for op in ops.iter() { println!("  {}", op); }
        }
        Some(val) => println!("Compiled: {}", val),
        None => println!("No output produced"),
    }

    Ok(())
}
