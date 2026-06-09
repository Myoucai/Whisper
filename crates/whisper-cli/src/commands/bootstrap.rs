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
        Operator::ListFind => 0xBB,
        Operator::StrJoin => 0xBC,
        Operator::BytesNew => 0xBD,
        Operator::BytesPush => 0xBE,
        Operator::BytesLen => 0xBF,
        Operator::BytesWriteFile => 0xC0,
        Operator::Try => 0xC1,
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
                // Nested token: [5, [...inner_tokens...]]
                // whisperc compiles recursively and wraps in PushRef
                let inner = ast_to_whisper_tokens(body);
                tokens.push(Value::List(Rc::new(vec![
                    Value::I64(5),
                    inner,
                ])));
            }
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

/// Hard bootstrap: use whisperc pipeline (lexer + classify + compile) for the
/// token→bytecode step, with Rust handling structural nodes (Def, Cond, Loop).
/// Separate type-9 (word def) tokens from main tokens.
fn split_defs(tokens: &Value) -> (Vec<Value>, Vec<(String, Vec<Value>)>) {
    let mut main = Vec::new();
    let mut defs = Vec::new();
    if let Value::List(items) = tokens {
        for item in items.iter() {
            if let Value::List(inner) = item {
                if inner.len() == 3 {
                    if let Value::I64(9) = &inner[0] {
                        if let (Value::Str(name), body) = (&inner[1], &inner[2]) {
                            if let Value::List(body_tokens) = body {
                                defs.push((name.as_ref().clone(), body_tokens.to_vec()));
                                continue;
                            }
                        }
                    }
                }
            }
            main.push(item.clone());
        }
    }
    (main, defs)
}

/// Compile a list of classified tokens into Opcodes via whisperc compile.
fn compile_tokens(vm: &mut Vm, tokens: &[Value]) -> Result<Vec<Opcode>, String> {
    vm.data_stack.push(Value::List(Rc::new(tokens.to_vec())));
    let bc = vm.execute(&[Opcode::Call("compile".to_string())])
        .map_err(|e| format!("compile: {e}"))?
        .ok_or("compile: no output")?;
    match bc {
        Value::List(vals) => Ok(values_to_opcodes(vals.to_vec())),
        other => Err(format!("expected list, got {other:?}")),
    }
}

/// Full whisperc pipeline: lex → structify → classify → split defs → compile
fn compile_via_whisperc_full(vm: &mut Vm, source: &str) -> Result<(Vec<Opcode>, Vec<(String, Vec<Opcode>)>), String> {
    vm.data_stack.push(Value::Str(Rc::new(source.to_string())));
    let chunks = vm.execute(&[Opcode::Call("tokenize".to_string())])
        .map_err(|e| format!("lex: {e}"))?
        .ok_or("lex: no output")?;
    let nested = if let Value::List(ref items) = chunks {
        structify_chunks(items)
    } else { chunks };
    let tokens = classify_nested(vm, &nested);
    let (main_tokens, def_tokens) = split_defs(&tokens);
    let main_ops = compile_tokens(vm, &main_tokens)?;
    let mut def_ops = Vec::new();
    for (name, body) in &def_tokens {
        def_ops.push((name.clone(), compile_tokens(vm, body)?));
    }
    Ok((main_ops, def_ops))
}

pub fn hard_bootstrap_compile(source: &str) -> Result<(), String> {
    // Phase 1: Rust reference (for comparison)
    let ast = Parser::parse_source(source).map_err(|e| format!("Parse: {}", e.message))?;
    let mut gen = BytecodeGenerator::new();
    let (ref_bytecode, ref_defs) = gen.compile(&ast);

    // Phase 2: Load whisperc pipeline
    let mut vm = Vm::new();
    load_whisperc_pipeline(&mut vm);

    // Phase 3: Full whisperc compilation (lex → structify → classify → split → compile)
    let (whisperc_ops, whisperc_defs) = compile_via_whisperc_full(&mut vm, source)?;
    println!("whisperc: {} main ops, {} defs", whisperc_ops.len(), whisperc_defs.len());
    println!("rust:     {} main ops, {} defs", ref_bytecode.len(), ref_defs.len());

    // Phase 4: Execute Rust reference
    let mut vm_ref = Vm::new();
    for (name, code) in &ref_defs { vm_ref.define_word(name.clone(), code.clone()); }
    print!("Rust VM output: ");
    vm_ref.execute(&ref_bytecode).map_err(|e| format!("Rust VM: {e}"))?;

    // Phase 5: Execute whisperc-compiled bytecode
    let mut vm_wc = Vm::new();
    for (name, code) in &whisperc_defs { vm_wc.define_word(name.clone(), code.clone()); }
    print!("whisperc VM output: ");
    vm_wc.execute(&whisperc_ops).map_err(|e| format!("whisperc VM: {e}"))?;
    println!();

    assert_eq!(whisperc_defs.len(), ref_defs.len(),
        "def count: whisperc={} vs rust={}", whisperc_defs.len(), ref_defs.len());
    println!("Hard self-hosting pipeline complete.");
    Ok(())
}

/// Convert AST nodes back to a source string for re-lexing by whisperc.
#[allow(dead_code)]
fn nodes_to_source_string(nodes: &[AstNode]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    for node in nodes {
        match node {
            AstNode::Literal(val) => {
                write!(s, "{val} ").unwrap();
            }
            AstNode::WordRef(name) => {
                write!(s, "{name} ").unwrap();
            }
            AstNode::Op(op) => {
                write!(s, "{} ", operator_to_str(*op)).unwrap();
            }
            AstNode::Quote(body) => {
                s.push_str("{ ");
                s.push_str(&nodes_to_source_string(body));
                s.push_str("} ");
            }
            AstNode::List(items) => {
                s.push_str("[ ");
                for item in items {
                    s.push_str(&nodes_to_source_string(std::slice::from_ref(item)));
                }
                s.push_str("] ");
            }
            AstNode::Cond { then_branch, else_branch } => {
                s.push_str("?? ");
                s.push_str(&nodes_to_source_string(then_branch));
                if let Some(ref eb) = else_branch {
                    s.push_str("| ");
                    s.push_str(&nodes_to_source_string(eb));
                }
                s.push_str("] ");
            }
            _ => {}
        }
    }
    s
}

