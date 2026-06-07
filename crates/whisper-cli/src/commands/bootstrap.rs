//! whisper bootstrap — Self-hosting compiler pipeline
//!
//! Pipeline: Rust Lexer → Whisper Compiler → VM Execute
//!
//! Two-level compilation:
//!   1. Rust pre-pass: compiles structural nodes (quotes, conds, loops)
//!      and word definitions into flat bytecode tokens
//!   2. Whisper compiler (whisperc/main.ws): maps flat tokens to bytecodes
//!
//! Token format (flat, passed to whisperc):
//!   0  = I64 literal   [0, value]
//!   1  = F64 literal   [1, bits_as_i64]
//!   2  = Str literal   [2, string]
//!   3  = Operator      [3, opcode_byte]
//!   4  = WordRef       [4, name_string]
//!   13 = Bool literal  [13, 0/1]
//!   14 = ListCount     [14, count]
//!   18 = Pre-compiled PushRef  [18, [inner_bytecodes...]]

use std::rc::Rc;
use whisper_codegen::bytecode_gen::BytecodeGenerator;
use whisper_core::opcode::Opcode;
use whisper_core::value::Value;
use whisper_core::vm::Vm;
use whisper_parser::ast::{AstNode, Operator};
use whisper_parser::Parser;

fn op_to_byte(op: Operator) -> u8 {
    match op {
        Operator::Dup => 0x00,
        Operator::Swap => 0x01,
        Operator::Drop => 0x02,
        Operator::Rot => 0x03,
        Operator::Add => 0x10,
        Operator::Sub => 0x11,
        Operator::Mul => 0x12,
        Operator::Div => 0x13,
        Operator::Mod => 0x14,
        Operator::Eq => 0x18,
        Operator::Lt => 0x19,
        Operator::Gt => 0x1A,
        Operator::Neq => 0x1B,
        Operator::Le => 0x1C,
        Operator::Ge => 0x1D,
        Operator::And => 0x20,
        Operator::Or => 0x21,
        Operator::Not => 0x22,
        Operator::Nth => 0x40,
        Operator::Append => 0x41,
        Operator::Len => 0x42,
        Operator::Map => 0x43,
        Operator::Each => 0x44,
        Operator::Fold => 0x45,
        Operator::StrLen => 0x46,
        Operator::StrCat => 0x47,
        Operator::StrSlice => 0x48,
        Operator::StrEq => 0x49,
        Operator::StrLt => 0x4A,
        Operator::StrFind => 0x4B,
        Operator::StrReplace => 0x4C,
        Operator::StrToI64 => 0x4D,
        Operator::I64ToStr => 0x4E,
        Operator::StrNth => 0x4F,
        Operator::StrChars => 0xB8,
        Operator::CharsStr => 0xB9,
        Operator::StrIter => 0xBA,
        Operator::I64ToF64 => 0xB0,
        Operator::F64ToI64 => 0xB1,
        Operator::FSqrt => 0xB2,
        Operator::FSin => 0xB3,
        Operator::FCos => 0xB4,
        Operator::FTan => 0xB5,
        Operator::JsonParse => 0xB6,
        Operator::JsonStringify => 0xB7,
        Operator::AtTimes => 0x53,
        Operator::CapExec => 0x71,
        Operator::OutputTop => 0x90,
        Operator::OutputAll => 0x91,
        Operator::ReadInput => 0x92,
        _ => 0x00,
    }
}

/// Convert AST nodes to flat token Values, pre-compiling structural nodes.
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
            AstNode::List(items) => {
                for item in items {
                    tokens.append(&mut ast_to_vec(std::slice::from_ref(item)));
                }
                tokens.push(Value::List(Rc::new(vec![
                    Value::I64(14),
                    Value::I64(items.len() as i64),
                ])));
            }
            AstNode::Quote(body) => {
                // Pre-compile quotation body with trailing Return.
                // Token format: [18, [0x35, count, ...inners]]
                // whisperc's op-ref emits the wrapped value as-is.
                let inner_tokens = ast_to_vec(body);
                let mut inner_bytecodes = simple_compile(&inner_tokens);
                inner_bytecodes.push(Value::I64(0x61)); // Return
                let mut wrapped: Vec<Value> =
                    vec![Value::I64(0x35), Value::I64(inner_bytecodes.len() as i64)];
                wrapped.extend(inner_bytecodes);
                tokens.push(Value::List(Rc::new(vec![
                    Value::I64(18),
                    Value::List(Rc::new(wrapped)),
                ])));
            }
            // Word definitions, conds, loops, etc. are handled by the Rust
            // compiler separately — whisperc only sees the main program body.
            _ => {}
        }
    }
    Value::List(Rc::new(tokens))
}

fn ast_to_vec(nodes: &[AstNode]) -> Vec<Value> {
    let tokens = ast_to_whisper_tokens(nodes);
    match tokens {
        Value::List(v) => v.to_vec(),
        _ => vec![],
    }
}

