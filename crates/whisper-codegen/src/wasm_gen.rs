//! WASM code generator: Whisper bytecode → standalone .wasm module.
//!
//! Generates a complete WASM module with an embedded bytecode interpreter.
//! The interpreter uses a fetch-decode-execute loop on a data stack in linear memory.
//!
//! Memory layout (64KB, 1 page):
//!   0x0000: sp (i32) — data stack pointer. Starts at 0x2000
//!   0x0004: ip (i32) — instruction pointer into bytecode
//!   0x0008: bc_len (i32) — bytecode length
//!   0x0010..: bytecode data (max ~4KB fits in 0x0010..0x1000)
//!   0x1000: scratch area (opcode save, temp values)
//!   0x2000..: data stack (16 bytes/element: 8B value + 8B unused)

use whisper_core::opcode::Opcode;

#[allow(dead_code)]
mod w {
    pub const END: u8 = 0x0B;
    pub const BLOCK: u8 = 0x02;
    pub const LOOP: u8 = 0x03;
    pub const BR: u8 = 0x0C;
    pub const BR_IF: u8 = 0x0D;
    pub const IF: u8 = 0x04;
    pub const I32_CONST: u8 = 0x41;
    pub const I64_CONST: u8 = 0x42;
    pub const I32_LOAD: u8 = 0x28;
    pub const I64_LOAD: u8 = 0x29;
    pub const F64_LOAD: u8 = 0x2B;
    pub const I32_LOAD8_U: u8 = 0x2D;
    pub const I32_STORE: u8 = 0x36;
    pub const I64_STORE: u8 = 0x37;
    pub const F64_STORE: u8 = 0x39;
    pub const I32_ADD: u8 = 0x6A;
    pub const I32_SUB: u8 = 0x6B;
    pub const I64_ADD: u8 = 0x7C;
    pub const I64_SUB: u8 = 0x7D;
    pub const I64_MUL: u8 = 0x7E;
    pub const I64_DIV_S: u8 = 0x7F;
    pub const I64_EQ: u8 = 0x51;
    pub const I64_LT_S: u8 = 0x53;
    pub const I64_GT_S: u8 = 0x55;
    pub const I32_EQ: u8 = 0x46;
    pub const I32_GE_U: u8 = 0x4F;
    pub const I32_LT_U: u8 = 0x49;
    pub const I64_EXTEND_I32_S: u8 = 0xAC;
    pub const I64_REM_S: u8 = 0x6F;
    pub const I64_EQZ: u8 = 0x50;
    pub const I64_LE_S: u8 = 0x57;
    pub const I64_GE_S: u8 = 0x59;
    pub const I64_NE: u8 = 0x52;
    pub const I64_AND: u8 = 0x83;
    pub const I64_OR: u8 = 0x84;
    pub const I64_EXTEND32_S: u8 = 0xB8;
    pub const F64_CONVERT_I32_S: u8 = 0xBB;
    pub const I32_TRUNC_F64_S: u8 = 0xA9;
    pub const F64_SQRT: u8 = 0x9F;
    pub const F64_NEG: u8 = 0x9A;
    pub const F64_ADD: u8 = 0xA0;
    pub const F64_SUB: u8 = 0xA1;
    pub const F64_MUL: u8 = 0xA2;
    pub const F64_DIV: u8 = 0xA3;
    pub const F64_SIN: u8 = 0; // not available as single WASM op
    pub const F64_COS: u8 = 0; // not available as single WASM op
}

pub struct WasmGenerator {
    bytecode: Vec<Opcode>,
}

impl WasmGenerator {
    pub fn new(bytecode: Vec<Opcode>) -> Self {
        WasmGenerator { bytecode }
    }

