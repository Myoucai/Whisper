//! Bridge between Rust Opcodes and Whisper C transpiler.
//!
//! Handles:
//! - Opcode → Whisper Value conversion
//! - Label collection and emission (Cond/Jump/Loop targets)
//! - Safe C identifier generation for Call opcodes
//! - Whisper VM loading and execution

use std::collections::HashSet;
use std::rc::Rc;
use whisper_core::opcode::Opcode;
use whisper_core::value::Value;
use whisper_core::vm::Vm;
use whisper_parser::Parser;
use crate::bytecode_gen::BytecodeGenerator;

/// Convert an Opcode to a Whisper Value (list of integers/strings).
fn opcode_to_value(op: &Opcode) -> Value {
    match op {
        Opcode::Dup => Value::List(Rc::new(vec![Value::I64(0x00)])),
        Opcode::Swap => Value::List(Rc::new(vec![Value::I64(0x01)])),
        Opcode::Drop => Value::List(Rc::new(vec![Value::I64(0x02)])),
        Opcode::Rot => Value::List(Rc::new(vec![Value::I64(0x03)])),
        Opcode::Add => Value::List(Rc::new(vec![Value::I64(0x10)])),
        Opcode::Sub => Value::List(Rc::new(vec![Value::I64(0x11)])),
        Opcode::Mul => Value::List(Rc::new(vec![Value::I64(0x12)])),
        Opcode::Div => Value::List(Rc::new(vec![Value::I64(0x13)])),
        Opcode::Mod => Value::List(Rc::new(vec![Value::I64(0x14)])),
        Opcode::Eq => Value::List(Rc::new(vec![Value::I64(0x18)])),
        Opcode::Lt => Value::List(Rc::new(vec![Value::I64(0x19)])),
        Opcode::Gt => Value::List(Rc::new(vec![Value::I64(0x1A)])),
        Opcode::Neq => Value::List(Rc::new(vec![Value::I64(0x1B)])),
        Opcode::Le => Value::List(Rc::new(vec![Value::I64(0x1C)])),
        Opcode::Ge => Value::List(Rc::new(vec![Value::I64(0x1D)])),
        Opcode::And => Value::List(Rc::new(vec![Value::I64(0x20)])),
        Opcode::Or => Value::List(Rc::new(vec![Value::I64(0x21)])),
        Opcode::Not => Value::List(Rc::new(vec![Value::I64(0x22)])),
        Opcode::Nth => Value::List(Rc::new(vec![Value::I64(0x40)])),
        Opcode::Append => Value::List(Rc::new(vec![Value::I64(0x41)])),
        Opcode::Len => Value::List(Rc::new(vec![Value::I64(0x42)])),
        Opcode::Map => Value::List(Rc::new(vec![Value::I64(0x43)])),
        Opcode::Each => Value::List(Rc::new(vec![Value::I64(0x44)])),
        Opcode::Fold => Value::List(Rc::new(vec![Value::I64(0x45)])),
        Opcode::StrLen => Value::List(Rc::new(vec![Value::I64(0x46)])),
        Opcode::StrCat => Value::List(Rc::new(vec![Value::I64(0x47)])),
        Opcode::StrSlice => Value::List(Rc::new(vec![Value::I64(0x48)])),
        Opcode::StrEq => Value::List(Rc::new(vec![Value::I64(0x49)])),
        Opcode::StrLt => Value::List(Rc::new(vec![Value::I64(0x4A)])),
        Opcode::StrFind => Value::List(Rc::new(vec![Value::I64(0x4B)])),
        Opcode::StrReplace => Value::List(Rc::new(vec![Value::I64(0x4C)])),
        Opcode::StrToI64 => Value::List(Rc::new(vec![Value::I64(0x4D)])),
        Opcode::I64ToStr => Value::List(Rc::new(vec![Value::I64(0x4E)])),
        Opcode::StrNth => Value::List(Rc::new(vec![Value::I64(0x4F)])),
        Opcode::Pick(n) => Value::List(Rc::new(vec![Value::I64(0x04), Value::I64(*n as i64)])),
        Opcode::Times => Value::List(Rc::new(vec![Value::I64(0x53)])),
        Opcode::Return => Value::List(Rc::new(vec![Value::I64(0x61)])),
        Opcode::OutputTop => Value::List(Rc::new(vec![Value::I64(0x90)])),
        Opcode::OutputAll => Value::List(Rc::new(vec![Value::I64(0x91)])),
        Opcode::ReadInput => Value::List(Rc::new(vec![Value::I64(0x92)])),
        Opcode::I64ToF64 => Value::List(Rc::new(vec![Value::I64(0xB0)])),
        Opcode::F64ToI64 => Value::List(Rc::new(vec![Value::I64(0xB1)])),
        Opcode::FSqrt => Value::List(Rc::new(vec![Value::I64(0xB2)])),
        Opcode::FSin => Value::List(Rc::new(vec![Value::I64(0xB3)])),
        Opcode::FCos => Value::List(Rc::new(vec![Value::I64(0xB4)])),
        Opcode::FTan => Value::List(Rc::new(vec![Value::I64(0xB5)])),

        // Opcodes with operands — replace offset with absolute label position
        Opcode::PushI64(n) => Value::List(Rc::new(vec![Value::I64(0x30), Value::I64(*n)])),
        Opcode::PushBool(b) => Value::List(Rc::new(vec![Value::I64(0x33), Value::I64(if *b { 1 } else { 0 })])),
        Opcode::PushStr(s) => Value::List(Rc::new(vec![Value::I64(0x32), Value::Str(Rc::new(s.as_ref().clone()))])),
        Opcode::Call(name) => Value::List(Rc::new(vec![Value::I64(0x60), Value::Str(Rc::new(name.clone()))])),
        // Cond/Jump/Loop: offset will be replaced with absolute position by caller
        Opcode::Cond(offset) => Value::List(Rc::new(vec![Value::I64(0x50), Value::I64(*offset as i64)])),
        Opcode::Jump(offset) => Value::List(Rc::new(vec![Value::I64(0x51), Value::I64(*offset as i64)])),
        Opcode::Loop(offset) => Value::List(Rc::new(vec![Value::I64(0x52), Value::I64(*offset as i64)])),

        _ => Value::List(Rc::new(vec![Value::I64(0xFF)])),
    }
}

