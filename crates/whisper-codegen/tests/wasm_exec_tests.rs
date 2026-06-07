use whisper_codegen::wasm_gen::WasmGenerator;
/// WASM end-to-end verification tests.
///
/// Verifies that generated .wasm files:
/// 1. Have valid WASM module structure (magic, version, sections)
/// 2. Contain the embedded Whisper bytecode in the data section
/// 3. Exports the required functions (whisper_run, memory, get_stack_ptr)
/// 4. The embedded bytecode matches the original Whisper opcodes
use whisper_core::opcode::Opcode;

fn read_leb128_u(data: &[u8], pos: &mut usize) -> u64 {
    let mut result: u64 = 0;
    let mut shift = 0;
    loop {
        let byte = data[*pos];
        *pos += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    result
}

fn read_leb128_s(data: &[u8], pos: &mut usize) -> i64 {
    let mut result: i64 = 0;
    let mut shift = 0;
    loop {
        let byte = data[*pos];
        *pos += 1;
        result |= ((byte & 0x7F) as i64) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            if shift < 64 && (byte & 0x40) != 0 {
                result |= -(1i64 << shift);
            }
            break;
        }
    }
    result
}

/// Parse WASM and extract bytecode from data segment at offset 0x0010.
fn extract_bytecode(wasm: &[u8]) -> Vec<u8> {
    assert_eq!(&wasm[0..4], b"\0asm");
    assert_eq!(&wasm[4..8], &[1, 0, 0, 0]);

    let mut pos = 8;
    while pos < wasm.len() {
        let section_id = wasm[pos];
        pos += 1;
        let section_len = read_leb128_u(wasm, &mut pos) as usize;
        let section_end = pos + section_len;

        if section_id == 11 {
            // Data section
            let count = read_leb128_u(wasm, &mut pos);
            for _ in 0..count {
                assert_eq!(wasm[pos], 0x00);
                pos += 1; // mode=active, memory=0
                assert_eq!(wasm[pos], 0x41);
                pos += 1; // i32.const
                let offset = read_leb128_s(wasm, &mut pos) as u32;
                assert_eq!(wasm[pos], 0x0B);
                pos += 1; // end
                let data_len = read_leb128_u(wasm, &mut pos) as usize;
                let data = wasm[pos..pos + data_len].to_vec();
                pos += data_len;
                if offset == 0x0010 {
                    return data;
                }
            }
        }
        pos = section_end;
    }
    panic!("Bytecode data segment not found");
}

/// Reconstruct original opcodes from raw bytecode data.
fn decode_bytecode(raw: &[u8]) -> Vec<Opcode> {
    let mut ops = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let byte = raw[i];
        i += 1;
        match byte {
            0x00 => ops.push(Opcode::Dup),
            0x01 => ops.push(Opcode::Swap),
            0x02 => ops.push(Opcode::Drop),
            0x10 => ops.push(Opcode::Add),
            0x11 => ops.push(Opcode::Sub),
            0x12 => ops.push(Opcode::Mul),
            0x13 => ops.push(Opcode::Div),
            0x18 => ops.push(Opcode::Eq),
            0x19 => ops.push(Opcode::Lt),
            0x1A => ops.push(Opcode::Gt),
            0x30 => {
                let mut buf = [0u8; 8];
                buf.copy_from_slice(&raw[i..i + 8]);
                ops.push(Opcode::PushI64(i64::from_le_bytes(buf)));
                i += 8;
            }
            0x31 => {
                let mut buf = [0u8; 8];
                buf.copy_from_slice(&raw[i..i + 8]);
                ops.push(Opcode::PushF64(f64::from_le_bytes(buf)));
                i += 8;
            }
            0x33 => {
                ops.push(Opcode::PushBool(raw[i] != 0));
                i += 1;
            }
            0x32 => {
                let len = u32::from_le_bytes(raw[i..i + 4].try_into().unwrap()) as usize;
                i += 4;
                let s = String::from_utf8_lossy(&raw[i..i + len]).to_string();
                ops.push(Opcode::PushStr(s));
                i += len;
            }
            0x50 => {
                let off = i32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
                ops.push(Opcode::Cond(off));
                i += 4;
            }
            0x51 => {
                let off = i32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
                ops.push(Opcode::Jump(off));
                i += 4;
            }
            0x60 => {
                let _idx = u32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
                ops.push(Opcode::Call(format!("f_{_idx}")));
                i += 4;
            }
            0x61 => ops.push(Opcode::Return),
            0x90 => ops.push(Opcode::OutputTop),
            _ => { /* skip unknown */ }
        }
    }
    ops
}