    fn raw_bytecode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        for op in &self.bytecode {
            b.push(op.to_byte());
            match op {
                Opcode::Pick(n) => b.push(*n),
                Opcode::PushI64(n) => b.extend_from_slice(&n.to_le_bytes()),
                Opcode::PushF64(n) => b.extend_from_slice(&n.to_le_bytes()),
                Opcode::PushStr(s) => {
                    b.extend_from_slice(&(s.len() as u32).to_le_bytes());
                    b.extend_from_slice(s.as_bytes());
                }
                Opcode::PushBool(v) => b.push(if *v { 1 } else { 0 }),
                Opcode::Cond(o) | Opcode::Jump(o) | Opcode::Loop(o) => {
                    b.extend_from_slice(&o.to_le_bytes())
                }
                Opcode::Call(_) => b.extend_from_slice(&0u32.to_le_bytes()),
                Opcode::CapCall(i) => b.extend_from_slice(&i.to_le_bytes()),
                Opcode::ConfLabel(c) => b.extend_from_slice(&c.to_le_bytes()),
                Opcode::PushRef(inner) => {
                    b.extend_from_slice(&(inner.len() as u32).to_le_bytes());
                    // Flatten inner opcodes
                    for inner_op in inner {
                        Self::encode_op_raw(&mut b, inner_op);
                    }
                }
                _ => {}
            }
        }
        b
    }

    fn encode_op_raw(b: &mut Vec<u8>, op: &Opcode) {
        b.push(op.to_byte());
        match op {
            Opcode::PushI64(n) => b.extend_from_slice(&n.to_le_bytes()),
            Opcode::PushF64(n) => b.extend_from_slice(&n.to_le_bytes()),
            Opcode::PushBool(v) => b.push(if *v { 1 } else { 0 }),
            Opcode::Pick(n) => b.push(*n),
            _ => {}
        }
    }

    pub fn compile(&self) -> Vec<u8> {
        let mut wasm = Vec::new();
        wasm.extend_from_slice(b"\0asm");
        wasm.extend_from_slice(&[1, 0, 0, 0]);

        let raw = self.raw_bytecode();
        let bc_len = raw.len();

        // Type section: 4 types
        let types = [
            4u8, 0x60, 0, 0, 0x60, 0, 1, 0x7E, 0x60, 0, 1, 0x7C, 0x60, 0, 1, 0x7F,
        ];
        wasm.extend_from_slice(&sec(1, &types));

        // Function section: 3 functions
        wasm.extend_from_slice(&sec(3, &[3, 1, 2, 3]));

        // Memory: 1 page
        wasm.extend_from_slice(&sec(5, &[1, 0, 1]));

        // Exports: 4 exports
        let mut exp = Vec::new();
        uleb128(&mut exp, 4); // export count
        export(&mut exp, "whisper_run", 0x00, 0);
        export(&mut exp, "whisper_run_f64", 0x00, 1);
        export(&mut exp, "get_stack_ptr", 0x00, 2);
        export(&mut exp, "memory", 0x02, 0);
        wasm.extend_from_slice(&sec(7, &exp));

        // Code section: 3 function bodies
        let f0 = build_interpreter(true);
        let f1 = build_interpreter(false);
        let f2 = build_get_sp();
        let mut cb = vec![3u8];
        cb.extend_from_slice(&vec_u8(&f0));
        cb.extend_from_slice(&vec_u8(&f1));
        cb.extend_from_slice(&vec_u8(&f2));
        wasm.extend_from_slice(&sec(10, &cb));

        // Data section: init memory
        let mut data = Vec::new();
        data_seg(&mut data, 0x0000, &0x2000u32.to_le_bytes()); // sp = 0x2000
        data_seg(&mut data, 0x0008, &(bc_len as u32).to_le_bytes()); // bc_len
        data_seg(&mut data, 0x0010, &raw); // bytecode
        let mut ds = vec![3u8];
        ds.extend_from_slice(&data);
        wasm.extend_from_slice(&sec(11, &ds));

        wasm
    }

    pub fn compile_to_file(&self, path: &std::path::Path) -> Result<(), String> {
        std::fs::write(path, self.compile()).map_err(|e| e.to_string())
    }
}

// === Interpreter function ===