/// Load the Whisper cgen into a VM.
fn load_cgen(vm: &mut Vm) {
    let cgen_src = include_str!("../../../../whisperc/native/cgen.ws");
    let ast = Parser::parse_source(cgen_src).unwrap();
    let mut gen = BytecodeGenerator::new();
    let (bc, defs) = gen.compile(&ast);
    for (name, code) in defs {
        vm.define_word(name, code);
    }
    vm.execute(&bc).unwrap();
}

/// Collect all label positions from Cond/Jump/Loop opcodes.
/// Returns a set of absolute bytecode positions that need labels.
fn collect_labels(opcodes: &[Opcode]) -> HashSet<usize> {
    let mut labels = HashSet::new();
    for (i, op) in opcodes.iter().enumerate() {
        let target = match op {
            Opcode::Cond(offset) => (i as i64 + 1 + *offset as i64) as usize,
            Opcode::Jump(offset) => (i as i64 + 1 + *offset as i64) as usize,
            Opcode::Loop(offset) => (i as i64 + 1 + *offset as i64) as usize,
            _ => continue,
        };
        labels.insert(target);
    }
    labels
}

/// Transpile a single opcode to C code using the Whisper cgen.
/// For Cond/Jump/Loop, replace the offset with the absolute label position.
/// For Call, replace the name with safe_c_name.
fn emit_opcode(vm: &mut Vm, op: &Opcode) -> Result<String, String> {
    let op_val = match op {
        // Replace offset with absolute label position
        Opcode::Cond(offset) => {
            // The caller will handle label emission; pass offset as-is
            // The Whisper cgen emits `goto L_OFFSET;` — we'll fix up in post-processing
            opcode_to_value(op)
        }
        Opcode::Jump(offset) => opcode_to_value(op),
        Opcode::Loop(offset) => opcode_to_value(op),
        // Replace Call name with safe_c_name
        Opcode::Call(name) => {
            let cname = safe_c_name(name);
            Value::List(Rc::new(vec![Value::I64(0x60), Value::Str(Rc::new(cname))]))
        }
        _ => opcode_to_value(op),
    };

    vm.data_stack.push(op_val);
    let result = vm.execute(&[Opcode::Call("emit-one".to_string())])
        .map_err(|e| format!("emit-one: {e}"))?;
    match result {
        Some(Value::Str(s)) => Ok(s.as_ref().clone()),
        other => Err(format!("emit-one returned: {other:?}")),
    }
}

