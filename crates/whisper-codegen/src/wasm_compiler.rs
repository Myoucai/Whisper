/// Direct WASM compiler: Whisper bytecode → WASM instructions.
///
/// Compiles a linear sequence of Whisper opcodes directly to WASM
/// stack operations. The WASM operand stack is used as the data stack.
///
/// For programs without control flow or word definitions, this produces
/// minimal, fast WASM that runs directly in the browser.

use whisper_core::opcode::Opcode;

/// WASM section IDs
const TYPE: u8 = 1;
const FUNCTION: u8 = 3;
const MEMORY: u8 = 5;
const EXPORT: u8 = 7;
const CODE: u8 = 10;
const DATA: u8 = 11;

/// WASM value types
const I32: u8 = 0x7F;
const I64: u8 = 0x7E;

/// WASM opcodes
const END: u8 = 0x0B;
const I32_CONST: u8 = 0x41;
const I64_CONST: u8 = 0x42;
const I64_ADD: u8 = 0x7C;
const I64_SUB: u8 = 0x7D;
const I64_MUL: u8 = 0x7E;
const I64_DIV_S: u8 = 0x7F;
const I32_STORE: u8 = 0x36;
const I64_STORE: u8 = 0x37;

/// Compile a linear Whisper program directly to WASM.
/// Returns the WASM binary. Only handles basic opcodes.
pub fn compile_direct(ops: &[Opcode]) -> Vec<u8> {
    let mut wasm = Vec::new();

    // Magic + version
    wasm.extend_from_slice(b"\0asm");
    wasm.extend_from_slice(&[1, 0, 0, 0]);

    // Collect string literals that need to go in the data section
    let mut strings: Vec<(usize, Vec<u8>)> = Vec::new(); // (addr, bytes)
    let mut body = Vec::new();
    let mut string_addr: u32 = 0x1000; // Start of string table

    for op in ops {
        match op {
            Opcode::PushI64(n) => {
                body.push(I64_CONST);
                leb128_s(&mut body, *n);
            }
            Opcode::Add => body.push(I64_ADD),
            Opcode::Sub => body.push(I64_SUB),
            Opcode::Mul => body.push(I64_MUL),
            Opcode::Div => body.push(I64_DIV_S),
            Opcode::PushStr(s) => {
                let bytes = s.as_bytes();
                strings.push((string_addr as usize, bytes.to_vec()));
                // Push the string pointer as i64
                body.push(I32_CONST);
                leb128_s(&mut body, string_addr as i64);
                body.push(0xAC); // i64.extend_i32_s
                string_addr += align_up(bytes.len() as u32, 4);
            }
            Opcode::OutputTop => {
                // Value stays on WASM stack as return value
                // Also store to memory[0] for string/external access
                body.push(0x21); // local.set 0 (save to local)
                body.push(0);    // local 0
                body.push(I32_CONST);
                leb128_s(&mut body, 0);
                body.push(0x20); // local.get 0
                body.push(0);    // local 0
                body.push(I64_STORE); // store to memory[0]
                body.push(3); body.push(0);
                body.push(0x20); // local.get 0 (restore for return)
                body.push(0);    // local 0
            }
            _ => {} // unsupported opcode, skip
        }
    }

    // Push default return value if nothing was output
    if !contains_output(ops) {
        body.push(I64_CONST);
        leb128_s(&mut body, 0);
    }

    body.push(END);

    // --- Build WASM sections ---

    // Type section: 2 types: ()->() and ()->i64
    let types = vec![2u8,
        0x60, 0, 0,          // type 0: () -> ()
        0x60, 0, 1, I64,     // type 1: () -> i64
    ];
    wasm.extend_from_slice(&section(TYPE, &types));

    // Function section: 1 function at type 1
    wasm.extend_from_slice(&section(FUNCTION, &[1, 1]));

    // Memory section: 1 page (64KB)
    wasm.extend_from_slice(&section(MEMORY, &[1, 0, 1]));

    // Export section
    let mut exp = Vec::new();
    // Export count
    leb128_u(&mut exp, 3);
    export_entry(&mut exp, "whisper_run", 0x00, 0);
    export_entry(&mut exp, "memory", 0x02, 0);
    export_entry(&mut exp, "get_result", 0x00, 0); // alias
    wasm.extend_from_slice(&section(EXPORT, &exp));

    // Code section: 1 function body
    let mut code = vec![1u8]; // 1 function
    // Function body: 0 locals + instructions + end
    let mut func = Vec::new();
    leb128_u(&mut func, 1); // 1 local set
    leb128_u(&mut func, 1); // count=1
    func.push(I64);          // type=i64
    func.extend_from_slice(&body);
    code.extend_from_slice(&vec_u8(&func));
    wasm.extend_from_slice(&section(CODE, &code));

    // Data section: string table
    if !strings.is_empty() {
        let mut data = Vec::new();
        leb128_u(&mut data, strings.len() as u64);
        for (addr, bytes) in &strings {
            data.push(0x00); // active, memory 0
            data.push(I32_CONST);
            leb128_s(&mut data, *addr as i64);
            data.push(END);
            data.extend_from_slice(&vec_u8(bytes));
        }
        wasm.extend_from_slice(&section(DATA, &data));
    }

    wasm
}

fn contains_output(ops: &[Opcode]) -> bool {
    ops.iter().any(|o| matches!(o, Opcode::OutputTop))
}

fn align_up(n: u32, align: u32) -> u32 {
    (n + align - 1) & !(align - 1)
}

// === WASM encoding helpers ===

fn section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = vec![id];
    v.extend_from_slice(&vec_u8(payload));
    v
}

fn vec_u8(d: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    leb128_u(&mut v, d.len() as u64);
    v.extend_from_slice(d);
    v
}

fn leb128_u(b: &mut Vec<u8>, mut n: u64) {
    loop {
        let mut byte = (n & 0x7F) as u8;
        n >>= 7;
        if n != 0 { byte |= 0x80; }
        b.push(byte);
        if n == 0 { break; }
    }
}

fn leb128_s(b: &mut Vec<u8>, mut n: i64) {
    loop {
        let byte = (n & 0x7F) as u8;
        n >>= 7;
        if (n == 0 && byte & 0x40 == 0) || (n == -1 && byte & 0x40 != 0) {
            b.push(byte); break;
        }
        b.push(byte | 0x80);
    }
}

fn export_entry(buf: &mut Vec<u8>, name: &str, kind: u8, idx: u32) {
    buf.extend_from_slice(&vec_u8(name.as_bytes()));
    buf.push(kind);
    leb128_u(buf, idx as u64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_arithmetic() {
        let ops = vec![Opcode::PushI64(42), Opcode::PushI64(13), Opcode::Add, Opcode::OutputTop];
        let wasm = compile_direct(&ops);
        assert_eq!(&wasm[0..4], b"\0asm");
        assert!(wasm.len() > 50);
    }

    #[test]
    fn test_compile_hello() {
        let ops = vec![Opcode::PushStr("Hi".to_string()), Opcode::OutputTop];
        let wasm = compile_direct(&ops);
        assert_eq!(&wasm[0..4], b"\0asm");
        // Should contain "Hi" in data section
        let wasm_str = String::from_utf8_lossy(&wasm);
        assert!(wasm_str.contains("Hi"));
    }
}