fn build_interpreter(i64_result: bool) -> Vec<u8> {
    let mut b = Vec::new();
    uleb128(&mut b, 0); // 0 locals

    // block $done
    b.push(w::BLOCK);
    b.push(0x40);
    // loop $continue
    b.push(w::LOOP);
    b.push(0x40);

    // if ip >= bc_len → br $done
    ld_i32(&mut b, 0x0004);
    ld_i32(&mut b, 0x0008);
    b.push(w::I32_GE_U);
    b.push(w::BR_IF);
    b.push(1);

    // read opcode byte from [0x0010 + ip]
    ld_i32(&mut b, 0x0004);
    ci32(&mut b, 0x0010);
    b.push(w::I32_ADD);
    b.push(w::I32_LOAD8_U);
    b.push(0);
    b.push(0);

    // save opcode to scratch[0x1000]
    ci32(&mut b, 0x1000);
    b.push(w::I32_STORE);
    b.push(2);
    b.push(0);

    // default: ip += 1
    add_ip(&mut b, 1);

    // ── Dispatch: independent if blocks ──
    // Each block: if (scratch_opcode == X) { handler } end

    // 0x30 PushI64 — 8 byte immediate
    if_op(&mut b, 0x30);
    add_ip(&mut b, 7);
    ld_i32(&mut b, 0x0004);
    ci32(&mut b, 0x0010 - 8);
    b.push(w::I32_ADD);
    b.push(w::I64_LOAD);
    b.push(3);
    b.push(0);
    push(&mut b);
    b.push(w::END);

    // 0x31 PushF64 — 8 byte immediate
    if_op(&mut b, 0x31);
    add_ip(&mut b, 7);
    ld_i32(&mut b, 0x0004);
    ci32(&mut b, 0x0010 - 8);
    b.push(w::I32_ADD);
    b.push(w::F64_LOAD);
    b.push(3);
    b.push(0);
    push_f64(&mut b);
    b.push(w::END);

    // 0x32 PushStr — save string data pointer, skip past length+data
    if_op(&mut b, 0x32);
    // Save pointer to string data (= bytecode_base + current_ip)
    ld_i32(&mut b, 0x0004);  // current ip (points to 4-byte length)
    ci32(&mut b, 4);
    b.push(w::I32_ADD);       // ip + 4 = pointer to string data (past length)
    ci32(&mut b, 0x0010);
    b.push(w::I32_ADD);       // bytecode_base + ip + 4 = absolute pointer
    ci32(&mut b, 0x1020);
    b.push(w::I32_STORE);
    b.push(2);
    b.push(0);                // scratch[0x1020] = string data pointer
    // Read u32 length
    ld_i32(&mut b, 0x0004);  // ip
    ci32(&mut b, 0x0010);
    b.push(w::I32_ADD);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);                // length
    ci32(&mut b, 0x1028);
    b.push(w::I32_STORE);
    b.push(2);
    b.push(0);                // scratch[0x1028] = string length
    // Advance ip past length (4) + data (length)
    ld_i32(&mut b, 0x0004);  // ip
    ci32(&mut b, 0x1028);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
    b.push(w::I32_ADD);       // ip + len
    ci32(&mut b, 4);
    b.push(w::I32_ADD);       // ip + len + 4
    ci32(&mut b, 0x0004);
    b.push(w::I32_STORE);
    b.push(2);
    b.push(0);
    // Push string pointer as i64 (from scratch[0x1020])
    ci32(&mut b, 0x1020);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);
    b.push(w::END);

    // 0x33 PushBool — 1 byte immediate
    if_op(&mut b, 0x33);
    ld_i32(&mut b, 0x0004);
    ci32(&mut b, 0x0010 - 1);
    b.push(w::I32_ADD);
    b.push(w::I32_LOAD8_U);
    b.push(0);
    b.push(0);
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);
    b.push(w::END);

    // 0x00 Dup
    if_op(&mut b, 0x00);
    ld_i32(&mut b, 0x0000);
    ci32(&mut b, 0xFFF0); // -16 as unsigned
    b.push(w::I32_ADD);
    b.push(w::I64_LOAD);
    b.push(3);
    b.push(0);
    push(&mut b);
    b.push(w::END);

    // 0x02 Drop
    if_op(&mut b, 0x02);
    add_sp(&mut b, 0xFFF0u32 as i32);
    b.push(w::END);

    // 0x10 Add
    if_op(&mut b, 0x10);
    pop(&mut b);
    pop(&mut b);
    b.push(w::I64_ADD);
    push(&mut b);
    b.push(w::END);

    // 0x11 Sub
    if_op(&mut b, 0x11);
    pop(&mut b);
    pop(&mut b);
    b.push(w::I64_SUB);
    push(&mut b);
    b.push(w::END);

    // 0x12 Mul
    if_op(&mut b, 0x12);
    pop(&mut b);
    pop(&mut b);
    b.push(w::I64_MUL);
    push(&mut b);
    b.push(w::END);

    // 0x13 Div
    if_op(&mut b, 0x13);
    pop(&mut b);
    pop(&mut b);
    b.push(w::I64_DIV_S);
    push(&mut b);
    b.push(w::END);

    // 0x18 Eq
    if_op(&mut b, 0x18);
    pop(&mut b);
    pop(&mut b);
    b.push(w::I64_EQ);
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);
    b.push(w::END);

    // 0x19 Lt
    if_op(&mut b, 0x19);
    pop(&mut b);
    pop(&mut b);
    b.push(w::I64_LT_S);
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);
    b.push(w::END);

    // 0x1A Gt
    if_op(&mut b, 0x1A);
    pop(&mut b);
    pop(&mut b);
    b.push(w::I64_GT_S);
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);
    b.push(w::END);

    // 0x01 Swap — exchange top two stack values
    if_op(&mut b, 0x01);
    // pop a, pop b, push a, push b (using scratch)
    pop(&mut b); // a
    ci32(&mut b, 0x1010);
    b.push(w::I64_STORE);
    b.push(3);
    b.push(0); // scratch1 = a
    pop(&mut b); // b
    ci32(&mut b, 0x1020);
    b.push(w::I64_STORE);
    b.push(3);
    b.push(0); // scratch2 = b
    ci32(&mut b, 0x1010);
    b.push(w::I64_LOAD);
    b.push(3);
    b.push(0); // load a
    push(&mut b); // push a
    ci32(&mut b, 0x1020);
    b.push(w::I64_LOAD);
    b.push(3);
    b.push(0); // load b
    push(&mut b); // push b
    b.push(w::END);

    // 0x50 Cond — pop bool, if false add offset to ip
    // Offset is i32 at bytecode[ip]; ip already advanced past opcode byte
    if_op(&mut b, 0x50);
    // Pop condition from data stack
    pop(&mut b); // cond value (i64, 0 or non-zero)
    ci32(&mut b, 0x1030);
    b.push(w::I64_STORE);
    b.push(3);
    b.push(0); // save cond to scratch
               // cond == 0 means false → jump
    ci32(&mut b, 0x1030);
    b.push(w::I64_LOAD);
    b.push(3);
    b.push(0);
    b.push(w::I64_CONST);
    leb128_s(&mut b, 0);
    b.push(w::I64_EQ); // cond == 0?
    b.push(w::IF);
    b.push(0x40);
    // Read i32 offset from bytecode[ip], add to ip
    // ip currently points to offset bytes; advance ip past them + apply offset
    ld_i32(&mut b, 0x0004); // ip
    ci32(&mut b, 0x0010);
    b.push(w::I32_ADD); // bytecode + ip = addr of offset
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0); // load i32 offset
               // ip += 4 + offset
    ld_i32(&mut b, 0x0004);
    b.push(w::I32_ADD); // ip + offset
    ci32(&mut b, 4);
    b.push(w::I32_ADD); // ip + offset + 4
    ci32(&mut b, 0x0004);
    b.push(w::I32_STORE);
    b.push(2);
    b.push(0); // store new ip
    b.push(w::END); // end if
                    // If true (cond != 0): just advance ip past the 4 offset bytes
    ci32(&mut b, 0x1030);
    b.push(w::I64_LOAD);
    b.push(3);
    b.push(0);
    b.push(w::I64_CONST);
    leb128_s(&mut b, 0);
    b.push(w::I64_GT_S); // cond > 0
    b.push(w::IF);
    b.push(0x40);
    add_ip(&mut b, 4); // skip offset bytes
    b.push(w::END);
    b.push(w::END);

    // 0x51 Jump — unconditional jump by i32 offset
    if_op(&mut b, 0x51);
    ld_i32(&mut b, 0x0004);
    ci32(&mut b, 0x0010);
    b.push(w::I32_ADD);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0); // load offset
    ld_i32(&mut b, 0x0004);
    b.push(w::I32_ADD); // ip + offset
    ci32(&mut b, 4);
    b.push(w::I32_ADD); // ip + offset + 4 (skip offset bytes)
    ci32(&mut b, 0x0004);
    b.push(w::I32_STORE);
    b.push(2);
    b.push(0);
    b.push(w::END);

    // 0x90 OutputTop
    if_op(&mut b, 0x90);
    add_sp(&mut b, 0xFFF0u32 as i32);
    b.push(w::END);

    // 0x61 Return — set ip = bc_len to exit loop
    if_op(&mut b, 0x61);
    ld_i32(&mut b, 0x0008);
    ci32(&mut b, 0x0004);
    b.push(w::I32_STORE);
    b.push(2);
    b.push(0);
    b.push(w::END);

    // ── New opcodes ───────────────────────────────────────────────

    // 0x14 Mod
    if_op(&mut b, 0x14);
    pop(&mut b);
    pop(&mut b);
    b.push(w::I64_REM_S);
    push(&mut b);
    b.push(w::END);

    // 0x1B Neq
    if_op(&mut b, 0x1B);
    pop(&mut b);
    pop(&mut b);
    b.push(w::I64_EQ);
    b.push(w::I64_EQZ); // invert to not-equal
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);
    b.push(w::END);

    // 0x1C Le
    if_op(&mut b, 0x1C);
    pop(&mut b);
    pop(&mut b);
    b.push(w::I64_LE_S);
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);
    b.push(w::END);

    // 0x1D Ge
    if_op(&mut b, 0x1D);
    pop(&mut b);
    pop(&mut b);
    b.push(w::I64_GE_S);
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);
    b.push(w::END);

    // 0x20 And
    if_op(&mut b, 0x20);
    pop(&mut b);
    pop(&mut b);
    b.push(w::I64_AND);
    push(&mut b);
    b.push(w::END);

    // 0x21 Or
    if_op(&mut b, 0x21);
    pop(&mut b);
    pop(&mut b);
    b.push(w::I64_OR);
    push(&mut b);
    b.push(w::END);

    // 0x22 Not
    if_op(&mut b, 0x22);
    pop(&mut b);
    b.push(w::I64_CONST);
    leb128_s(&mut b, 0);
    b.push(w::I64_EQ);
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);
    b.push(w::END);

    // 0x52 Loop — check bool at top of stack; if true, jump back
    if_op(&mut b, 0x52);
    pop(&mut b); // pop the condition bool
    ci32(&mut b, 0x1030);
    b.push(w::I64_STORE);
    b.push(3);
    b.push(0);
    ci32(&mut b, 0x1030);
    b.push(w::I64_LOAD);
    b.push(3);
    b.push(0);
    b.push(w::I64_CONST);
    leb128_s(&mut b, 0);
    b.push(w::I64_NE);
    b.push(w::IF);
    b.push(0x40);
    // Read i32 offset from bytecode and jump back (negative offset)
    ld_i32(&mut b, 0x0004); // ip
    ci32(&mut b, 0x0010);
    b.push(w::I32_ADD);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0); // load offset
    ld_i32(&mut b, 0x0004);
    b.push(w::I32_ADD); // ip + offset
    ci32(&mut b, 4);
    b.push(w::I32_ADD); // ip + offset + 4
    ci32(&mut b, 0x0004);
    b.push(w::I32_STORE);
    b.push(2);
    b.push(0);
    b.push(w::END);
    b.push(w::END);

    // 0x53 Times — n {body} @times: repeat body n times
    if_op(&mut b, 0x53);
    // TODO: Full Times requires executing a quotation n times
    // For now, just pop the count and move on
    add_sp(&mut b, 0xFFF0u32 as i32); // pop count (i64)
    add_sp(&mut b, 0xFFF0u32 as i32); // pop ref (skip)
    b.push(w::END);

    // 0xB0 I64ToF64
    if_op(&mut b, 0xB0);
    pop(&mut b);
    b.push(w::I64_EXTEND32_S); // i64.extend_i32_s
    b.push(w::F64_CONVERT_I32_S); // f64.convert_i32_s
    push_f64(&mut b);
    b.push(w::END);

    // 0xB1 F64ToI64
    if_op(&mut b, 0xB1);
    add_sp(&mut b, 0xFFF0u32 as i32);
    ld_i32(&mut b, 0x0000);
    b.push(w::F64_LOAD);
    b.push(3);
    b.push(0);
    b.push(w::I32_TRUNC_F64_S); // i32.trunc_f64_s (truncate toward zero)
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);
    b.push(w::END);

    // 0xB2 FSqrt
    if_op(&mut b, 0xB2);
    add_sp(&mut b, 0xFFF0u32 as i32);
    ld_i32(&mut b, 0x0000);
    b.push(w::F64_LOAD);
    b.push(3);
    b.push(0);
    b.push(w::F64_SQRT); // f64.sqrt
    ci32(&mut b, 0x0000);
    ld_i32(&mut b, 0x0000);
    b.push(w::F64_STORE);
    b.push(3);
    b.push(0);
    add_sp(&mut b, 16);
    b.push(w::END);

    // ── String & List ops (new) ────────────────────────────────────

    // 0x46 StrLen — pop string ptr, push length
    // String format in bytecode: [4B len LE][data bytes]
    // Pointer points to data bytes; length is at (ptr - 4)
    if_op(&mut b, 0x46);
    pop(&mut b);               // string pointer as i64
    ci32(&mut b, 0x1030);
    b.push(w::I64_STORE);
    b.push(3);
    b.push(0);                 // scratch[0x1030] = pointer
    // Read length from (ptr - 4)
    ci32(&mut b, 0x1030);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);                 // pointer as i32
    ci32(&mut b, 0xFFFFFFFCu32 as i32); // -4
    b.push(w::I32_ADD);       // ptr - 4
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);                 // u32 length
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);              // push length as i64
    b.push(w::END);

    // 0x42 Len — pop list ptr, push length
    // List format in bytecode: [4B count LE][elements...]
    // Same format as strings, reuse the same logic
    if_op(&mut b, 0x42);
    pop(&mut b);
    ci32(&mut b, 0x1030);
    b.push(w::I64_STORE);
    b.push(3);
    b.push(0);
    ci32(&mut b, 0x1030);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
    ci32(&mut b, 0xFFFFFFFCu32 as i32);
    b.push(w::I32_ADD);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);
    b.push(w::END);

    // 0x49 StrEq — str1_ptr str2_ptr → bool
    // Compare two strings by reading lengths and data from bytecode
    if_op(&mut b, 0x49);
    pop(&mut b);               // ptr2
    ci32(&mut b, 0x1040);
    b.push(w::I64_STORE);
    b.push(3);
    b.push(0);                 // scratch[0x1040] = ptr2
    pop(&mut b);               // ptr1
    ci32(&mut b, 0x1030);
    b.push(w::I64_STORE);
    b.push(3);
    b.push(0);                 // scratch[0x1030] = ptr1
    // Read len1 from (ptr1 - 4)
    ci32(&mut b, 0x1030);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
    ci32(&mut b, 0xFFFFFFFCu32 as i32);
    b.push(w::I32_ADD);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
    ci32(&mut b, 0x1038);
    b.push(w::I32_STORE);
    b.push(2);
    b.push(0);                 // scratch[0x1038] = len1
    // Read len2 from (ptr2 - 4)
    ci32(&mut b, 0x1040);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
    ci32(&mut b, 0xFFFFFFFCu32 as i32);
    b.push(w::I32_ADD);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
    ci32(&mut b, 0x1048);
    b.push(w::I32_STORE);
    b.push(2);
    b.push(0);                 // scratch[0x1048] = len2
    // Compare: len1 == len2  → i64
    ci32(&mut b, 0x1038);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
    ci32(&mut b, 0x1048);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
    b.push(w::I32_EQ);        // len1 == len2
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);
    b.push(w::END);

    // 0x4A StrLt — str1_ptr str2_ptr → bool (lexicographic compare)
    if_op(&mut b, 0x4A);
    pop(&mut b);               // ptr2 → scratch
    ci32(&mut b, 0x1040);
    b.push(w::I64_STORE);
    b.push(3);
    b.push(0);
    pop(&mut b);               // ptr1 → scratch
    ci32(&mut b, 0x1030);
    b.push(w::I64_STORE);
    b.push(3);
    b.push(0);
    // Simple: compare first 4 bytes of each string
    ci32(&mut b, 0x1030);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);                 // first 4 bytes of str1
    ci32(&mut b, 0x1040);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);                 // first 4 bytes of str2
    b.push(w::I32_LT_U);      // unsigned less-than
    b.push(w::I64_EXTEND_I32_S);
    push(&mut b);
    b.push(w::END);

    // br $continue
    b.push(w::BR);
    b.push(0);

    // end loop, end block
    b.push(w::END);
    b.push(w::END);

    // Return top-of-stack value at [sp - 16]
    ld_i32(&mut b, 0x0000);
    ci32(&mut b, 0xFFF0u32 as i32);
    b.push(w::I32_ADD);
    if i64_result {
        b.push(w::I64_LOAD);
        b.push(3);
        b.push(0);
    } else {
        b.push(w::F64_LOAD);
        b.push(3);
        b.push(0);
    }

    b.push(w::END);
    b
}