/// Simple flat token compilation: maps token values directly to bytecode values.
/// This is what whisperc does — we provide a Rust implementation as reference.
fn simple_compile(tokens: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    for token in tokens {
        match token {
            Value::List(ref items) if items.len() >= 2 => {
                let ty = match &items[0] {
                    Value::I64(t) => *t,
                    _ => continue,
                };
                match ty {
                    0 => {
                        if let Value::I64(n) = &items[1] {
                            result
                                .push(Value::List(Rc::new(vec![Value::I64(0x30), Value::I64(*n)])));
                        }
                    }
                    1 => {
                        if let Value::I64(bits) = &items[1] {
                            result.push(Value::List(Rc::new(vec![
                                Value::I64(0x31),
                                Value::I64(*bits),
                            ])));
                        }
                    }
                    2 => {
                        result.push(Value::List(Rc::new(vec![
                            Value::I64(0x32),
                            items[1].clone(),
                        ])));
                    }
                    3 => {
                        if let Value::I64(byte) = &items[1] {
                            result.push(Value::I64(*byte));
                        }
                    }
                    4 => {
                        result.push(Value::List(Rc::new(vec![
                            Value::I64(0x60),
                            items[1].clone(),
                        ])));
                    }
                    13 => {
                        if let Value::I64(n) = &items[1] {
                            result
                                .push(Value::List(Rc::new(vec![Value::I64(0x33), Value::I64(*n)])));
                        }
                    }
                    14 => {
                        if let Value::I64(n) = &items[1] {
                            result
                                .push(Value::List(Rc::new(vec![Value::I64(0x34), Value::I64(*n)])));
                        }
                    }
                    18 => {
                        // Pre-wrapped PushRef [0x35, count, ...inners] — emit as-is
                        result.push(items[1].clone());
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    result
}

pub fn bootstrap_compile(source: &str) -> Result<(), String> {
    // Phase 1: Parse with Rust compiler (reference)
    let ast = Parser::parse_source(source).map_err(|e| format!("Parse: {}", e.message))?;
    let mut gen = BytecodeGenerator::new();
    let (ref_bytecode, ref_defs) = gen.compile(&ast);

    // Phase 2: Generate flat tokens for whisperc (main body only, defs handled separately)
    let main_body: Vec<AstNode> = ast
        .iter()
        .filter(|n| !matches!(n, AstNode::Def { .. }))
        .cloned()
        .collect();
    let tokens = ast_to_whisper_tokens(&main_body);
    let token_count = match &tokens {
        Value::List(l) => l.len(),
        _ => 0,
    };
    println!("Tokens: {} items", token_count);

    // Phase 3: Load whisperc compiler
    let compiler_src = include_str!("../../../../whisperc/main.ws");
    let compiler_ast =
        Parser::parse_source(compiler_src).map_err(|e| format!("whisperc parse: {}", e.message))?;
    let mut cgen = BytecodeGenerator::new();
    let (compiler_bc, compiler_defs) = cgen.compile(&compiler_ast);

    // Phase 4: Run whisperc on tokens
    let mut vm = Vm::new();
    for (name, code) in compiler_defs {
        vm.define_word(name, code);
    }
    vm.execute(&compiler_bc)
        .map_err(|e| format!("whisperc init: {e}"))?;
    vm.data_stack.push(tokens);
    let call = [Opcode::Call("compile".to_string())];
    let whisperc_result = vm.execute(&call).map_err(|e| format!("whisperc: {e}"))?;

    // Phase 5: Show whisperc output and convert to Opcodes
    print!("whisperc output: ");
    let whisperc_ops = match &whisperc_result {
        Some(Value::List(vals)) => {
            for v in vals.iter() {
                print!("{} ", v);
            }
            println!();
            values_to_opcodes(vals.to_vec())
        }
        _ => {
            println!("(none)");
            println!("whisperc produced no bytecode");
            return Ok(());
        }
    };

    // Phase 7: Execute Rust-compiled reference bytecode
    let mut vm2 = Vm::new();
    for (name, code) in &ref_defs {
        vm2.define_word(name.clone(), code.clone());
    }
    print!("Rust VM output: ");
    vm2.execute(&ref_bytecode).map_err(|e| format!("VM: {e}"))?;

    // Phase 8: Execute whisperc-compiled bytecode
    println!("\nwhisperc bytecode: {} opcodes", whisperc_ops.len());
    let mut vm3 = Vm::new();
    for (name, code) in &ref_defs {
        vm3.define_word(name.clone(), code.clone());
    }
    print!("whisperc VM output: ");
    vm3.execute(&whisperc_ops)
        .map_err(|e| format!("whisperc VM: {e}"))?;
    println!();

    println!("Self-hosting pipeline complete.");
    Ok(())
}

/// Convert whisperc output Values to Opcodes.
fn values_to_opcodes(vals: Vec<Value>) -> Vec<Opcode> {
    let mut ops = Vec::new();
    for val in &vals {
        match val {
            Value::I64(n) => {
                ops.push(byte_to_opcode(*n as u8));
            }
            Value::List(ref items) => {
                if items.is_empty() {
                    continue;
                }
                let byte = match &items[0] {
                    Value::I64(n) => *n as u8,
                    _ => continue,
                };
                match byte {
                    0x30 => {
                        if items.len() >= 2 {
                            if let Value::I64(n) = &items[1] {
                                ops.push(Opcode::PushI64(*n));
                            }
                        }
                    }
                    0x31 => {
                        if items.len() >= 2 {
                            if let Value::I64(n) = &items[1] {
                                ops.push(Opcode::PushF64(f64::from_bits(*n as u64)));
                            }
                        }
                    }
                    0x32 => {
                        if items.len() >= 2 {
                            if let Value::Str(s) = &items[1] {
                                ops.push(Opcode::PushStr(s.as_ref().clone()));
                            }
                        }
                    }
                    0x33 => {
                        if items.len() >= 2 {
                            if let Value::I64(n) = &items[1] {
                                ops.push(Opcode::PushBool(*n != 0));
                            }
                        }
                    }
                    0x34 => {
                        if items.len() >= 2 {
                            if let Value::I64(n) = &items[1] {
                                ops.push(Opcode::PushI64(*n));
                                ops.push(Opcode::PushList);
                            }
                        }
                    }
                    0x35 => {
                        // PushRef: [0x35, count, inner_vals...]
                        if items.len() >= 3 {
                            if let Value::I64(_count) = &items[1] {
                                let inner_vals: Vec<Value> = items[2..].to_vec();
                                let inner_ops = values_to_opcodes(inner_vals);
                                ops.push(Opcode::PushRef(inner_ops));
                            }
                        }
                    }
                    0x50 => {
                        if items.len() >= 2 {
                            if let Value::I64(off) = &items[1] {
                                ops.push(Opcode::Cond(*off as i32));
                            }
                        }
                    }
                    0x51 => {
                        if items.len() >= 2 {
                            if let Value::I64(off) = &items[1] {
                                ops.push(Opcode::Jump(*off as i32));
                            }
                        }
                    }
                    0x52 => {
                        if items.len() >= 2 {
                            if let Value::I64(off) = &items[1] {
                                ops.push(Opcode::Loop(*off as i32));
                            }
                        }
                    }
                    0x60 if items.len() >= 2 => {
                        if let Value::Str(s) = &items[1] {
                            ops.push(Opcode::Call(s.as_ref().clone()));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    ops
}

fn byte_to_opcode(byte: u8) -> Opcode {
    match byte {
        0x00 => Opcode::Dup,
        0x01 => Opcode::Swap,
        0x02 => Opcode::Drop,
        0x03 => Opcode::Rot,
        0x10 => Opcode::Add,
        0x11 => Opcode::Sub,
        0x12 => Opcode::Mul,
        0x13 => Opcode::Div,
        0x14 => Opcode::Mod,
        0x18 => Opcode::Eq,
        0x19 => Opcode::Lt,
        0x1A => Opcode::Gt,
        0x1B => Opcode::Neq,
        0x1C => Opcode::Le,
        0x1D => Opcode::Ge,
        0x20 => Opcode::And,
        0x21 => Opcode::Or,
        0x22 => Opcode::Not,
        0x40 => Opcode::Nth,
        0x41 => Opcode::Append,
        0x42 => Opcode::Len,
        0x43 => Opcode::Map,
        0x44 => Opcode::Each,
        0x45 => Opcode::Fold,
        0x46 => Opcode::StrLen,
        0x47 => Opcode::StrCat,
        0x48 => Opcode::StrSlice,
        0x49 => Opcode::StrEq,
        0x4A => Opcode::StrLt,
        0x4B => Opcode::StrFind,
        0x4C => Opcode::StrReplace,
        0x4D => Opcode::StrToI64,
        0x4E => Opcode::I64ToStr,
        0x4F => Opcode::StrNth,
        0xB8 => Opcode::StrChars,
        0xB9 => Opcode::CharsStr,
        0xBA => Opcode::StrIter,
        0xB0 => Opcode::I64ToF64,
        0xB1 => Opcode::F64ToI64,
        0xB2 => Opcode::FSqrt,
        0xB3 => Opcode::FSin,
        0xB4 => Opcode::FCos,
        0xB5 => Opcode::FTan,
        0xB6 => Opcode::JsonParse,
        0xB7 => Opcode::JsonStringify,
        0x53 => Opcode::Times,
        0x61 => Opcode::Return,
        0x71 => Opcode::CapExec,
        0x90 => Opcode::OutputTop,
        0x91 => Opcode::OutputAll,
        0x92 => Opcode::ReadInput,
        _ => Opcode::PushI64(byte as i64),
    }
}