/// Transpile opcodes to C code using the Whisper cgen.
/// Handles label emission and fixup for goto targets.
pub fn transpile_via_whisper(
    main_bytecode: &[Opcode],
    defs: &[(String, Vec<Opcode>)],
) -> Result<String, String> {
    let mut vm = Vm::new();
    load_cgen(&mut vm);

    let mut c_code = String::new();

    // Header
    c_code.push_str("// Auto-generated by Whisper C transpiler (self-hosted)\n");
    c_code.push_str("#include \"runtime.c\"\n\n");

    // Forward declarations
    for (name, _) in defs {
        let cname = safe_c_name(name);
        c_code.push_str(&format!("static void w_{cname}(void);\n"));
    }
    c_code.push('\n');

    // Word definitions
    for (name, code) in defs {
        let cname = safe_c_name(name);
        c_code.push_str(&format!("static void w_{cname}(void) {{\n"));
        transpile_block(&mut vm, code, &mut c_code)?;
        c_code.push_str("}\n\n");
    }

    // Main function
    c_code.push_str("int main(int argc, char**argv) {\n");
    c_code.push_str("  w_init(argc, argv);\n");
    transpile_block(&mut vm, main_bytecode, &mut c_code)?;
    c_code.push_str("  return 0;\n}\n");

    Ok(c_code)
}

/// Transpile a block of opcodes, emitting labels at jump targets.
fn transpile_block(vm: &mut Vm, opcodes: &[Opcode], c_code: &mut String) -> Result<(), String> {
    let labels = collect_labels(opcodes);

    for (i, op) in opcodes.iter().enumerate() {
        // Emit label if this position is a jump target
        if labels.contains(&i) {
            c_code.push_str(&format!("L_{i}:\n"));
        }

        // Emit C code for this opcode
        let mut c = emit_opcode(vm, op)?;

        // Fix up goto labels: replace "goto L_OFFSET" with "goto L_ABSOLUTE"
        // The Whisper cgen emits the raw offset; we need the absolute position
        if matches!(op, Opcode::Cond(_) | Opcode::Jump(_) | Opcode::Loop(_)) {
            let offset = match op {
                Opcode::Cond(o) | Opcode::Jump(o) | Opcode::Loop(o) => *o,
                _ => 0,
            };
            let abs_target = (i as i64 + 1 + offset as i64) as usize;
            // Replace "goto L_OFFSET" with "goto L_ABSOLUTE" or "goto L_-N" with "goto L_ABSOLUTE"
            let old_label = format!("L_{offset}");
            let new_label = format!("L_{abs_target}");
            c = c.replace(&old_label, &new_label);
        }

        c_code.push_str(&c);
    }

    // Emit trailing label if needed
    if labels.contains(&opcodes.len()) {
        c_code.push_str(&format!("L_{}: ;\n", opcodes.len()));
    }

    Ok(())
}

/// Safe C identifier from word name.
fn safe_c_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}