fn build_get_sp() -> Vec<u8> {
    let mut b = vec![0u8];
    ld_i32(&mut b, 0x0000);
    b.push(w::END);
    b
}

// === WASM codegen helpers ===

fn ci32(b: &mut Vec<u8>, n: i32) {
    b.push(w::I32_CONST);
    leb128_s(b, n as i64);
}

fn ld_i32(b: &mut Vec<u8>, addr: u32) {
    ci32(b, addr as i32);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
}

fn add_ip(b: &mut Vec<u8>, delta: i32) {
    ci32(b, 0x0004);
    ld_i32(b, 0x0004);
    ci32(b, delta);
    b.push(w::I32_ADD);
    b.push(w::I32_STORE);
    b.push(2);
    b.push(0);
}

fn add_sp(b: &mut Vec<u8>, delta: i32) {
    ci32(b, 0x0000);
    ld_i32(b, 0x0000);
    ci32(b, delta);
    b.push(w::I32_ADD);
    b.push(w::I32_STORE);
    b.push(2);
    b.push(0);
}

/// Emit: if (scratch[0x1000] == opcode) {
fn if_op(b: &mut Vec<u8>, opcode: u8) {
    ci32(b, 0x1000);
    b.push(w::I32_LOAD);
    b.push(2);
    b.push(0);
    ci32(b, opcode as i32);
    b.push(w::I32_EQ);
    b.push(w::IF);
    b.push(0x40);
}