#[test]
fn test_wasm_module_structure() {
    let ops = vec![Opcode::PushI64(1), Opcode::PushI64(2), Opcode::Add];
    let gen = WasmGenerator::new(ops);
    let wasm = gen.compile();

    assert_eq!(&wasm[0..4], b"\0asm");
    assert_eq!(&wasm[4..8], &[1, 0, 0, 0]);
    assert!(wasm.len() > 100, "WASM module too small");

    // Verify sections exist
    let sections = [1u8, 3, 5, 7, 10, 11]; // type, func, memory, export, code, data
    let mut pos = 8;
    let mut found = vec![false; 12];
    while pos < wasm.len() {
        let id = wasm[pos];
        pos += 1;
        let len = read_leb128_u(&wasm, &mut pos) as usize;
        if (id as usize) < found.len() {
            found[id as usize] = true;
        }
        pos += len;
    }
    for &s in &sections {
        assert!(found[s as usize], "Missing section {}", s);
    }
}

#[test]
fn test_wasm_bytecode_roundtrip() {
    // Generate WASM from known opcodes
    let ops = vec![
        Opcode::PushI64(42),
        Opcode::PushI64(13),
        Opcode::Add,
        Opcode::PushI64(55),
        Opcode::Eq,
    ];
    let gen = WasmGenerator::new(ops.clone());
    let wasm = gen.compile();

    // Extract embedded bytecode
    let raw = extract_bytecode(&wasm);
    let decoded = decode_bytecode(&raw);

    // Verify opcodes match (at minimum, the PushI64 values)
    assert_eq!(decoded.len(), ops.len(), "Opcodes count mismatch");
    for (i, (a, b)) in ops.iter().zip(decoded.iter()).enumerate() {
        assert_eq!(a, b, "Opcode mismatch at index {}", i);
    }
}

#[test]
fn test_wasm_hello_world_structure() {
    // hello.ws: "Hello, World!" OutputTop
    let ops = vec![
        Opcode::PushStr("Hello, World!".to_string()),
        Opcode::OutputTop,
    ];
    let gen = WasmGenerator::new(ops);
    let wasm = gen.compile();

    let raw = extract_bytecode(&wasm);
    let decoded = decode_bytecode(&raw);

    assert_eq!(decoded.len(), 2);
    assert!(matches!(decoded[0], Opcode::PushStr(_)));
    assert_eq!(decoded[1], Opcode::OutputTop);
}

#[test]
fn test_wasm_complex_program_roundtrip() {
    // (10-3)*(2+1) = 21
    let ops = vec![
        Opcode::PushI64(10),
        Opcode::PushI64(3),
        Opcode::Sub,
        Opcode::PushI64(2),
        Opcode::PushI64(1),
        Opcode::Add,
        Opcode::Mul,
    ];
    let gen = WasmGenerator::new(ops.clone());
    let wasm = gen.compile();

    let raw = extract_bytecode(&wasm);
    let decoded = decode_bytecode(&raw);

    assert_eq!(decoded, ops);
}

#[test]
fn test_wasm_exports_present() {
    let ops = vec![Opcode::PushI64(1), Opcode::PushI64(2), Opcode::Add];
    let gen = WasmGenerator::new(ops);
    let wasm = gen.compile();

    // Check that "whisper_run", "memory", "get_stack_ptr" appear in the export section
    let wasm_str = String::from_utf8_lossy(&wasm);
    assert!(
        wasm_str.contains("whisper_run"),
        "Missing whisper_run export"
    );
    assert!(wasm_str.contains("memory"), "Missing memory export");
    assert!(
        wasm_str.contains("get_stack_ptr"),
        "Missing get_stack_ptr export"
    );
}