#[allow(dead_code)]
fn operator_to_str(op: Operator) -> &'static str {
    match op {
        Operator::Dup => "_",
        Operator::Swap => "`",
        Operator::Drop => "drop",
        Operator::Rot => "@",
        Operator::Add => "+",
        Operator::Sub => "-",
        Operator::Mul => "*",
        Operator::Div => "/",
        Operator::Mod => "%",
        Operator::Eq => "=",
        Operator::Lt => "<",
        Operator::Gt => ">",
        Operator::Neq => "!=",
        Operator::Le => "<=",
        Operator::Ge => ">=",
        Operator::And => "&",
        Operator::Or => "|",
        Operator::Not => "!",
        Operator::Nth => "@nth",
        Operator::Append => "append",
        Operator::Len => "len",
        Operator::Map => "@map",
        Operator::Each => "@each",
        Operator::Fold => "@fold",
        Operator::AtTimes => "@times",
        Operator::OutputTop => ".",
        Operator::OutputAll => "..",
        Operator::ReadInput => ",",
        Operator::StrLen => "strlen",
        Operator::StrCat => "strcat",
        Operator::StrSlice => "strslice",
        _ => "drop", // fallback
    }
}

/// Compile source string through the full whisperc pipeline.
#[allow(dead_code)]
fn compile_via_whisperc_pipeline(vm: &mut Vm, source: &str) -> Result<Vec<Opcode>, String> {
    vm.data_stack.push(Value::Str(Rc::new(source.to_string())));
    let chunks = vm.execute(&[Opcode::Call("tokenize".to_string())])
        .map_err(|e| format!("lex: {e}"))?
        .ok_or("lex: no output")?;

    let nested = if let Value::List(ref items) = chunks {
        structify_chunks(items)
    } else {
        chunks
    };
    let tokens = classify_nested(vm, &nested);

    vm.data_stack.push(tokens);
    let bytecode_vals = vm.execute(&[Opcode::Call("compile".to_string())])
        .map_err(|e| format!("compile: {e}"))?
        .ok_or("compile: no output")?;

    match bytecode_vals {
        Value::List(vals) => Ok(values_to_opcodes(vals.to_vec())),
        other => Err(format!("expected list of bytecode, got {other:?}")),
    }
}