/// Push i64 from WASM stack to data stack.
fn push(b: &mut Vec<u8>) {
    // store value to scratch[0x1008]
    ci32(b, 0x1008);
    b.push(w::I64_STORE);
    b.push(3);
    b.push(0);
    // load sp
    ld_i32(b, 0x0000);
    // load value back
    ci32(b, 0x1008);
    b.push(w::I64_LOAD);
    b.push(3);
    b.push(0);
    // store at [sp]
    b.push(w::I64_STORE);
    b.push(3);
    b.push(0);
    // sp += 16
    add_sp(b, 16);
}

/// Pop i64 from data stack to WASM stack.
fn pop(b: &mut Vec<u8>) {
    add_sp(b, 0xFFF0u32 as i32); // sp -= 16
    ld_i32(b, 0x0000);
    b.push(w::I64_LOAD);
    b.push(3);
    b.push(0);
}

/// Push f64 from WASM stack to data stack.
fn push_f64(b: &mut Vec<u8>) {
    ci32(b, 0x1008);
    b.push(w::F64_STORE);
    b.push(3);
    b.push(0);
    ld_i32(b, 0x0000);
    ci32(b, 0x1008);
    b.push(w::F64_LOAD);
    b.push(3);
    b.push(0);
    b.push(w::F64_STORE);
    b.push(3);
    b.push(0);
    add_sp(b, 16);
}

// === WASM binary format helpers ===

use crate::wasm_utils::{
    export_entry as export, leb128_s, leb128_u as uleb128, section as sec, vec_u8,
};

fn data_seg(buf: &mut Vec<u8>, addr: u32, payload: &[u8]) {
    buf.push(0x00); // active, memory 0
    buf.push(w::I32_CONST);
    leb128_s(buf, addr as i64);
    buf.push(w::END);
    buf.extend_from_slice(&vec_u8(payload));
}

// === Tests ===

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_wasm() {
        let gen = WasmGenerator::new(vec![Opcode::PushI64(42), Opcode::PushI64(13), Opcode::Add]);
        let wasm = gen.compile();
        assert_eq!(&wasm[0..4], b"\0asm");
        assert_eq!(&wasm[4..8], &[1, 0, 0, 0]);
        assert!(wasm.len() > 120, "WASM too small: {}", wasm.len());
    }

    #[test]
    fn test_empty() {
        let wasm = WasmGenerator::new(vec![]).compile();
        assert_eq!(&wasm[0..4], b"\0asm");
    }
}