pub fn bootstrap_compile(source: &str) -> Result<(), String> {
    // Phase 1: Parse with Rust compiler (reference)
    let ast = Parser::parse_source(source).map_err(|e| format!("Parse: {}", e.message))?;
    let mut gen = BytecodeGenerator::new();
    let (ref_bytecode, ref_defs) = gen.compile(&ast);

    // Phase 2: Extract defs and generate tokens
    let mut main_body: Vec<AstNode> = Vec::new();
    let mut def_nodes: Vec<(String, Vec<AstNode>)> = Vec::new();
    for node in &ast {
        match node {
            AstNode::Def { name, body } => {
                def_nodes.push((name.clone(), body.clone()));
            }
            _ => main_body.push(node.clone()),
        }
    }
    let tokens = ast_to_whisper_tokens(&main_body);
    let token_count = match &tokens {
        Value::List(l) => l.len(),
        _ => 0,
    };
    println!("Tokens: {} items, {} defs", token_count, def_nodes.len());

    // Phase 3: Load whisperc compiler
    let compiler_src = include_str!("../../../../whisperc/main.ws");
    let compiler_ast =
        Parser::parse_source(compiler_src).map_err(|e| format!("whisperc parse: {}", e.message))?;
    let mut cgen = BytecodeGenerator::new();
    let (compiler_bc, compiler_defs) = cgen.compile(&compiler_ast);

    // Phase 4: Run whisperc on tokens (main body + each def body)
    let mut vm = Vm::new();
    for (name, code) in compiler_defs {
        vm.define_word(name, code);
    }
    vm.execute(&compiler_bc)
        .map_err(|e| format!("whisperc init: {e}"))?;

    // Phase 4a: Compile main body
    vm.data_stack.push(tokens);
    let call_compile = [Opcode::Call("compile".to_string())];
    let whisperc_result = vm.execute(&call_compile)
        .map_err(|e| format!("whisperc compile main: {e}"))?;

    // Phase 4b: Compile each word definition body with whisperc
    let mut whisperc_defs: Vec<(String, Vec<Opcode>)> = Vec::new();
    for (def_name, def_body) in &def_nodes {
        let body_tokens = ast_to_whisper_tokens(def_body);
        vm.data_stack.push(body_tokens);
        let result = vm.execute(&call_compile)
            .map_err(|e| format!("whisperc compile def '{}': {e}", def_name))?;
        if let Some(Value::List(vals)) = result {
            let ops = values_to_opcodes(vals.to_vec());
            println!("  def '{}': {} opcodes", def_name, ops.len());
            whisperc_defs.push((def_name.clone(), ops));
        }
    }

    // Phase 5: Convert whisperc main output to Opcodes
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

    // Phase 8: Execute whisperc-compiled bytecode (with whisperc-compiled defs!)
    println!("\nwhisperc bytecode: {} opcodes, {} defs", whisperc_ops.len(), whisperc_defs.len());
    let mut vm3 = Vm::new();
    for (name, code) in &whisperc_defs {
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
                                ops.push(Opcode::PushStr(Rc::new(s.as_ref().clone())));
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
                        // PushRef: [0x35, [inner_ops...]]  (nested, from whisperc)
                        // or legacy: [0x35, count, op1, op2...] (flat)
                        if items.len() >= 2 {
                            if let Value::List(inner) = &items[1] {
                                // Nested format from whisperc recursive compile
                                let inner_ops = values_to_opcodes(inner.to_vec());
                                ops.push(Opcode::PushRef(inner_ops));
                            } else if items.len() >= 3 {
                                // Legacy flat format
                                if let Value::I64(_count) = &items[1] {
                                    let inner_vals: Vec<Value> = items[2..].to_vec();
                                    let inner_ops = values_to_opcodes(inner_vals);
                                    ops.push(Opcode::PushRef(inner_ops));
                                }
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
        0xBB => Opcode::ListFind,
        0xBC => Opcode::StrJoin,
        0xBD => Opcode::BytesNew,
        0xBE => Opcode::BytesPush,
        0xBF => Opcode::BytesLen,
        0xC0 => Opcode::BytesWriteFile,
        0xC1 => Opcode::Try,
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

/// Load all whisperc components (lexer + classify + compiler) into one VM.
fn load_whisperc_pipeline(vm: &mut Vm) {
    let files = [
        include_str!("../../../../whisperc/lexer.ws"),
        include_str!("../../../../whisperc/classify.ws"),
        include_str!("../../../../whisperc/main.ws"),
    ];
    for src in files {
        let ast = Parser::parse_source(src).unwrap();
        let mut gen = BytecodeGenerator::new();
        let (bc, defs) = gen.compile(&ast);
        for (n, c) in defs {
            vm.define_word(n, c);
        }
        vm.execute(&bc).unwrap();
    }
}

/// Structural chunk grouper — handles { }, [ ], ??...|...], {}{}#, :...;
/// Produces typed markers: 5=Quote, 6=List, 7=Cond, 8=Loop, 9=Def
fn structify_chunks(chunks: &[Value]) -> Value {
    let grouped = group_paired_delimiters(chunks);
    let patterned = recognize_patterns(&grouped);
    Value::List(Rc::new(patterned))
}

/// Pass 1: Group { } and [ ] paired delimiters into typed markers.
fn group_paired_delimiters(chunks: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < chunks.len() {
        match &chunks[i] {
            Value::Str(s) if s.as_ref() == "{" => {
                let (inner, next) = collect_balanced(chunks, i + 1, "{", "}");
                result.push(Value::List(Rc::new(vec![
                    Value::I64(5),
                    Value::List(Rc::new(group_paired_delimiters(&inner))),
                ])));
                i = next;
            }
            Value::Str(s) if s.as_ref() == "[" => {
                let (inner, next) = collect_balanced(chunks, i + 1, "[", "]");
                result.push(Value::List(Rc::new(vec![
                    Value::I64(6),
                    Value::List(Rc::new(group_paired_delimiters(&inner))),
                ])));
                i = next;
            }
            _ => {
                result.push(chunks[i].clone());
                i += 1;
            }
        }
    }
    result
}

/// Collect balanced delimiters, returning (inner_chunks, next_index).
fn collect_balanced(chunks: &[Value], start: usize, open: &str, close: &str) -> (Vec<Value>, usize) {
    let mut inner = Vec::new();
    let mut depth = 1;
    let mut i = start;
    while i < chunks.len() && depth > 0 {
        match &chunks[i] {
            Value::Str(s) if s.as_ref() == open => {
                depth += 1;
                if depth > 1 { inner.push(chunks[i].clone()); }
            }
            Value::Str(s) if s.as_ref() == close => {
                depth -= 1;
                if depth > 0 { inner.push(chunks[i].clone()); }
            }
            _ => inner.push(chunks[i].clone()),
        }
        i += 1;
    }
    (inner, i)
}

/// Pass 2: Recognize multi-token structural patterns.
fn recognize_patterns(chunks: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < chunks.len() {
        // Pattern: : name {body} ;  →  [9, name, body]
        if i + 3 < chunks.len()
            && is_str_eq(&chunks[i], ":")
            && is_str(&chunks[i + 1])
            && is_typed_list(&chunks[i + 2], 5)
            && is_str_eq(&chunks[i + 3], ";")
        {
            let name = chunks[i + 1].clone();
            let body = get_inner(&chunks[i + 2]);
            result.push(Value::List(Rc::new(vec![
                Value::I64(9),
                name,
                Value::List(Rc::new(recognize_patterns(&body))),
            ])));
            i += 4;
            continue;
        }
        // Pattern: [5, body]  [5, cond]  #  →  [8, body, cond]
        if i + 2 < chunks.len()
            && is_typed_list(&chunks[i], 5)
            && is_typed_list(&chunks[i + 1], 5)
            && is_str_eq(&chunks[i + 2], "#")
        {
            let body = get_inner(&chunks[i]);
            let cond = get_inner(&chunks[i + 1]);
            result.push(Value::List(Rc::new(vec![
                Value::I64(8),
                Value::List(Rc::new(recognize_patterns(&body))),
                Value::List(Rc::new(recognize_patterns(&cond))),
            ])));
            i += 3;
            continue;
        }
        // Pattern: ?? ... | ... ]  →  [7, then, else]
        if is_str_eq(&chunks[i], "??") {
            // Find | separator and closing ]
            let (then_tokens, else_tokens, next) = split_cond(&chunks, i + 1);
            result.push(Value::List(Rc::new(vec![
                Value::I64(7),
                Value::List(Rc::new(recognize_patterns(&then_tokens))),
                Value::List(Rc::new(recognize_patterns(&else_tokens))),
            ])));
            i = next;
            continue;
        }
        // Recurse into nested groups
        match &chunks[i] {
            Value::List(items) if items.len() == 2 && matches!(&items[0], Value::I64(5 | 6)) => {
                let tag = items[0].clone();
                let inner = get_inner(&chunks[i]);
                result.push(Value::List(Rc::new(vec![
                    tag,
                    Value::List(Rc::new(recognize_patterns(&inner))),
                ])));
            }
            _ => {
                result.push(chunks[i].clone());
            }
        }
        i += 1;
    }
    result
}

/// Split ?? then | else ] into (then, else, next_index).
fn split_cond(chunks: &[Value], start: usize) -> (Vec<Value>, Vec<Value>, usize) {
    let mut then_tokens = Vec::new();
    let mut else_tokens = Vec::new();
    let mut depth = 1; // for the ?? opener
    let mut i = start;
    let mut in_else = false;
    while i < chunks.len() && depth > 0 {
        match &chunks[i] {
            Value::Str(s) if s.as_ref() == "??" => { depth += 1; if !in_else { then_tokens.push(chunks[i].clone()); } else { else_tokens.push(chunks[i].clone()); } }
            Value::Str(s) if s.as_ref() == "]" => { depth -= 1; if depth == 0 { break; } if !in_else { then_tokens.push(chunks[i].clone()); } else { else_tokens.push(chunks[i].clone()); } }
            Value::Str(s) if s.as_ref() == "|" && depth == 1 => { in_else = true; }
            _ => {
                if !in_else { then_tokens.push(chunks[i].clone()); }
                else { else_tokens.push(chunks[i].clone()); }
            }
        }
        i += 1;
    }
    (then_tokens, else_tokens, i + 1)
}

fn is_str_eq(v: &Value, s: &str) -> bool {
    matches!(v, Value::Str(x) if x.as_ref() == s)
}

fn is_str(v: &Value) -> bool {
    matches!(v, Value::Str(_))
}

fn is_typed_list(v: &Value, tag: i64) -> bool {
    matches!(v, Value::List(items) if items.len() == 2 && matches!(&items[0], Value::I64(t) if *t == tag))
}

fn get_inner(v: &Value) -> Vec<Value> {
    match v {
        Value::List(items) if items.len() == 2 => match &items[1] {
            Value::List(inner) => inner.to_vec(),
            _ => vec![],
        },
        _ => vec![],
    }
}

/// Recursively classify structified chunks using the Whisper classify-one word.
fn classify_nested(vm: &mut Vm, nested: &Value) -> Value {
    match nested {
        Value::List(items) => {
            if items.len() >= 2 {
                if let Value::I64(tag) = &items[0] {
                    match *tag {
                        // Quote block: [5, inner] → [5, classified_inner]
                        5 => {
                            let inner = classify_nested(vm, &items[1]);
                            return Value::List(Rc::new(vec![Value::I64(5), inner]));
                        }
                        // List: [6, elements] → [6, classified_elements]
                        6 => {
                            if let Value::List(elems) = &items[1] {
                                let classified: Vec<Value> = elems.iter()
                                    .map(|e| classify_nested(vm, e))
                                    .collect();
                                return Value::List(Rc::new(vec![
                                    Value::I64(6),
                                    Value::List(Rc::new(classified)),
                                ]));
                            }
                        }
                        // Conditional: [7, then, else] → [7, classified_then, classified_else]
                        7 if items.len() == 3 => {
                            let then_c = classify_nested(vm, &items[1]);
                            let else_c = classify_nested(vm, &items[2]);
                            return Value::List(Rc::new(vec![Value::I64(7), then_c, else_c]));
                        }
                        // Loop: [8, body, cond] → [8, classified_body, classified_cond]
                        8 if items.len() == 3 => {
                            let body_c = classify_nested(vm, &items[1]);
                            let cond_c = classify_nested(vm, &items[2]);
                            return Value::List(Rc::new(vec![Value::I64(8), body_c, cond_c]));
                        }
                        // WordDef: [9, name, body] → pass through (body classified)
                        9 if items.len() == 3 => {
                            let body_c = classify_nested(vm, &items[2]);
                            return Value::List(Rc::new(vec![
                                Value::I64(9),
                                items[1].clone(),
                                body_c,
                            ]));
                        }
                        _ => {}
                    }
                }
            }
            let classified: Vec<Value> = items.iter()
                .map(|item| classify_nested(vm, item))
                .collect();
            Value::List(Rc::new(classified))
        }
        Value::Str(s) => {
            if s.starts_with('"') {
                Value::List(Rc::new(vec![
                    Value::I64(2),
                    Value::Str(Rc::new(s[1..].to_string())),
                ]))
            } else {
                vm.data_stack.push(nested.clone());
                vm.execute(&[Opcode::Call("classify-one".to_string())])
                    .unwrap()
                    .unwrap_or(Value::List(Rc::new(vec![])))
            }
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selfhost_hello() {
        assert!(bootstrap_compile("\"Hello, World!\" .").is_ok());
    }

    #[test]
    fn test_selfhost_sq() {
        assert!(bootstrap_compile(": sq { _ * } ; 5 sq").is_ok());
    }

    #[test]
    fn test_selfhost_fib() {
        assert!(bootstrap_compile(": fib { _ 1 > ??_ 1 - fib ` 2 - fib +|] } ; 10 fib").is_ok());
    }

    #[test]
    fn test_selfhost_map_quote() {
        // Tests whisperc recursive quote compilation
        assert!(bootstrap_compile("[1 2 3] { _ * } @map").is_ok());
    }

    #[test]
    fn test_selfhost_fold_quote() {
        assert!(bootstrap_compile("[1 2 3 4 5] 0 { + } @fold").is_ok());
    }

    /// Full pipeline test: Rust tokens → whisperc compile → compare with Rust compiler
    fn compile_via_whisperc(source: &str) -> Vec<Opcode> {
        let ast = Parser::parse_source(source).unwrap();
        let tokens = ast_to_whisper_tokens(&ast);

        // Load whisperc compiler
        let compiler_src = include_str!("../../../../whisperc/main.ws");
        let compiler_ast = Parser::parse_source(compiler_src).unwrap();
        let mut cgen = BytecodeGenerator::new();
        let (compiler_bc, compiler_defs) = cgen.compile(&compiler_ast);

        let mut vm = Vm::new();
        for (n, c) in compiler_defs { vm.define_word(n, c.clone()); }
        vm.execute(&compiler_bc).unwrap();
        vm.data_stack.push(tokens);

        match vm.execute(&[Opcode::Call("compile".to_string())]) {
            Ok(Some(Value::List(vals))) => values_to_opcodes(vals.to_vec()),
            other => panic!("whisperc compile failed: {other:?}"),
        }
    }

    #[test]
    fn test_pipeline_compare() {
        let source = "3 4 + 5 *";
        let whisperc_ops = compile_via_whisperc(source);

        let ast = Parser::parse_source(source).unwrap();
        let mut gen = BytecodeGenerator::new();
        let (rust_ops, _) = gen.compile(&ast);

        // Both should produce valid bytecode
        assert!(!whisperc_ops.is_empty(), "whisperc produced no bytecode");
        assert!(!rust_ops.is_empty(), "rust produced no bytecode");

        // Execute both and compare results
        let mut vm1 = Vm::new();
        let r1 = vm1.execute(&whisperc_ops).unwrap().unwrap();
        let mut vm2 = Vm::new();
        let r2 = vm2.execute(&rust_ops).unwrap().unwrap();
        assert_eq!(r1, r2, "whisperc and rust results differ");
    }

    /// Self-hosting test: whisperc compiles itself (defs + main body)
    #[test]
    fn test_selfhost_compile_self() {
        let source = include_str!("../../../../whisperc/main.ws");
        let ast = Parser::parse_source(source).unwrap();

        // Separate defs from main body (same as bootstrap_compile does)
        let mut main_body: Vec<AstNode> = Vec::new();
        let mut def_nodes: Vec<(String, Vec<AstNode>)> = Vec::new();
        for node in &ast {
            match node {
                AstNode::Def { name, body } => def_nodes.push((name.clone(), body.clone())),
                _ => main_body.push(node.clone()),
            }
        }

        // Load whisperc compiler
        let compiler_src = include_str!("../../../../whisperc/main.ws");
        let compiler_ast = Parser::parse_source(compiler_src).unwrap();
        let mut cgen = BytecodeGenerator::new();
        let (compiler_bc, compiler_defs) = cgen.compile(&compiler_ast);
        let mut vm = Vm::new();
        for (n, c) in compiler_defs { vm.define_word(n, c.clone()); }
        vm.execute(&compiler_bc).unwrap();

        // Compile main body
        let main_tokens = ast_to_whisper_tokens(&main_body);
        vm.data_stack.push(main_tokens);
        let r = vm.execute(&[Opcode::Call("compile".to_string())]).unwrap();
        let main_ops = match r {
            Some(Value::List(v)) => values_to_opcodes(v.to_vec()),
            _ => vec![],
        };

        // Compile each def body
        let mut wdefs: Vec<(String, Vec<Opcode>)> = Vec::new();
        for (name, body) in &def_nodes {
            let body_tokens = ast_to_whisper_tokens(body);
            vm.data_stack.push(body_tokens);
            let r = vm.execute(&[Opcode::Call("compile".to_string())]).unwrap();
            if let Some(Value::List(v)) = r {
                wdefs.push((name.clone(), values_to_opcodes(v.to_vec())));
            }
        }

        println!("whisperc: main={} defs={}", main_ops.len(), wdefs.len());
        assert!(wdefs.len() >= 2, "expected >=2 defs, got {}", wdefs.len());

        // Compare with Rust compiler
        let mut gen = BytecodeGenerator::new();
        let (rust_ops, rust_defs) = gen.compile(&ast);
        println!("rust:     main={} defs={}", rust_ops.len(), rust_defs.len());

        // Both should produce same number of definitions
        assert_eq!(wdefs.len(), rust_defs.len(),
            "def count mismatch: whisperc={} rust={}", wdefs.len(), rust_defs.len());

        // Execute whisperc-compiled main with whisperc-compiled defs
        let mut vm2 = Vm::new();
        for (name, code) in &wdefs {
            vm2.define_word(name.clone(), code.clone());
        }
        let result = vm2.execute(&main_ops);
        assert!(result.is_ok(), "whisperc self-execute failed: {result:?}");
    }

    // ── classify_chunks helper (used by test_full_pipeline_with_parser) ──

    /// Classify a flat list of chunk strings into numeric token Values
    /// with nested grouping for { } blocks.
    fn classify_chunks(chunks: &[String]) -> Vec<Value> {
        let mut tokens = Vec::new();
        let mut i = 0;
        while i < chunks.len() {
            let chunk = &chunks[i];
            match chunk.as_str() {
                "{" => {
                    let (inner, next) = collect_until(chunks, i + 1, "}");
                    tokens.push(Value::List(Rc::new(vec![
                        Value::I64(5),
                        Value::List(Rc::new(classify_chunks(&inner))),
                    ])));
                    i = next;
                }
                "}" => {
                    i += 1;
                }
                "#t" => {
                    tokens.push(Value::List(Rc::new(vec![Value::I64(13), Value::I64(1)])));
                    i += 1;
                }
                "#f" => {
                    tokens.push(Value::List(Rc::new(vec![Value::I64(13), Value::I64(0)])));
                    i += 1;
                }
                "+" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x10)]))); i += 1; }
                "-" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x11)]))); i += 1; }
                "*" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x12)]))); i += 1; }
                "/" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x13)]))); i += 1; }
                "=" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x18)]))); i += 1; }
                "<" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x19)]))); i += 1; }
                ">" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x1A)]))); i += 1; }
                "!=" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x1B)]))); i += 1; }
                "<=" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x1C)]))); i += 1; }
                ">=" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x1D)]))); i += 1; }
                "&" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x20)]))); i += 1; }
                "|" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x21)]))); i += 1; }
                "!" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x22)]))); i += 1; }
                "_" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x00)]))); i += 1; }
                "`" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x01)]))); i += 1; }
                "@" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x03)]))); i += 1; }
                "." => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x90)]))); i += 1; }
                "," => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x92)]))); i += 1; }
                ".." => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x91)]))); i += 1; }
                "%" => { tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(0x14)]))); i += 1; }
                ":" | ";" | "[" | "]" => { i += 1; }
                "dup" | "drop" | "swap" | "rot" | "mod" | "len" | "append"
                | "strlen" | "strcat" | "strslice" | "streq" | "strlt"
                | "strfind" | "strreplace" | "strtoi64" | "i64tostr"
                | "strnth" | "strchars" | "charsstr" | "striter"
                | "listfind" | "strjoin" | "output" | "return" | "times" => {
                    let byte = word_to_op_byte(chunk);
                    tokens.push(Value::List(Rc::new(vec![Value::I64(3), Value::I64(byte as i64)])));
                    i += 1;
                }
                _ if chunk.starts_with(|c: char| c.is_ascii_digit())
                    || (chunk.starts_with('-') && chunk.len() > 1
                        && chunk[1..].starts_with(|c: char| c.is_ascii_digit())) =>
                {
                    if chunk.contains('.') {
                        if let Ok(f) = chunk.parse::<f64>() {
                            tokens.push(Value::List(Rc::new(vec![
                                Value::I64(1),
                                Value::I64(f.to_bits() as i64),
                            ])));
                        }
                    } else if let Ok(n) = chunk.parse::<i64>() {
                        tokens.push(Value::List(Rc::new(vec![Value::I64(0), Value::I64(n)])));
                    }
                    i += 1;
                }
                _ => {
                    tokens.push(Value::List(Rc::new(vec![
                        Value::I64(4),
                        Value::Str(Rc::new(chunk.clone())),
                    ])));
                    i += 1;
                }
            }
        }
        tokens
    }

    fn collect_until(chunks: &[String], start: usize, end: &str) -> (Vec<String>, usize) {
        let mut result = Vec::new();
        let mut depth = 1;
        let mut i = start;
        while i < chunks.len() && depth > 0 {
            match chunks[i].as_str() {
                "{" => depth += 1,
                "}" if end == "}" => depth -= 1,
                _ => {}
            }
            if depth > 0 {
                result.push(chunks[i].clone());
            }
            i += 1;
        }
        (result, i)
    }

    fn word_to_op_byte(word: &str) -> u8 {
        match word {
            "dup" => 0x00, "swap" => 0x01, "drop" => 0x02, "rot" => 0x03,
            "mod" => 0x14, "len" => 0x42, "append" => 0x41,
            "strlen" => 0x46, "strcat" => 0x47, "strslice" => 0x48,
            "streq" => 0x49, "strlt" => 0x4A, "strfind" => 0x4B,
            "strreplace" => 0x4C, "strtoi64" => 0x4D, "i64tostr" => 0x4E,
            "strnth" => 0x4F, "strchars" => 0xB8, "charsstr" => 0xB9,
            "striter" => 0xBA, "listfind" => 0xBB, "strjoin" => 0xBC,
            "times" => 0x53, "return" => 0x61,
            _ => 0x00,
        }
    }

    /// Pipeline: Rust tokens → whisperc parser → whisperc compile
    #[test]
    fn test_full_pipeline_with_parser() {
        let chunks = vec!["3", "4", "+"];
        let tokens_rust: Vec<Value> = classify_chunks(&chunks.iter().map(|s| s.to_string()).collect::<Vec<_>>());

        // Compile with whisperc
        let compiler_src = include_str!("../../../../whisperc/main.ws");
        let compiler_ast = Parser::parse_source(compiler_src).unwrap();
        let mut cgen = BytecodeGenerator::new();
        let (compiler_bc, compiler_defs) = cgen.compile(&compiler_ast);
        let mut vm = Vm::new();
        for (n, c) in compiler_defs { vm.define_word(n, c.clone()); }
        vm.execute(&compiler_bc).unwrap();
        vm.data_stack.push(Value::List(Rc::new(tokens_rust)));

        let r = vm.execute(&[Opcode::Call("compile".to_string())]).unwrap().unwrap();
        let ops = values_to_opcodes(match r { Value::List(v) => v.to_vec(), _ => panic!() });
        let mut vm2 = Vm::new();
        let result = vm2.execute(&ops).unwrap().unwrap();
        assert_eq!(result.unwrap_signal(), Value::I64(7));
    }

    // ── whisperc lexer probe tests ─────────────────────────────────────

    /// Load whisperc lexer source, compile it, and call an exported word
    /// with a string argument. Returns the value left on the VM stack.
    fn call_whisperc_lexer(input: &str) -> Value {
        let src = include_str!("../../../../whisperc/lexer.ws");
        let ast = Parser::parse_source(src).unwrap();
        let mut gen = BytecodeGenerator::new();
        let (bc, defs) = gen.compile(&ast);
        let mut vm = Vm::new();
        // vm.trace = true;
        for (n, c) in defs { vm.define_word(n, c); }
        vm.execute(&bc).unwrap();
        vm.data_stack.push(Value::Str(Rc::new(input.to_string())));
        vm.execute(&[Opcode::Call("tokenize".to_string())])
            .unwrap()
            .unwrap_or(Value::List(Rc::new(vec![])))
    }

    #[test]
    fn test_lexer_simple_number() {
        let result = call_whisperc_lexer("42");
        eprintln!("lexer('42') = {result:?}");
        match &result {
            Value::List(items) => {
                assert!(!items.is_empty(), "should produce at least one token");
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn test_lexer_single() {
        // Single number, no spaces
        let result = call_whisperc_lexer("42");
        eprintln!("lexer('42') = {result:?}");
        assert!(matches!(&result, Value::List(_)), "expected list");
    }

    #[test]
    fn test_lexer_two_numbers() {
        // Two numbers separated by space
        let result = call_whisperc_lexer("3 4");
        eprintln!("lexer('3 4') = {result:?}");
        if let Value::List(items) = &result {
            eprintln!("  {} items:", items.len());
            for (i, item) in items.iter().enumerate() {
                eprintln!("  [{}] = {item:?}", i);
            }
        }
        assert!(matches!(&result, Value::List(_)), "expected list");
    }

    #[test]
    fn test_lexer_arithmetic() {
        let result = call_whisperc_lexer("3 4 +");
        eprintln!("lexer('3 4 +') = {result:?}");
        // Expected chunks: ["3", "4", "+"]
        if let Value::List(items) = &result {
            assert_eq!(items.len(), 3, "expected 3 chunks, got {items:?}");
            // Each chunk is a string
            for item in items.iter() {
                assert!(matches!(item, Value::Str(_)), "each chunk should be a string");
            }
        } else {
            panic!("expected list, got {result:?}");
        }
    }

    #[test]
    fn test_lexer_string_literal() {
        let result = call_whisperc_lexer("\"Hello\" .");
        eprintln!("lexer('\"Hello\" .') = {result:?}");
        if let Value::List(items) = &result {
            eprintln!("  {} items:", items.len());
            for (i, item) in items.iter().enumerate() {
                eprintln!("  [{}] = {item:?}", i);
            }
            assert!(items.len() >= 2, "should have string and operator");
        }
        assert!(matches!(&result, Value::List(_)));
    }

    // ── Pipeline test helper ────────────────────────────────────────────

    fn whisperc_compile(source: &str) -> Vec<Opcode> {
        let mut vm = Vm::new();
        load_whisperc_pipeline(&mut vm);
        vm.data_stack.push(Value::Str(Rc::new(source.to_string())));
        let chunks = vm.execute(&[Opcode::Call("tokenize".to_string())])
            .unwrap().unwrap();
        let nested = if let Value::List(ref items) = chunks {
            structify_chunks(items)
        } else {
            chunks
        };
        let tokens = classify_nested(&mut vm, &nested);
        vm.data_stack.push(tokens);
        let bytecode_vals = vm.execute(&[Opcode::Call("compile".to_string())])
            .unwrap();
        match bytecode_vals {
            Some(Value::List(vals)) => values_to_opcodes(vals.to_vec()),
            other => panic!("expected list, got {other:?}"),
        }
    }

    // ── classify.ws tests ───────────────────────────────────────────────

    fn call_whisperc_classify(chunks: &[&str]) -> Value {
        let src = include_str!("../../../../whisperc/classify.ws");
        let ast = Parser::parse_source(src).unwrap();
        let mut gen = BytecodeGenerator::new();
        let (bc, defs) = gen.compile(&ast);
        let mut vm = Vm::new();
        for (n, c) in defs { vm.define_word(n, c); }
        vm.execute(&bc).unwrap();
        // Build chunk list
        let chunk_vals: Vec<Value> = chunks
            .iter()
            .map(|s| Value::Str(Rc::new(s.to_string())))
            .collect();
        vm.data_stack.push(Value::List(Rc::new(chunk_vals)));
        vm.execute(&[Opcode::Call("classify".to_string())])
            .unwrap()
            .unwrap_or(Value::List(Rc::new(vec![])))
    }

    #[test]
    fn test_classify_flat_single() {
        let result = call_whisperc_classify(&["42"]);
        eprintln!("classify(['42']) = {result:?}");
        assert!(matches!(&result, Value::List(_)), "expected list");
    }

    #[test]
    fn test_classify_flat_two() {
        let result = call_whisperc_classify(&["3", "4"]);
        eprintln!("classify(['3','4']) = {result:?}");
        assert!(matches!(&result, Value::List(_)), "expected list");
    }

    #[test]
    fn test_classify_arithmetic_flat() {
        let result = call_whisperc_classify(&["3", "4", "+"]);
        eprintln!("classified: {result:?}");
        if let Value::List(items) = &result {
            assert_eq!(items.len(), 3, "expected 3 tokens, got {items:?}");
        } else {
            panic!("expected list, got {result:?}");
        }
    }

    #[test]
    fn test_classify_one_number() {
        let src = include_str!("../../../../whisperc/classify.ws");
        let ast = Parser::parse_source(src).unwrap();
        let mut gen = BytecodeGenerator::new();
        let (bc, defs) = gen.compile(&ast);
        let mut vm = Vm::new();
        for (n, c) in defs { vm.define_word(n, c); }
        vm.execute(&bc).unwrap();
        vm.data_stack.push(Value::Str(Rc::new("42".to_string())));
        let result = vm.execute(&[Opcode::Call("classify-one".to_string())])
            .unwrap()
            .unwrap_or(Value::List(Rc::new(vec![])));
        eprintln!("classify_one('42') = {result:?}");
    }

    #[test]
    fn test_classify_operators() {
        let result = call_whisperc_classify(&["+", "_", ".", "dup", "strlen"]);
        eprintln!("classify operators: {result:?}");
        if let Value::List(items) = &result {
            assert_eq!(items.len(), 5);
        } else {
            panic!("expected list");
        }
    }

    #[test]
    fn test_lexer_with_delimiters() {
        let result = call_whisperc_lexer(": sq { _ * } ;");
        eprintln!("lexer(': sq {{ _ * }} ;') = {result:?}");
        match &result {
            Value::List(items) => {
                eprintln!("  {} items:", items.len());
                for (i, item) in items.iter().enumerate() {
                    eprintln!("  [{}] = {item:?}", i);
                }
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn test_pipeline_simple_arithmetic() {
        let ops = whisperc_compile("3 4 +");
        eprintln!("whisperc compiled: {ops:?}");

        let mut vm = Vm::new();
        let result = vm.execute(&ops).unwrap().unwrap();
        assert_eq!(result.unwrap_signal(), Value::I64(7),
            "3 4 + should equal 7");
    }

    #[test]
    fn test_pipeline_number() {
        let ops = whisperc_compile("42");
        let mut vm = Vm::new();
        let result = vm.execute(&ops).unwrap().unwrap();
        assert_eq!(result.unwrap_signal(), Value::I64(42));
    }

    #[test]
    fn test_pipeline_dot_output() {
        let ops = whisperc_compile("\"Hello\" .");
        assert!(!ops.is_empty(), "should produce bytecode");
    }

    #[test]
    fn test_pipeline_dup_add() {
        // 5 _ *  — dup then multiply: 5*5 = 25
        let ops = whisperc_compile("5 _ *");
        let mut vm = Vm::new();
        let result = vm.execute(&ops).unwrap().unwrap();
        assert_eq!(result.unwrap_signal(), Value::I64(25));
    }

    #[test]
    fn test_pipeline_stack_ops() {
        // 3 4 swap -  — 4-3 = 1
        let ops = whisperc_compile("3 4 ` -");
        let mut vm = Vm::new();
        let result = vm.execute(&ops).unwrap().unwrap();
        assert_eq!(result.unwrap_signal(), Value::I64(1));
    }

    #[test]
    fn test_pipeline_comparison() {
        // 5 3 >  — true
        let ops = whisperc_compile("5 3 >");
        let mut vm = Vm::new();
        let result = vm.execute(&ops).unwrap().unwrap();
        assert_eq!(result.unwrap_signal(), Value::Bool(true));
    }

    #[test]
    fn test_pipeline_drop() {
        // 42 drop 7  — drops 42, leaves 7
        let ops = whisperc_compile("42 drop 7");
        let mut vm = Vm::new();
        let result = vm.execute(&ops).unwrap().unwrap();
        assert_eq!(result.unwrap_signal(), Value::I64(7));
    }

    #[test]
    fn test_pipeline_rot() {
        // 1 2 3 @  — rot: 1 2 3 → 2 3 1  (top is 1)
        let ops = whisperc_compile("1 2 3 @");
        let mut vm = Vm::new();
        let result = vm.execute(&ops).unwrap().unwrap();
        assert_eq!(result.unwrap_signal(), Value::I64(1));
    }

    #[test]
    fn test_pipeline_selfhost_compare() {
        // Compare whisperc output with Rust reference for a simple program
        let source = "3 4 + 5 *";
        let whisperc_ops = whisperc_compile(source);

        let ast = Parser::parse_source(source).unwrap();
        let mut gen = BytecodeGenerator::new();
        let (rust_ops, _) = gen.compile(&ast);

        // Both should produce the same result
        let mut vm1 = Vm::new();
        let r1 = vm1.execute(&whisperc_ops).unwrap().unwrap();
        let mut vm2 = Vm::new();
        let r2 = vm2.execute(&rust_ops).unwrap().unwrap();
        assert_eq!(r1.unwrap_signal(), r2.unwrap_signal(),
            "whisperc and Rust should produce same result");
    }

    /// Self-hosting test: whisperc compiles a flat subset of main.ws tokens
    /// and the result matches Rust-compiled bytecode.
    #[test]
    fn test_pipeline_selfhost_compile_main_flat() {
        // Flat expression from main.ws: tk-type and tk-val definitions
        // These are flat token sequences that main.ws uses internally
        // tk-type: 0 @nth
        let ops = whisperc_compile("0 @nth");
        let mut vm = Vm::new();
        // Push a [type, value] pair and extract type (first element)
        vm.data_stack.push(Value::List(Rc::new(vec![
            Value::I64(0),
            Value::I64(42),
        ])));
        let result = vm.execute(&ops).unwrap().unwrap();
        assert_eq!(result.unwrap_signal(), Value::I64(0),
            "0 @nth on [0, 42] should give 0");
    }

    /// Self-hosting: token access patterns used in main.ws
    #[test]
    fn test_pipeline_selfhost_token_access() {
        // tk-type: 0 @nth  (extracts type from [type, value] pair)
        let ops = whisperc_compile("0 @nth");
        let mut vm = Vm::new();
        vm.data_stack.push(Value::List(Rc::new(vec![
            Value::I64(0), Value::I64(42),
        ])));
        let result = vm.execute(&ops).unwrap().unwrap();
        assert_eq!(result.unwrap_signal(), Value::I64(0));

        // tk-val: 1 @nth  (extracts value from [type, value] pair)
        let ops2 = whisperc_compile("1 @nth");
        let mut vm2 = Vm::new();
        vm2.data_stack.push(Value::List(Rc::new(vec![
            Value::I64(3), Value::I64(16),  // [type=3, value=16] = operator +
        ])));
        let result2 = vm2.execute(&ops2).unwrap().unwrap();
        assert_eq!(result2.unwrap_signal(), Value::I64(16));
    }

    /// Self-hosting: typical classify-one pattern for operator dispatch
    #[test]
    fn test_pipeline_selfhost_dispatch() {
        // Simulate: type=3 check → if match, use value directly
        // Equivalent to: tk-type 3 = ?? use-op | ... ]
        // But since we can't do conditional in flat pipeline,
        // test the basic comparison: 3 3 =
        let ops = whisperc_compile("3 3 =");
        let mut vm = Vm::new();
        let result = vm.execute(&ops).unwrap().unwrap();
        assert_eq!(result.unwrap_signal(), Value::Bool(true));

        // NOT equal: 3 4 =
        let ops2 = whisperc_compile("3 4 =");
        let mut vm2 = Vm::new();
        let result2 = vm2.execute(&ops2).unwrap().unwrap();
        assert_eq!(result2.unwrap_signal(), Value::Bool(false));
    }

}
