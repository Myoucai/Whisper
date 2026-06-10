//! Native code generator: Whisper bytecode → standalone ELF64 executable.
//! Zero external dependencies — no assembler, compiler, or linker needed.
//!
//! Memory layout:
//!   0x400000        code segment (R+X)
//!   0x401000        embedded bytecode
//!   0x420000        alloc_ptr (8 bytes) — bump allocator for heap
//!   0x420008        call_sp (8 bytes) — call stack pointer
//!   0x420010        call_stack[64] — saved (r14, r13) pairs
//!   0x430000        output buffer (256 bytes)
//!   0x500000        heap (1MB) — strings and lists
//!   0x800000        data stack (grows down, 64KB)

use whisper_core::opcode::Opcode;

const PAGE_SIZE: u64 = 0x1000;
const STACK_SIZE: u64 = 0x10000;
const CODE_VADDR: u64 = 0x400000;
const HEAP_VADDR: u64 = 0x500000;
const HEAP_SIZE: u64 = 0x100000; // 1MB
const ALLOC_PTR_ADDR: u64 = 0x420000;
const CALL_SP_ADDR: u64 = 0x420008;
const CALL_STACK_ADDR: u64 = 0x420010;
const OUT_BUF_ADDR: u64 = 0x430000;
const WORD_TABLE_ADDR: u64 = 0x500000; // word dispatch table at start of data segment

// ── x86-64 encoder ──────────────────────────────────────────────────

struct X {
    v: Vec<u8>,
    handler_patches: Vec<(u8, usize)>,
    next_pos: usize,
}

impl X {
    fn new() -> Self {
        X {
            v: Vec::new(),
            handler_patches: vec![],
            next_pos: 0,
        }
    }
    fn b(&mut self, b: u8) {
        self.v.push(b);
    }
    fn i(&mut self, b: &[u8]) {
        self.v.extend_from_slice(b);
    }
    fn i32(&mut self, n: i32) {
        self.i(&n.to_le_bytes());
    }
    fn u64(&mut self, n: u64) {
        self.i(&n.to_le_bytes());
    }
    fn m(&mut self) -> usize {
        self.v.len()
    }
    fn p_i32(&mut self, pos: usize, val: i32) {
        self.v[pos..pos + 4].copy_from_slice(&val.to_le_bytes());
    }
    fn mark_next(&mut self) {
        self.next_pos = self.v.len();
    }
    // REX prefix helpers: W=1 always (64-bit), R extends reg, B extends r/m
    fn rex_w(&self) -> u8 { 0x48 } // REX.W only
    fn rex_wr(&self, reg: u8) -> u8 { 0x48 | ((reg >> 3) & 1) << 2 } // REX.W + R (extends reg field)
    fn rex_wb(&self, rm: u8) -> u8 { 0x48 | ((rm >> 3) & 1) } // REX.W + B (extends r/m field)
    fn rex_wrb(&self, reg: u8, rm: u8) -> u8 { 0x48 | ((reg >> 3) & 1) << 2 | ((rm >> 3) & 1) } // REX.W + R + B

    fn mov_r64i(&mut self, r: u8, v: u64) {
        // mov r64, imm64: REX.W[B] + B8+rd
        if r >= 8 { self.b(0x49); } else { self.b(0x48); }
        self.b(0xB8 | (r & 7));
        self.u64(v);
    }
    fn mov_rr(&mut self, d: u8, s: u8) {
        // mov r64, r64: REX.WRB + 89 /r (mod=11, reg=s, r/m=d)
        self.b(self.rex_wrb(s, d));
        self.b(0x89);
        self.b(0xC0 | ((s & 7) << 3) | (d & 7));
    }
    fn mov_rm(&mut self, d: u8, b: u8, o: i32) {
        // mov r64, [r64+disp32]: REX.WRB + 8B /r (mod=10, reg=d, r/m=b)
        self.b(self.rex_wrb(d, b));
        self.b(0x8B);
        self.b(0x80 | ((d & 7) << 3) | (b & 7));
        self.i32(o);
    }
    fn mov_mr(&mut self, b: u8, o: i32, s: u8) {
        // mov [r64+disp32], r64: REX.WRB + 89 /r (mod=10, reg=s, r/m=b)
        self.b(self.rex_wrb(s, b));
        self.b(0x89);
        self.b(0x80 | ((s & 7) << 3) | (b & 7));
        self.i32(o);
    }
    fn add_ri(&mut self, r: u8, n: i32) {
        if n == 1 {
            // inc r64: REX.WB + FF /0 (mod=11, reg=0, r/m=r)
            self.b(self.rex_wb(r));
            self.b(0xFF);
            self.b(0xC0 | (r & 7));
        } else {
            // add r64, imm32: REX.WB + 81 /0 (mod=11, reg=0, r/m=r)
            self.b(self.rex_wb(r));
            self.b(0x81);
            self.b(0xC0 | (r & 7));
            self.i32(n);
        }
    }
    fn sub_ri(&mut self, r: u8, n: i32) {
        if n == 1 {
            // dec r64: REX.WB + FF /1 (mod=11, reg=1, r/m=r)
            self.b(self.rex_wb(r));
            self.b(0xFF);
            self.b(0xC8 | (r & 7));
        } else {
            // sub r64, imm32: REX.WB + 81 /5 (mod=11, reg=5, r/m=r)
            self.b(self.rex_wb(r));
            self.b(0x81);
            self.b(0xE8 | (r & 7));
            self.i32(n);
        }
    }
    fn push_r(&mut self, r: u8) {
        // push r64: REX.B + 50+rd (only needs REX if r8-r15)
        if r >= 8 { self.b(0x41); }
        self.b(0x50 | (r & 7));
    }
    fn pop_r(&mut self, r: u8) {
        // pop r64: REX.B + 58+rd
        if r >= 8 { self.b(0x41); }
        self.b(0x58 | (r & 7));
    }
    fn xor_rr(&mut self, a: u8, b: u8) {
        // xor r64, r64: REX.WRB + 31 /r (mod=11, reg=b, r/m=a)
        self.b(self.rex_wrb(b, a));
        self.b(0x31);
        self.b(0xC0 | ((b & 7) << 3) | (a & 7));
    }
    fn je(&mut self) -> usize {
        self.b(0x0F);
        self.b(0x84);
        let p = self.v.len();
        self.i32(0);
        p
    }
    fn je8(&mut self) -> usize {
        self.b(0x74);
        let p = self.v.len();
        self.b(0);
        p
    }
    fn jne8(&mut self) -> usize {
        self.b(0x75);
        let p = self.v.len();
        self.b(0);
        p
    }
    fn jmp(&mut self) -> usize {
        self.b(0xE9);
        let p = self.v.len();
        self.i32(0);
        p
    }
    fn jmp8(&mut self) -> usize {
        self.b(0xEB);
        let p = self.v.len();
        self.b(0);
        p
    }
    fn jne(&mut self) -> usize {
        self.b(0x0F);
        self.b(0x85);
        let p = self.v.len();
        self.i32(0);
        p
    }
    fn jle8(&mut self) -> usize {
        self.b(0x7E);
        let p = self.v.len();
        self.b(0);
        p
    }
    fn jb8(&mut self) -> usize {
        self.b(0x72);
        let p = self.v.len();
        self.b(0);
        p
    }
    fn ja8(&mut self) -> usize {
        self.b(0x77);
        let p = self.v.len();
        self.b(0);
        p
    }
    fn jge8(&mut self) -> usize {
        self.b(0x7D);
        let p = self.v.len();
        self.b(0);
        p
    }
    fn jg8(&mut self) -> usize {
        self.b(0x7F);
        let p = self.v.len();
        self.b(0);
        p
    }
    fn syscall(&mut self) {
        self.i(&[0x0F, 0x05]);
    }
    fn ret(&mut self) {
        self.b(0xC3);
    }
    fn op(&mut self, opcode: u8) {
        self.b(0x3C);
        self.b(opcode);
        let p = self.je();
        self.handler_patches.push((opcode, p));
    }
    fn done(&mut self) {
        let p = self.jmp();
        self.p_i32(p, self.next_pos as i32 - (p + 4) as i32);
    }
    fn patch_handler(&mut self, opcode: u8) -> usize {
        let pos = self.m();
        let patch = self.handler(opcode);
        self.p_i32(patch, pos as i32 - (patch + 4) as i32);
        pos
    }
    fn handler(&mut self, opcode: u8) -> usize {
        for (o, p) in &self.handler_patches {
            if *o == opcode {
                return *p;
            }
        }
        panic!("handler not found for {opcode:02X}");
    }
    fn back(&mut self) {
        let p = self.jmp();
        self.p_i32(p, self.next_pos as i32 - (p + 4) as i32);
    }
    fn patch_jmp_rel8(&mut self, j: usize) {
        self.v[j] = (self.m() - j - 1) as u8;
    }
    fn patch_jmp_rel32(&mut self, j: usize) {
        let m = self.m();
        self.p_i32(j, m as i32 - (j + 4) as i32);
    }
}

// ── Entry point ─────────────────────────────────────────────────────

pub fn compile_to_native(bytecode: &[Opcode], defs: &[(String, Vec<Opcode>)]) -> Vec<u8> {
    let raw_bc = raw_bytecode(bytecode);

    // Build word table: serialize names and bytecodes for embedding in ELF.
    // Each entry: { name_ptr: u32, name_len: u32, bc_ptr: u32, bc_len: u32 }
    // Stored as absolute addresses (patched after ELF layout is known).
    let mut word_entries: Vec<(String, Vec<u8>)> = Vec::new();
    for (name, code) in defs {
        word_entries.push((name.clone(), raw_bytecode(code)));
    }

    let mut x = X::new();

    // ── _start: register setup ───────────────────────────────────
    x.mov_r64i(15, CODE_VADDR + 0x80000 - 8); // r15 = stack top
    x.mov_r64i(14, CODE_VADDR + 0x1000); // r14 = bytecode base
    x.xor_rr(13, 13); // r13 = 0 (ip)
    // Initialize alloc_ptr
    x.mov_r64i(0, HEAP_VADDR);
    x.i(&[0x49, 0xA3]); // mov [ALLOC_PTR_ADDR], rax (uses REX.WR prefix)
    // Actually, let's use a simpler approach: store via indirect
    x.mov_r64i(8, ALLOC_PTR_ADDR);
    x.mov_mr(8, 0, 0); // [r8] = rax = HEAP_VADDR

    // ── Main interpreter loop ────────────────────────────────────
    x.mark_next();
    // Fetch: al = bytecode[ip]; ip++
    x.i(&[0x43, 0x0F, 0xB6, 0x04, 0x2E]); // movzx eax, [r14+r13]
    x.add_ri(13, 1);

    // Register all opcodes for dispatch
    register_all_opcodes(&mut x);
    x.done();

    // ── Implement all handlers ───────────────────────────────────
    impl_stack_ops(&mut x);
    impl_arith_ops(&mut x);
    impl_cmp_ops(&mut x);
    impl_logic_ops(&mut x);
    impl_push_ops(&mut x, &raw_bc);
    impl_list_ops(&mut x);
    impl_string_ops(&mut x);
    impl_control_ops(&mut x);
    // CALL/RETURN: pass word table info (will be at a known address in data segment)
    // The actual address is WORD_TABLE_ADDR. The table is built in build_elf.
    let word_count = word_entries.len();
    impl_call_return(&mut x, WORD_TABLE_ADDR, word_count);
    impl_io_ops(&mut x);
    impl_float_ops(&mut x);
    impl_misc_ops(&mut x);

    // ── Helper routines ──────────────────────────────────────────
    let itoa_addr = impl_itoa(&mut x);
    let str_eq_addr = impl_str_eq(&mut x);
    let str_cmp_addr = impl_str_cmp(&mut x);
    let alloc_addr = impl_alloc(&mut x);
    let run_ref_addr = impl_run_ref(&mut x, &raw_bc);

    // Patch helper call sites (marker 0=itoa, 1=alloc, 2=run_ref)
    patch_helper_calls(&mut x, itoa_addr, str_eq_addr, str_cmp_addr, alloc_addr, run_ref_addr);

    // ── Build ELF ────────────────────────────────────────────────
    build_elf(&x.v, &raw_bc, &word_entries)
}

// ── Opcode registration ─────────────────────────────────────────────

fn register_all_opcodes(x: &mut X) {
    // Stack
    for op in [0x00, 0x01, 0x02, 0x03, 0x04] { x.op(op); }
    // Arith
    for op in [0x10, 0x11, 0x12, 0x13, 0x14] { x.op(op); }
    // Compare
    for op in [0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D] { x.op(op); }
    // Logic
    for op in [0x20, 0x21, 0x22] { x.op(op); }
    // Literals
    for op in [0x30, 0x31, 0x32, 0x33, 0x34, 0x35] { x.op(op); }
    // List ops
    for op in [0x40, 0x41, 0x42, 0x43, 0x44, 0x45] { x.op(op); }
    // String ops 0x46-0x4F
    for op in 0x46..=0x4F { x.op(op); }
    // Control
    for op in [0x50, 0x51, 0x52, 0x53] { x.op(op); }
    // Call/Return
    for op in [0x60, 0x61] { x.op(op); }
    // Capability
    for op in [0x70, 0x71] { x.op(op); }
    // Confidence
    for op in [0x80, 0x81] { x.op(op); }
    // IO
    for op in [0x90, 0x91, 0x92] { x.op(op); }
    // Definitions (no-ops at runtime)
    for op in [0xA0, 0xA1, 0xA2, 0xA3] { x.op(op); }
    // Float ops
    for op in [0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5] { x.op(op); }
    // JSON
    for op in [0xB6, 0xB7] { x.op(op); }
    // Extended string ops
    for op in [0xB8, 0xB9, 0xBA, 0xBB, 0xBC] { x.op(op); }
    // Bytes
    for op in [0xBD, 0xBE, 0xBF, 0xC0] { x.op(op); }
    // Try
    x.op(0xC1);
}

// ── Stack operations ────────────────────────────────────────────────

fn impl_stack_ops(x: &mut X) {
    // DUP (0x00)
    x.patch_handler(0x00);
    x.mov_rm(0, 15, 0);
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    x.back();

    // SWAP (0x01)
    x.patch_handler(0x01);
    x.mov_rm(0, 15, 0);
    x.mov_rm(1, 15, 8);
    x.mov_mr(15, 8, 0);
    x.mov_mr(15, 0, 1);
    x.back();

    // DROP (0x02)
    x.patch_handler(0x02);
    x.add_ri(15, 8);
    x.back();

    // ROT (0x03)
    x.patch_handler(0x03);
    x.mov_rm(0, 15, 0);
    x.mov_rm(1, 15, 8);
    x.mov_rm(2, 15, 16);
    x.mov_mr(15, 0, 1);
    x.mov_mr(15, 8, 0);
    x.mov_mr(15, 16, 2);
    x.back();

    // PICK (0x04)
    x.patch_handler(0x04);
    x.i(&[0x43, 0x0F, 0xB6, 0x04, 0x2E]); // movzx eax, [r14+r13]
    x.add_ri(13, 1);
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
    x.i(&[0x49, 0x01, 0xF8]); // add rax, r15
    x.mov_rm(0, 0, 0); // rax = [rax]
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    x.back();
}

// ── Arithmetic operations ───────────────────────────────────────────

fn impl_arith_ops(x: &mut X) {
    // ADD (0x10)
    x.patch_handler(0x10);
    x.mov_rm(0, 15, 0);
    x.add_ri(15, 8);
    x.i(&[0x49, 0x01, 0x07]); // add [r15], rax
    x.back();

    // SUB (0x11)
    x.patch_handler(0x11);
    x.mov_rm(0, 15, 0);
    x.add_ri(15, 8);
    x.i(&[0x49, 0x29, 0x07]); // sub [r15], rax
    x.back();

    // MUL (0x12)
    x.patch_handler(0x12);
    x.mov_rm(0, 15, 0);
    x.add_ri(15, 8);
    x.i(&[0x49, 0x0F, 0xAF, 0x07]); // imul rax, [r15]
    x.mov_mr(15, 0, 0);
    x.back();

    // DIV (0x13)
    x.patch_handler(0x13);
    x.mov_rm(0, 15, 0); // rax = b (top = divisor)
    x.add_ri(15, 8);
    x.mov_rm(1, 15, 0); // rcx = a (new top = dividend)
    x.i(&[0x48, 0x85, 0xC0]); // test rax, rax (check divisor)
    let ok2 = x.jne8();
    x.mov_r64i(0, 60); x.xor_rr(7, 7); x.syscall(); // exit(1)
    x.patch_jmp_rel8(ok2);
    x.mov_rr(7, 0); // rdi = divisor (save)
    x.mov_rr(0, 1); // rax = dividend
    x.i(&[0x48, 0x99]); // cqo
    x.i(&[0x48, 0xF7, 0xFF]); // idiv rdi
    x.mov_mr(15, 0, 0); // [r15] = quotient (rax)
    x.back();
}

// ── Comparison operations ───────────────────────────────────────────

fn impl_cmp_ops(x: &mut X) {
    let cmps: [(u8, u8); 6] = [
        (0x18, 0x94), // EQ  → sete
        (0x19, 0x9C), // LT  → setl
        (0x1A, 0x9F), // GT  → setg
        (0x1B, 0x95), // NEQ → setne
        (0x1C, 0x9E), // LE  → setle
        (0x1D, 0x9D), // GE  → setge
    ];
    for (opc, cc) in &cmps {
        x.patch_handler(*opc);
        x.mov_rm(0, 15, 0);
        x.add_ri(15, 8);
        x.i(&[0x49, 0x39, 0x07]); // cmp [r15], rax
        x.b(0x0F);
        x.b(*cc);
        x.b(0xC0); // setcc al
        x.i(&[0x48, 0x0F, 0xB6, 0xC0]); // movzx rax, al
        x.mov_mr(15, 0, 0);
        x.back();
    }
}

// ── Logic operations ────────────────────────────────────────────────

fn impl_logic_ops(x: &mut X) {
    // AND (0x20)
    x.patch_handler(0x20);
    x.mov_rm(0, 15, 0);
    x.add_ri(15, 8);
    x.i(&[0x49, 0x21, 0x07]); // and [r15], rax
    x.back();

    // OR (0x21)
    x.patch_handler(0x21);
    x.mov_rm(0, 15, 0);
    x.add_ri(15, 8);
    x.i(&[0x49, 0x09, 0x07]); // or [r15], rax
    x.back();

    // NOT (0x22)
    x.patch_handler(0x22);
    x.i(&[0x49, 0x83, 0x37, 0x01]); // xor qword [r15], 1
    x.back();
}

// ── Push operations ─────────────────────────────────────────────────

fn impl_push_ops(x: &mut X, _raw_bc: &[u8]) {
    // PUSH_I64 (0x30)
    x.patch_handler(0x30);
    x.i(&[0x4B, 0x8B, 0x04, 0x2E]); // mov rax, [r14+r13]
    x.add_ri(13, 8);
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    x.back();

    // PUSH_F64 (0x31) — same as PUSH_I64 (raw 8 bytes)
    x.patch_handler(0x31);
    x.i(&[0x4B, 0x8B, 0x04, 0x2E]);
    x.add_ri(13, 8);
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    x.back();

    // PUSH_STR (0x32) — push address of string data in bytecode
    x.patch_handler(0x32);
    // Read length from bytecode: eax = [r14+r13]
    x.i(&[0x47, 0x8B, 0x04, 0x2E]);
    // Save length to r8
    x.mov_rr(8, 0);
    // Compute string address: rax = r14 + r13 + 4
    x.mov_rr(0, 13);
    x.i(&[0x4C, 0x01, 0xF0]); // add rax, r14
    x.add_ri(0, 4);
    // Push address
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    // Advance ip: ip += 4 + length
    x.add_ri(13, 4);
    x.i(&[0x4D, 0x01, 0xC5]); // add r13, r8
    x.back();

    // PUSH_BOOL (0x33)
    x.patch_handler(0x33);
    x.i(&[0x43, 0x0F, 0xB6, 0x04, 0x2E]); // movzx eax, [r14+r13]
    x.add_ri(13, 1);
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    x.back();

    // PUSH_LIST (0x34) — pop count, allocate list in heap, copy elements
    // Heap layout: [count:8B] [elem0:8B] [elem1:8B] ...
    // Stack grows down: [r15]=last_pushed, [r15+8]=second_last, ...
    // List order: elem0=first_pushed (deepest), elemN=last_pushed (top)
    // So heap[i] = stack[count-1-i] = [r15 + (count-1-i)*8]
    x.patch_handler(0x34);
    x.mov_rm(0, 15, 0); // rax = count
    x.add_ri(15, 8); // pop count
    x.mov_rr(9, 0); // r9 = count
    // Allocate (count+1)*8 bytes
    x.add_ri(0, 1);
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
    x.mov_rr(7, 0);
    x.push_r(9); x.push_r(14); x.push_r(13);
    x.b(0xE8); x.i32(1); // call alloc
    x.pop_r(13); x.pop_r(14); x.pop_r(9);
    x.mov_rr(8, 0); // r8 = list_ptr
    x.mov_mr(8, 0, 9); // [r8] = count
    // Copy elements in correct order: heap[i] = [r15 + (count-1-i)*8]
    x.xor_rr(10, 10); // r10 = i (heap index)
    let pl_loop = x.m();
    x.i(&[0x4D, 0x39, 0xCA]); // cmp r10, r9
    let pl_done = x.jge8();
    // stack_idx = count - 1 - i
    x.mov_rr(0, 9); // rax = count
    x.sub_ri(0, 1); // rax = count - 1
    x.i(&[0x4C, 0x29, 0xD0]); // sub rax, r10 (rax = count-1-i)
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
    x.i(&[0x49, 0x01, 0xF8]); // add rax, r15
    x.mov_rm(0, 0, 0); // rax = [r15 + (count-1-i)*8]
    // Write to heap: [r8 + 8 + i*8]
    x.mov_rr(11, 10);
    x.i(&[0x49, 0xC1, 0xE3, 0x03]); // shl r11, 3
    x.i(&[0x4D, 0x01, 0xC3]); // add r11, r8
    x.add_ri(11, 8);
    x.mov_mr(11, 0, 0);
    x.add_ri(10, 1);
    let pl_back = x.jmp();
    x.p_i32(pl_back, pl_loop as i32 - (pl_back + 4) as i32);
    x.patch_jmp_rel8(pl_done);
    // Adjust stack: remove count elements, push list pointer
    x.mov_rr(0, 9); // rax = count
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
    x.i(&[0x49, 0x01, 0xC7]); // add r15, rax (remove elements)
    x.sub_ri(15, 8); // push list pointer
    x.mov_mr(15, 0, 8); // [r15] = r8 (list_ptr)
    x.back();

    // PUSH_REF (0x35) — push address of inline bytecode
    x.patch_handler(0x35);
    // Read the ref bytecode length
    x.i(&[0x47, 0x8B, 0x04, 0x2E]); // mov eax, [r14+r13]
    x.mov_rr(8, 0); // r8 = length
    // Compute address of ref bytecode: rax = r14 + r13 + 4
    x.mov_rr(0, 13);
    x.i(&[0x4C, 0x01, 0xF0]); // add rax, r14
    x.add_ri(0, 4);
    // Push address
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    // Advance ip: ip += 4 + length
    x.add_ri(13, 4);
    x.i(&[0x4D, 0x01, 0xC5]); // add r13, r8
    x.back();
}

// ── List operations ─────────────────────────────────────────────────

fn impl_list_ops(x: &mut X) {
    // NTH (0x40) — list_ptr idx → element
    x.patch_handler(0x40);
    x.mov_rm(0, 15, 0); // rax = idx
    x.add_ri(15, 8); // pop idx
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3 (idx * 8)
    x.i(&[0x49, 0x03, 0x07]); // add rax, [r15] (rax = list_ptr + idx*8)
    x.add_ri(0, 8); // skip count field
    x.mov_rm(0, 0, 0); // rax = [rax] (element value)
    x.mov_mr(15, 0, 0); // [r15] = element (reuse stack slot)
    x.back();

    // APPEND (0x41) — list_ptr elem → new_list_ptr
    x.patch_handler(0x41);
    x.mov_rm(0, 15, 0); // rax = elem
    x.add_ri(15, 8); // pop elem
    // Save elem to [r15] temporarily (we'll overwrite this slot with result later)
    x.mov_mr(15, 0, 0); // [r15] = elem (temporary save)
    x.mov_rm(1, 15, 8); // rcx = old list_ptr (at [r15+8])
    x.mov_rm(2, 1, 0); // rdx = old count
    // Allocate (old_count+2)*8
    x.mov_rr(0, 2);
    x.add_ri(0, 2);
    x.i(&[0x48, 0xC1, 0xE0, 0x03]);
    x.mov_rr(7, 0);
    x.push_r(14); x.push_r(13); x.push_r(1); x.push_r(2);
    x.b(0xE8); x.i32(1); // call alloc
    x.pop_r(2); x.pop_r(1); x.pop_r(13); x.pop_r(14);
    // rax = new_ptr, rcx = old_ptr, rdx = old_count
    x.mov_rr(8, 0); // r8 = new_ptr
    x.mov_mr(8, 0, 2); // write old_count as new count (old_count = old_count, same value)
    // Actually, new count = old_count + 1. But rdx = old_count.
    // The new element adds 1, so new count should be old_count + 1.
    // But APPEND creates a new list with all old elements + 1 new element.
    // So new count = old_count + 1. But we wrote old_count. Fix:
    x.mov_rm(0, 8, 0); // rax = old count (from what we just wrote)
    x.add_ri(0, 1);
    x.mov_mr(8, 0, 0); // write correct new count
    // Copy old elements
    x.xor_rr(10, 10);
    let ap_loop = x.m();
    x.i(&[0x4D, 0x39, 0xD2]); // cmp r10, rdx
    let ap_done = x.jge8();
    // Read: rax = [rcx + 8 + r10*8]
    x.mov_rr(0, 10);
    x.i(&[0x48, 0xC1, 0xE0, 0x03]);
    x.i(&[0x48, 0x01, 0xC8]);
    x.add_ri(0, 8);
    x.mov_rm(0, 0, 0);
    // Write: [r8 + 8 + r10*8] = rax
    x.mov_rr(11, 10);
    x.i(&[0x49, 0xC1, 0xE3, 0x03]);
    x.i(&[0x4D, 0x01, 0xC3]);
    x.add_ri(11, 8);
    x.mov_mr(11, 0, 0);
    x.add_ri(10, 1);
    let ap_back = x.jmp();
    x.p_i32(ap_back, ap_loop as i32 - (ap_back + 4) as i32);
    x.patch_jmp_rel8(ap_done);
    // Write new element at [r8 + 8 + old_count*8]
    x.mov_rr(0, 2); // rax = old_count
    x.i(&[0x48, 0xC1, 0xE0, 0x03]);
    x.i(&[0x4C, 0x01, 0xC0]); // add rax, r8
    x.add_ri(0, 8);
    x.mov_rm(1, 15, 0); // rcx = elem (saved at [r15])
    x.mov_mr(0, 0, 1); // store elem
    // Replace old list_ptr on stack with new_ptr
    x.mov_mr(15, 8, 8); // [r15+8] = new_ptr
    // Remove elem from stack (it was at [r15], old list at [r15+8])
    // Actually, we want: stack has new_list_ptr at top
    // Current: [r15] = elem(saved), [r15+8] = old_list_ptr
    // After: [r15] = new_list_ptr
    x.add_ri(15, 8); // remove saved elem, now [r15] = old_list_ptr slot
    x.mov_mr(15, 0, 8); // [r15] = new_ptr
    x.back();

    // LEN (0x42) — list_ptr → count
    x.patch_handler(0x42);
    x.mov_rm(0, 15, 0); // rax = list_ptr
    x.mov_rm(0, 0, 0); // rax = [rax] = count
    x.mov_mr(15, 0, 0); // [r15] = count
    x.back();

    // MAP (0x43) — list_ptr ref_bc_ptr → new_list_ptr
    x.patch_handler(0x43);
    // Skip inline ref bytecode
    x.i(&[0x47, 0x8B, 0x04, 0x2E]); // mov eax, [r14+r13]
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]); // add r13, rax
    // Stack: [... list_ptr, ref_bc_addr]
    x.mov_rm(9, 15, 0); // r9 = ref_bc_addr
    x.mov_rm(10, 15, 8); // r10 = list_ptr
    x.add_ri(15, 16); // pop both
    x.mov_rm(11, 10, 0); // r11 = count
    // Allocate result list: (count+1)*8
    x.mov_rr(0, 11);
    x.add_ri(0, 1);
    x.i(&[0x48, 0xC1, 0xE0, 0x03]);
    x.mov_rr(7, 0);
    x.push_r(9); x.push_r(10); x.push_r(11);
    x.b(0xE8); x.i32(1); // call alloc (marker 1)
    x.pop_r(11); x.pop_r(10); x.pop_r(9);
    // rax = result list ptr
    x.mov_rr(8, 0); // r8 = result list ptr
    x.mov_mr(8, 0, 11); // write count
    // Loop: for i=0..count-1
    x.xor_rr(12, 12); // r12 = i
    let map_loop = x.m();
    x.i(&[0x4D, 0x39, 0xDC]); // cmp r12, r11
    let map_done = x.jge8();
    // Push element onto data stack
    x.mov_rr(0, 12);
    x.i(&[0x48, 0xC1, 0xE0, 0x03]);
    x.i(&[0x49, 0x01, 0xD0]); // add rax, r10
    x.add_ri(0, 8);
    x.mov_rm(0, 0, 0); // rax = element
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    // Call run_ref: rdi = ref_bc_addr, rsi = ref_len
    x.mov_rr(7, 9); // rdi = ref_bc_addr
    x.mov_r64i(6, 0xFFFF); // rsi = large (RETURN breaks out)
    x.push_r(8); x.push_r(9); x.push_r(10); x.push_r(11); x.push_r(12);
    x.b(0xE8); x.i32(2); // call run_ref (marker 2)
    x.pop_r(12); x.pop_r(11); x.pop_r(10); x.pop_r(9); x.pop_r(8);
    // Result is on data stack top
    x.mov_rm(0, 15, 0); // rax = result
    x.add_ri(15, 8); // pop result
    // Store in result list at [r8 + 8 + i*8]
    x.mov_rr(1, 12);
    x.i(&[0x48, 0xC1, 0xE1, 0x03]);
    x.i(&[0x49, 0x01, 0xC1]); // add rcx, r8
    x.add_ri(1, 8);
    x.mov_mr(1, 0, 0); // store
    x.add_ri(12, 1);
    let map_back = x.jmp();
    x.p_i32(map_back, map_loop as i32 - (map_back + 4) as i32);
    x.patch_jmp_rel8(map_done);
    // Push result list ptr
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 8);
    x.back();

    // EACH (0x44) — list_ptr ref_bc_ptr → (nothing)
    x.patch_handler(0x44);
    // Skip inline ref
    x.i(&[0x47, 0x8B, 0x04, 0x2E]);
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]);
    // Stack: [... list_ptr, ref_bc_addr]
    x.mov_rm(9, 15, 0); // r9 = ref_bc_addr
    x.mov_rm(10, 15, 8); // r10 = list_ptr
    x.add_ri(15, 16); // pop both
    x.mov_rm(11, 10, 0); // r11 = count
    // Loop: for i=0..count-1
    x.xor_rr(12, 12);
    let each_loop = x.m();
    x.i(&[0x4D, 0x39, 0xDC]); // cmp r12, r11
    let each_done = x.jge8();
    // Push element
    x.mov_rr(0, 12);
    x.i(&[0x48, 0xC1, 0xE0, 0x03]);
    x.i(&[0x49, 0x01, 0xD0]);
    x.add_ri(0, 8);
    x.mov_rm(0, 0, 0);
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    // Call run_ref
    x.mov_rr(7, 9);
    x.mov_r64i(6, 0xFFFF);
    x.push_r(8); x.push_r(9); x.push_r(10); x.push_r(11); x.push_r(12);
    x.b(0xE8); x.i32(2); // call run_ref
    x.pop_r(12); x.pop_r(11); x.pop_r(10); x.pop_r(9); x.pop_r(8);
    // Discard result
    x.add_ri(15, 8);
    x.add_ri(12, 1);
    let each_back = x.jmp();
    x.p_i32(each_back, each_loop as i32 - (each_back + 4) as i32);
    x.patch_jmp_rel8(each_done);
    x.back();

    // FOLD (0x45) — list_ptr init ref_bc_ptr → result
    x.patch_handler(0x45);
    // Skip inline ref
    x.i(&[0x47, 0x8B, 0x04, 0x2E]);
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]);
    // Stack: [... list_ptr, init, ref_bc_addr]
    x.mov_rm(9, 15, 0); // r9 = ref_bc_addr
    x.mov_rm(0, 15, 8); // rax = init
    x.mov_rm(10, 15, 16); // r10 = list_ptr
    x.add_ri(15, 24); // pop all three
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0); // push init (accumulator)
    x.mov_rm(11, 10, 0); // r11 = count
    // Loop: for i=0..count-1
    x.xor_rr(12, 12);
    let fold_loop = x.m();
    x.i(&[0x4D, 0x39, 0xDC]);
    let fold_done = x.jge8();
    // Push element
    x.mov_rr(0, 12);
    x.i(&[0x48, 0xC1, 0xE0, 0x03]);
    x.i(&[0x49, 0x01, 0xD0]);
    x.add_ri(0, 8);
    x.mov_rm(0, 0, 0);
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    // Call run_ref (body sees [accumulator, element])
    x.mov_rr(7, 9);
    x.mov_r64i(6, 0xFFFF);
    x.push_r(8); x.push_r(9); x.push_r(10); x.push_r(11); x.push_r(12);
    x.b(0xE8); x.i32(2);
    x.pop_r(12); x.pop_r(11); x.pop_r(10); x.pop_r(9); x.pop_r(8);
    // New accumulator is on stack top (body consumed old acc + elem, pushed new)
    x.add_ri(12, 1);
    let fold_back = x.jmp();
    x.p_i32(fold_back, fold_loop as i32 - (fold_back + 4) as i32);
    x.patch_jmp_rel8(fold_done);
    // Final accumulator is on stack
    x.back();
}

// ── String operations ───────────────────────────────────────────────

fn impl_string_ops(x: &mut X) {
    // STRLEN (0x46)
    x.patch_handler(0x46);
    x.mov_rm(0, 15, 0); // rax = string addr
    x.i(&[0x48, 0x83, 0xE8, 0x04]); // sub rax, 4
    x.i(&[0x8B, 0x00]); // mov eax, [rax]
    x.i(&[0x48, 0x98]); // cdqe
    x.mov_mr(15, 0, 0);
    x.back();

    // STRCAT (0x47) — s1 s2 → s3 (concatenated)
    x.patch_handler(0x47);
    x.mov_rm(6, 15, 0); // r6 = s2 addr
    x.mov_rm(7, 15, 8); // r7 = s1 addr
    x.add_ri(15, 8); // pop s2, reuse slot
    // Read len1 from [s1-4], len2 from [s2-4]
    x.mov_rm(0, 7, -4); // rax = len1
    x.mov_rm(1, 6, -4); // rcx = len2
    // Allocate len1+len2+5 bytes (4 for length prefix + data + null)
    x.mov_rr(2, 0); // rdx = len1
    x.i(&[0x48, 0x01, 0xC8]); // add rax, rcx (len1+len2)
    x.add_ri(0, 5); // +5 (4 prefix + 1 null)
    x.mov_rr(7, 0); // rdi = alloc size
    x.push_r(14); x.push_r(13); x.push_r(6); x.push_r(2);
    x.b(0xE8); x.i32(1); // call alloc
    x.pop_r(2); x.pop_r(6); x.pop_r(13); x.pop_r(14);
    // rax = alloc'd ptr (points to length prefix)
    // Write total length: len1+len2
    x.mov_rr(8, 0); // r8 = new str ptr
    x.mov_rm(0, 7, -4); // rax = len1
    x.mov_rm(1, 6, -4); // rcx = len2
    x.i(&[0x48, 0x01, 0xC8]); // add rax, rcx
    x.i(&[0x41, 0x89, 0x00]); // mov [r8], eax (write 4-byte length)
    // Copy s1 data
    x.add_ri(8, 4); // r8 points to data area
    // memcpy(r8, s1, len1) — use rep movsb
    x.mov_rr(7, 6); // rdi = r8? No, rdi is already used. Let me use different regs.
    // Actually: r8 = dest, s1 = r7 (original s1 addr), len = rdx
    // rep movsb: rdi=dest, rsi=src, rcx=count
    x.push_r(0); // save rax
    x.i(&[0x49, 0x89, 0xC7]); // mov rdi, r8 (dest)
    x.i(&[0x49, 0x89, 0xDE]); // mov rsi, r7? No — r6=s2, r7=s1
    // Wait, I used r7 for s1 addr. But rdi is register 7. Conflict!
    // Let me use different registers: s1 in r12, s2 in r13? But r13=ip.
    // I'll use r9 for s1, r10 for s2.
    // Actually, let me just do byte-by-byte copy with a loop.
    // Copy s1: for i=0..len1-1: [r8+i] = [s1+i]
    x.xor_rr(9, 9); // r9 = 0 (index)
    let sc_loop1 = x.m();
    x.mov_rm(0, 7, -4); // rax = len1
    x.i(&[0x4C, 0x39, 0xC8]); // cmp rax, r9
    let sc_done1 = x.jge8();
    // Read byte: al = [s1 + r9]
    x.mov_rr(0, 7);
    x.i(&[0x4C, 0x01, 0xC8]); // add rax, r9
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    // Write byte: [r8 + r9] = al
    x.mov_rr(1, 8);
    x.i(&[0x4C, 0x01, 0xC9]); // add rcx, r9
    x.i(&[0x88, 0x01]); // mov [rcx], al
    x.add_ri(9, 1);
    let sc_back1 = x.jmp();
    x.p_i32(sc_back1, sc_loop1 as i32 - (sc_back1 + 4) as i32);
    x.patch_jmp_rel8(sc_done1);
    // Now r8 still points to data area, r9 = len1
    // Copy s2: for i=0..len2-1: [r8+len1+i] = [s2+i]
    x.xor_rr(10, 10); // r10 = 0
    let sc_loop2 = x.m();
    x.mov_rm(0, 6, -4); // rax = len2
    x.i(&[0x4C, 0x39, 0xD0]); // cmp rax, r10
    let sc_done2 = x.jge8();
    // Read byte from s2
    x.mov_rr(0, 6);
    x.i(&[0x4C, 0x01, 0xD0]); // add rax, r10
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    // Write to [r8 + len1 + r10]
    x.mov_rr(1, 8);
    x.i(&[0x4C, 0x01, 0xC9]); // add rcx, r9 (r9 = len1)
    x.i(&[0x4C, 0x01, 0xD1]); // add rcx, r10
    x.i(&[0x88, 0x01]); // mov [rcx], al
    x.add_ri(10, 1);
    let sc_back2 = x.jmp();
    x.p_i32(sc_back2, sc_loop2 as i32 - (sc_back2 + 4) as i32);
    x.patch_jmp_rel8(sc_done2);
    // Write null terminator
    x.mov_rr(0, 8);
    x.i(&[0x4C, 0x01, 0xC8]); // add rax, r9 (data + len1)
    x.mov_rm(1, 6, -4); // rcx = len2
    x.i(&[0x48, 0x01, 0xC8]); // add rax, rcx
    x.i(&[0xC6, 0x00, 0x00]); // mov byte [rax], 0
    // Push new string address (r8 points to data area, which is alloc+4)
    // Wait — r8 was set to alloc'd ptr + 4 earlier. But alloc'd ptr is the start.
    // The string address should be alloc'd_ptr + 4 (past the length prefix).
    // But we already did add_ri(8, 4). So r8 = data area. Good.
    x.pop_r(0); // restore rax
    x.mov_mr(15, 0, 8); // [r15] = r8 (string data addr)
    x.back();

    // STRSLICE (0x48) — str start len → substr
    x.patch_handler(0x48);
    x.mov_rm(0, 15, 0); // rax = len
    x.mov_rm(1, 15, 8); // rcx = start
    x.mov_rm(2, 15, 16); // rdx = str addr
    x.add_ri(15, 16); // pop start and len, reuse slot for result
    // Clamp: if start < 0, start = 0
    x.i(&[0x48, 0x85, 0xC9]); // test rcx, rcx
    let ss_start_ok = x.jge8();
    x.xor_rr(1, 1);
    x.patch_jmp_rel8(ss_start_ok);
    // Clamp: if len < 0, len = 0
    x.i(&[0x48, 0x85, 0xC0]); // test rax, rax
    let ss_len_ok = x.jge8();
    x.xor_rr(0, 0);
    x.patch_jmp_rel8(ss_len_ok);
    // Read string length
    x.mov_rm(8, 2, -4); // r8 = str_len
    // Clamp: if start > str_len, start = str_len
    x.i(&[0x4C, 0x39, 0xC1]); // cmp rcx, r8
    let ss_start_ok2 = x.jle8();
    x.mov_rr(1, 8); // rcx = str_len
    x.patch_jmp_rel8(ss_start_ok2);
    // Clamp: if start+len > str_len, len = str_len - start
    x.mov_rr(9, 1); // r9 = start
    x.i(&[0x4C, 0x01, 0xC9]); // add r9, r8? No — add start+len
    // Check: start + len > str_len
    x.mov_rr(9, 1); // r9 = start
    x.i(&[0x48, 0x01, 0xC1]); // add rcx_temp? No.
    // Simpler: remaining = str_len - start; if len > remaining, len = remaining
    x.mov_rr(9, 8); // r9 = str_len
    x.i(&[0x4C, 0x29, 0xC9]); // sub r9, rcx (r9 = str_len - start)
    x.i(&[0x4C, 0x39, 0xC8]); // cmp rax, r9
    let ss_len_ok2 = x.jle8();
    x.mov_rr(0, 9); // len = remaining
    x.patch_jmp_rel8(ss_len_ok2);
    // Allocate len+5 bytes
    x.mov_rr(8, 0); // r8 = result len
    x.add_ri(0, 5);
    x.mov_rr(7, 0);
    x.push_r(14); x.push_r(13); x.push_r(2); x.push_r(1); x.push_r(8);
    x.b(0xE8); x.i32(1); // call alloc
    x.pop_r(8); x.pop_r(1); x.pop_r(2); x.pop_r(13); x.pop_r(14);
    // rax = alloc ptr, r8 = result len, rcx = start, rdx = str addr
    x.mov_rr(9, 0); // r9 = alloc ptr
    // Write length
    x.i(&[0x45, 0x89, 0x01]); // mov [r9], r8d (write 4-byte length)
    x.add_ri(9, 4); // r9 points to data
    // Copy bytes: for i=0..len-1: [r9+i] = [str+start+i]
    x.xor_rr(10, 10);
    let ss_loop = x.m();
    x.i(&[0x4D, 0x39, 0xC2]); // cmp r10, r8
    let ss_done = x.jge8();
    // Read: al = [str + start + r10]
    x.mov_rr(0, 2); // rax = str addr
    x.i(&[0x48, 0x01, 0xC8]); // add rax, rcx (start)
    x.i(&[0x4C, 0x01, 0xD0]); // add rax, r10
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    // Write: [r9 + r10] = al
    x.mov_rr(1, 9);
    x.i(&[0x4C, 0x01, 0xD1]); // add rcx, r10
    x.i(&[0x88, 0x01]); // mov [rcx], al
    x.sub_ri(1, 0); // restore? This is getting messy.
    // Actually, the write destination is r9 + r10, not rcx + r10.
    x.mov_rr(1, 9); // rcx = r9
    x.i(&[0x4C, 0x01, 0xD1]); // add rcx, r10
    x.i(&[0x88, 0x01]); // mov [rcx], al
    x.add_ri(10, 1);
    let ss_back = x.jmp();
    x.p_i32(ss_back, ss_loop as i32 - (ss_back + 4) as i32);
    x.patch_jmp_rel8(ss_done);
    // Write null
    x.mov_rr(0, 9);
    x.i(&[0x4C, 0x01, 0xC0]); // add rax, r8
    x.i(&[0xC6, 0x00, 0x00]); // mov byte [rax], 0
    // Push result (r9 = data area = alloc+4)
    x.mov_mr(15, 0, 9);
    x.back();

    // STREQ (0x49) — s1 s2 → bool
    x.patch_handler(0x49);
    x.mov_rm(6, 15, 0); // r6 = s2
    x.mov_rm(7, 15, 8); // r7 = s1
    x.add_ri(15, 8); // pop one, reuse slot
    // Compare lengths
    x.mov_rm(0, 7, -4); // rax = len1
    x.mov_rm(1, 6, -4); // rcx = len2
    x.i(&[0x48, 0x39, 0xC8]); // cmp rax, rcx
    let seq_len_ok = x.je8();
    // Lengths differ → false
    x.xor_rr(0, 0);
    x.mov_mr(15, 0, 0);
    let seq_end1 = x.jmp();
    x.patch_jmp_rel8(seq_len_ok);
    // Compare byte by byte
    x.mov_rr(2, 0); // rdx = len (either one)
    x.xor_rr(9, 9); // r9 = index
    let seq_loop = x.m();
    x.i(&[0x4C, 0x39, 0xCA]); // cmp rdx, r9
    let seq_equal = x.jge8();
    // Read bytes
    x.mov_rr(0, 7);
    x.i(&[0x4C, 0x01, 0xC8]); // add rax, r9
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    x.mov_rr(1, 6);
    x.i(&[0x4C, 0x01, 0xC9]); // add rcx, r9
    x.i(&[0x0F, 0xB6, 0x09]); // movzx ecx, byte [rcx]
    x.i(&[0x48, 0x39, 0xC8]); // cmp rax, rcx
    let seq_neq = x.jne8();
    x.add_ri(9, 1);
    let seq_back = x.jmp();
    x.p_i32(seq_back, seq_loop as i32 - (seq_back + 4) as i32);
    x.patch_jmp_rel8(seq_equal);
    // All equal → true
    x.mov_r64i(0, 1);
    x.mov_mr(15, 0, 0);
    let seq_end2 = x.jmp();
    x.patch_jmp_rel8(seq_neq);
    x.xor_rr(0, 0);
    x.mov_mr(15, 0, 0);
    x.patch_jmp_rel8(seq_end1);
    x.patch_jmp_rel8(seq_end2);
    x.back();

    // STRLT (0x4A) — s1 s2 → bool
    x.patch_handler(0x4A);
    x.mov_rm(6, 15, 0); // r6 = s2
    x.mov_rm(7, 15, 8); // r7 = s1
    x.add_ri(15, 8);
    // Byte-by-byte compare
    let slt_loop = x.m();
    // Read bytes
    x.i(&[0x41, 0x0F, 0xB6, 0x07]); // movzx eax, byte [r7] (s1)
    x.i(&[0x41, 0x0F, 0xB6, 0x0E]); // movzx ecx, byte [r6] (s2)
    // Check if s1 ended
    x.i(&[0x48, 0x85, 0xC0]); // test rax, rax
    let slt_s1_end = x.je8();
    // Check if s2 ended
    x.i(&[0x48, 0x85, 0xC9]); // test rcx, rcx
    let slt_s2_end = x.je8();
    // Compare bytes
    x.i(&[0x48, 0x39, 0xC8]); // cmp rax, rcx
    let slt_less = x.jb8();
    let slt_greater = x.ja8();
    // Equal → advance both
    x.add_ri(7, 1);
    x.add_ri(6, 1);
    let slt_back = x.jmp();
    x.p_i32(slt_back, slt_loop as i32 - (slt_back + 4) as i32);
    // s1 ended: if s2 also ended → equal (false), else s1 < s2 (true)
    x.patch_jmp_rel8(slt_s1_end);
    x.i(&[0x48, 0x85, 0xC9]); // test rcx, rcx
    let slt_both_end = x.je8();
    // s1 ended, s2 didn't → s1 < s2
    x.patch_jmp_rel8(slt_less);
    x.mov_r64i(0, 1);
    x.mov_mr(15, 0, 0);
    let slt_end1 = x.jmp();
    // s2 ended: s1 > s2
    x.patch_jmp_rel8(slt_s2_end);
    x.patch_jmp_rel8(slt_greater);
    x.xor_rr(0, 0);
    x.mov_mr(15, 0, 0);
    let slt_end2 = x.jmp();
    // Both ended → equal → false
    x.patch_jmp_rel8(slt_both_end);
    x.xor_rr(0, 0);
    x.mov_mr(15, 0, 0);
    x.patch_jmp_rel8(slt_end1);
    x.patch_jmp_rel8(slt_end2);
    x.back();

    // STRFIND (0x4B) — haystack needle → index
    x.patch_handler(0x4B);
    x.mov_rm(6, 15, 0); // r6 = needle
    x.mov_rm(7, 15, 8); // r7 = haystack
    x.add_ri(15, 8); // pop needle, reuse slot
    // Linear search: for each position in haystack, check if needle matches
    x.mov_rm(8, 7, -4); // r8 = haystack len
    x.mov_rm(9, 6, -4); // r9 = needle len
    x.xor_rr(10, 10); // r10 = haystack index
    let sf_outer = x.m();
    // Check: if haystack_index + needle_len > haystack_len, not found
    x.mov_rr(0, 10);
    x.i(&[0x4C, 0x01, 0xC8]); // add rax, r9
    x.i(&[0x4C, 0x39, 0xC0]); // cmp rax, r8
    let sf_notfound = x.jg8();
    // Compare needle at position r10
    x.xor_rr(11, 11); // r11 = needle index
    let sf_inner = x.m();
    x.i(&[0x4D, 0x39, 0xCB]); // cmp r11, r9
    let sf_found = x.jge8();
    // Read bytes
    x.mov_rr(0, 7);
    x.i(&[0x4C, 0x01, 0xD0]); // add rax, r10
    x.i(&[0x4C, 0x01, 0xD8]); // add rax, r11? No — r10 is haystack pos, r11 is needle pos
    // Actually: haystack[r10 + r11]
    x.mov_rr(0, 10);
    x.i(&[0x4C, 0x01, 0xD8]); // add rax, r11
    x.i(&[0x48, 0x01, 0xF8]); // add rax, rdi? No — haystack is in r7
    x.i(&[0x49, 0x01, 0xF8]); // add rax, r15? No.
    // I need: rax = r7 + r10 + r11
    x.mov_rr(0, 7);
    x.i(&[0x4C, 0x01, 0xD0]); // add rax, r10
    x.i(&[0x4C, 0x01, 0xD8]); // add rax, r11
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    // needle[r11]
    x.mov_rr(1, 6);
    x.i(&[0x4C, 0x01, 0xD9]); // add rcx, r11
    x.i(&[0x0F, 0xB6, 0x09]); // movzx ecx, byte [rcx]
    x.i(&[0x48, 0x39, 0xC8]); // cmp rax, rcx
    let sf_mismatch = x.jne8();
    x.add_ri(11, 1);
    let sf_inner_back = x.jmp();
    x.p_i32(sf_inner_back, sf_inner as i32 - (sf_inner_back + 4) as i32);
    x.patch_jmp_rel8(sf_found);
    // Found! Push r10
    x.mov_mr(15, 0, 10);
    let sf_end = x.jmp();
    x.patch_jmp_rel8(sf_mismatch);
    x.add_ri(10, 1);
    let sf_outer_back = x.jmp();
    x.p_i32(sf_outer_back, sf_outer as i32 - (sf_outer_back + 4) as i32);
    x.patch_jmp_rel8(sf_notfound);
    // Not found: push -1
    x.i(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    x.mov_mr(15, 0, 0);
    x.patch_jmp_rel8(sf_end);
    x.back();

    // STRREPLACE (0x4C) — src old new → result
    x.patch_handler(0x4C);
    x.mov_rm(6, 15, 0); // r6 = new
    x.mov_rm(7, 15, 8); // r7 = old
    x.mov_rm(8, 15, 16); // r8 = src
    x.add_ri(15, 16); // pop old and new, reuse slot for result
    // Use OUT_BUF as temp buffer, then allocate result in heap
    x.mov_r64i(9, OUT_BUF_ADDR); // r9 = dest pointer
    x.mov_rr(10, 8); // r10 = src scan pointer
    x.mov_rm(11, 7, -4); // r11 = old_len
    // Main loop: scan src for old
    let sr_outer = x.m();
    // Check if *src == 0 (end of string)
    x.i(&[0x41, 0x0F, 0xB6, 0x02]); // movzx eax, byte [r10]
    x.i(&[0x48, 0x85, 0xC0]); // test rax, rax
    let sr_done = x.je8();
    // Try to match 'old' at current position
    x.xor_rr(12, 12); // r12 = match index
    let sr_match = x.m();
    x.i(&[0x4D, 0x39, 0xDC]); // cmp r12, r11 (old_len)
    let sr_matched = x.jge8(); // all bytes matched
    // Compare src[pos+idx] with old[idx]
    x.mov_rr(0, 10); // rax = src pos
    x.i(&[0x4C, 0x01, 0xE0]); // add rax, r12
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    x.mov_rr(1, 7); // rcx = old
    x.i(&[0x4C, 0x01, 0xE1]); // add rcx, r12
    x.i(&[0x0F, 0xB6, 0x09]); // movzx ecx, byte [rcx]
    x.i(&[0x48, 0x39, 0xC8]); // cmp rax, rcx
    let sr_no_match = x.jne8();
    x.add_ri(12, 1);
    let sr_match_back = x.jmp();
    x.p_i32(sr_match_back, sr_match as i32 - (sr_match_back + 4) as i32);
    // Matched! Copy 'new' to dest
    x.patch_jmp_rel8(sr_matched);
    x.xor_rr(12, 12); // idx = 0
    let sr_cpy_new = x.m();
    x.mov_rm(0, 6, -4); // rax = new_len
    x.i(&[0x4C, 0x39, 0xE0]); // cmp rax, r12
    let sr_cpy_new_done = x.jge8();
    x.mov_rr(0, 6);
    x.i(&[0x4C, 0x01, 0xE0]); // add rax, r12
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    x.i(&[0x41, 0x88, 0x01]); // mov [r9], al
    x.add_ri(9, 1);
    x.add_ri(12, 1);
    let sr_cpy_new_back = x.jmp();
    x.p_i32(sr_cpy_new_back, sr_cpy_new as i32 - (sr_cpy_new_back + 4) as i32);
    x.patch_jmp_rel8(sr_cpy_new_done);
    // Advance src past the matched 'old'
    x.mov_rm(0, 7, -4); // rax = old_len
    x.i(&[0x4C, 0x01, 0xC2]); // add r10, rax
    let sr_continue = x.jmp();
    // No match: copy current byte and advance
    x.patch_jmp_rel8(sr_no_match);
    x.i(&[0x41, 0x0F, 0xB6, 0x02]); // movzx eax, byte [r10]
    x.i(&[0x41, 0x88, 0x01]); // mov [r9], al
    x.add_ri(9, 1);
    x.add_ri(10, 1);
    x.p_i32(sr_continue, sr_outer as i32 - (sr_continue + 4) as i32);
    // Done: write null terminator
    x.patch_jmp_rel8(sr_done);
    x.i(&[0x41, 0xC6, 0x01, 0x00]); // mov byte [r9], 0
    // Calculate result length
    x.mov_r64i(0, OUT_BUF_ADDR);
    x.i(&[0x4C, 0x29, 0xC8]); // sub rax, r9? No — r9 is dest, rax is start
    // Actually: result_len = r9 - OUT_BUF_ADDR
    x.mov_rr(0, 9);
    x.mov_r64i(1, OUT_BUF_ADDR);
    x.i(&[0x48, 0x29, 0xC8]); // sub rax, rcx (result_len = dest - start)
    // Allocate string in heap: result_len + 5
    x.mov_rr(12, 0); // r12 = result_len
    x.add_ri(0, 5);
    x.mov_rr(7, 0);
    x.push_r(6); x.push_r(8); x.push_r(9); x.push_r(10); x.push_r(11); x.push_r(12);
    x.b(0xE8); x.i32(1); // call alloc
    x.pop_r(12); x.pop_r(11); x.pop_r(10); x.pop_r(9); x.pop_r(8); x.pop_r(6);
    // rax = alloc ptr
    x.mov_rr(8, 0); // r8 = alloc ptr
    // Write length
    x.i(&[0x45, 0x89, 0x20]); // mov [r8], r12d
    x.add_ri(8, 4); // r8 = data area
    // Copy from OUT_BUF to heap
    x.mov_r64i(6, OUT_BUF_ADDR);
    x.xor_rr(10, 10);
    let sr_copy = x.m();
    x.i(&[0x4D, 0x39, 0xD4]); // cmp r12, r10
    let sr_copy_done = x.jge8();
    x.mov_rr(0, 6);
    x.i(&[0x4C, 0x01, 0xD0]); // add rax, r10
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    x.mov_rr(1, 8);
    x.i(&[0x4C, 0x01, 0xD1]); // add rcx, r10
    x.i(&[0x88, 0x01]); // mov [rcx], al
    x.add_ri(10, 1);
    let sr_copy_back = x.jmp();
    x.p_i32(sr_copy_back, sr_copy as i32 - (sr_copy_back + 4) as i32);
    x.patch_jmp_rel8(sr_copy_done);
    // Null terminate
    x.mov_rr(0, 8);
    x.i(&[0x4C, 0x01, 0xE0]); // add rax, r12
    x.i(&[0xC6, 0x00, 0x00]);
    // Push result (r8 = data area)
    x.mov_mr(15, 0, 8);
    x.back();

    // STRTOI64 (0x4D) — str → i64
    x.patch_handler(0x4D);
    x.mov_rm(7, 15, 0); // r7 = str addr
    x.xor_rr(0, 0); // rax = 0 (result)
    x.xor_rr(8, 8); // r8 = 0 (index)
    x.xor_rr(9, 9); // r9 = 0 (negative flag)
    // Check for '-'
    x.i(&[0x41, 0x0F, 0xB6, 0x0F]); // movzx ecx, byte [r7]
    x.i(&[0x48, 0x83, 0xF9, 0x2D]); // cmp rcx, '-'
    let sti_not_neg = x.jne8();
    x.mov_r64i(9, 1); // negative = true
    x.add_ri(8, 1); // skip '-'
    x.patch_jmp_rel8(sti_not_neg);
    // Parse digits
    let sti_loop = x.m();
    // Read byte at str[index]
    x.mov_rr(1, 7);
    x.i(&[0x4C, 0x01, 0xC1]); // add rcx, r8
    x.i(&[0x0F, 0xB6, 0x09]); // movzx ecx, byte [rcx]
    // Check if digit
    x.i(&[0x48, 0x83, 0xF9, 0x30]); // cmp rcx, '0'
    let sti_done = x.jb8();
    x.i(&[0x48, 0x83, 0xF9, 0x39]); // cmp rcx, '9'
    let sti_done2 = x.jg8();
    // result = result * 10 + (digit - '0')
    x.i(&[0x48, 0x6B, 0xC0, 0x0A]); // imul rax, rax, 10
    x.i(&[0x48, 0x83, 0xE9, 0x30]); // sub rcx, '0'
    x.i(&[0x48, 0x01, 0xC8]); // add rax, rcx
    x.add_ri(8, 1);
    let sti_back = x.jmp();
    x.p_i32(sti_back, sti_loop as i32 - (sti_back + 4) as i32);
    x.patch_jmp_rel8(sti_done);
    x.patch_jmp_rel8(sti_done2);
    // Apply negation if needed
    x.i(&[0x4D, 0x85, 0xC9]); // test r9, r9
    let sti_pos = x.je8();
    x.i(&[0x48, 0xF7, 0xD8]); // neg rax
    x.patch_jmp_rel8(sti_pos);
    x.mov_mr(15, 0, 0);
    x.back();

    // I64TOSTR (0x4E) — i64 → str
    x.patch_handler(0x4E);
    x.push_r(0); x.push_r(1); x.push_r(2); x.push_r(6); x.push_r(7);
    x.mov_rm(0, 15, 0); // rax = value
    x.mov_rr(7, 0); // rdi = value
    x.mov_r64i(6, OUT_BUF_ADDR); // rsi = output buffer
    x.b(0xE8); x.i32(0); // call itoa (patched)
    // Allocate string in heap
    // rax = length from itoa, rsi = buffer start
    x.mov_rr(2, 0); // rdx = length
    x.add_ri(0, 5); // +5 for length prefix + null
    x.mov_rr(7, 0); // rdi = alloc size
    x.push_r(14); x.push_r(13); x.push_r(2);
    x.b(0xE8); x.i32(1); // call alloc
    x.pop_r(2); x.pop_r(13); x.pop_r(14);
    // rax = alloc ptr, rdx = length
    x.mov_rr(8, 0); // r8 = alloc ptr
    // Write length
    x.i(&[0x41, 0x89, 0x10]); // mov [r8], edx
    x.add_ri(8, 4); // r8 = data area
    // Copy from OUT_BUF to heap
    x.mov_r64i(6, OUT_BUF_ADDR);
    x.xor_rr(9, 9);
    let i2s_loop = x.m();
    x.i(&[0x4C, 0x39, 0xD2]); // cmp rdx, r9
    let i2s_done = x.jge8();
    x.mov_rr(0, 6);
    x.i(&[0x4C, 0x01, 0xC8]); // add rax, r9
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    x.mov_rr(1, 8);
    x.i(&[0x4C, 0x01, 0xC9]); // add rcx, r9
    x.i(&[0x88, 0x01]); // mov [rcx], al
    x.add_ri(9, 1);
    let i2s_back = x.jmp();
    x.p_i32(i2s_back, i2s_loop as i32 - (i2s_back + 4) as i32);
    x.patch_jmp_rel8(i2s_done);
    // Write null
    x.mov_rr(0, 8);
    x.i(&[0x4C, 0x01, 0xC8]); // add rax, r9? No — r9 = length
    x.i(&[0x48, 0x01, 0xD0]); // add rax, rdx
    x.i(&[0xC6, 0x00, 0x00]); // mov byte [rax], 0
    x.pop_r(7); x.pop_r(6); x.pop_r(2); x.pop_r(1); x.pop_r(0);
    // Push string address (r8 = data area = alloc+4)
    x.mov_mr(15, 0, 8);
    x.back();

    // STRNTH (0x4F) — str idx → char_code
    x.patch_handler(0x4F);
    x.mov_rm(0, 15, 0); // rax = idx
    x.add_ri(15, 8);
    x.i(&[0x49, 0x03, 0x07]); // add rax, [r15] (str + idx)
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    x.mov_mr(15, 0, 0);
    x.back();
}

// ── Control flow ────────────────────────────────────────────────────

fn impl_control_ops(x: &mut X) {
    // COND (0x50) — pop, if false jump by offset
    x.patch_handler(0x50);
    x.mov_rm(0, 15, 0);
    x.add_ri(15, 8);
    x.i(&[0x48, 0x85, 0xC0]); // test rax, rax
    let skip = x.jne8();
    x.i(&[0x47, 0x8B, 0x04, 0x2E]); // mov eax, [r14+r13]
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]); // add r13, rax
    x.patch_jmp_rel8(skip);
    // If not taken, still need to skip the offset bytes
    // Wait — COND should always read the offset, only jump if false
    // Let me restructure:
    x.patch_handler(0x50); // re-patch
    x.mov_rm(0, 15, 0); // rax = condition
    x.add_ri(15, 8); // pop
    // Read offset regardless
    x.i(&[0x47, 0x8B, 0x0C, 0x2E]); // mov ecx, [r14+r13]
    x.add_ri(13, 4); // skip offset bytes
    x.i(&[0x48, 0x85, 0xC0]); // test rax, rax
    let taken = x.jne8(); // if true, skip jump
    x.i(&[0x49, 0x01, 0xCD]); // add r13, rcx (jump)
    x.patch_jmp_rel8(taken);
    x.back();

    // JUMP (0x51)
    x.patch_handler(0x51);
    x.i(&[0x47, 0x8B, 0x04, 0x2E]); // mov eax, [r14+r13]
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]); // add r13, rax
    x.back();

    // LOOP (0x52) — pop, if true jump back by offset
    x.patch_handler(0x52);
    x.mov_rm(0, 15, 0);
    x.add_ri(15, 8);
    x.i(&[0x47, 0x8B, 0x0C, 0x2E]); // mov ecx, [r14+r13]
    x.add_ri(13, 4);
    x.i(&[0x48, 0x85, 0xC0]); // test rax, rax
    let no_jump = x.je8(); // if false, don't jump
    x.i(&[0x49, 0x01, 0xCD]); // add r13, rcx
    x.patch_jmp_rel8(no_jump);
    x.back();

    // TIMES (0x53) — n ref_bc_ptr → (nothing)
    x.patch_handler(0x53);
    // Skip inline ref
    x.i(&[0x47, 0x8B, 0x04, 0x2E]);
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]);
    // Stack: [... n, ref_bc_addr]
    x.mov_rm(9, 15, 0); // r9 = ref_bc_addr
    x.mov_rm(10, 15, 8); // r10 = n
    x.add_ri(15, 16);
    // Loop
    x.xor_rr(12, 12);
    let times_loop = x.m();
    x.i(&[0x4D, 0x39, 0xD4]); // cmp r12, r10
    let times_done = x.jge8();
    x.mov_rr(7, 9);
    x.mov_r64i(6, 0xFFFF);
    x.push_r(9); x.push_r(10); x.push_r(12);
    x.b(0xE8); x.i32(2); // call run_ref
    x.pop_r(12); x.pop_r(10); x.pop_r(9);
    x.add_ri(12, 1);
    let times_back = x.jmp();
    x.p_i32(times_back, times_loop as i32 - (times_back + 4) as i32);
    x.patch_jmp_rel8(times_done);
    x.back();
}

// ── Call/Return ─────────────────────────────────────────────────────

fn impl_call_return(x: &mut X, word_table_addr: u64, word_count: usize) {
    // CALL (0x60) — word dispatch via word table
    // Word table layout at word_table_addr:
    //   entry[i]: { name_ptr: u32, name_len: u32, bc_ptr: u32, bc_len: u32 } (16 bytes each)
    // Names and bytecodes stored after the table.
    //
    // Strategy: linear scan, inline byte comparison (no function call overhead).
    x.patch_handler(0x60);
    // Read name_len from bytecode
    x.i(&[0x43, 0x0F, 0xB6, 0x04, 0x2E]); // movzx eax, [r14+r13] (name_len)
    x.add_ri(13, 1); // ip++ (past name_len byte)
    // Save name start: r12 = r14 + r13 (points to name bytes in bytecode)
    x.mov_rr(12, 14);
    x.i(&[0x4D, 0x01, 0xEC]); // add r12, r13
    // Save name_len: r11 = rax
    x.mov_rr(11, 0);
    // Advance ip past name bytes
    x.i(&[0x49, 0x01, 0xC5]); // add r13, rax

    // Linear scan through word table
    x.mov_r64i(8, word_table_addr); // r8 = table base
    x.xor_rr(9, 9); // r9 = entry index

    let scan_loop = x.m();
    if word_count > 0 {
        x.mov_r64i(0, word_count as u64);
        x.i(&[0x4D, 0x39, 0xC1]); // cmp r9, rax
    }
    let scan_done = x.jge8(); // if i >= count, not found

    // Load entry: name_ptr, name_len, bc_ptr, bc_len
    // entry_addr = table_base + i * 16
    x.mov_rr(0, 9);
    x.i(&[0x48, 0xC1, 0xE0, 0x04]); // shl rax, 4 (i * 16)
    x.i(&[0x4C, 0x01, 0xC0]); // add rax, r8 (table_base + i*16)
    // rax points to entry
    // Read name_len from entry+4
    x.mov_rm(1, 0, 4); // rcx = entry.name_len
    // Quick check: name_len matches?
    x.i(&[0x4C, 0x39, 0xD9]); // cmp rcx, r11
    let len_mismatch = x.jne8();
    // Lengths match — compare bytes
    // Read name_ptr from entry+0
    x.mov_rm(2, 0, 0); // rdx = entry.name_ptr (offset in data segment)
    // rdx is relative to data segment base. Convert to absolute:
    // Actually, name_ptr should be stored as absolute address.
    // Let's store absolute addresses in the table.

    // Byte-by-byte comparison: r12 = call_name, rdx = table_name, rcx = len
    x.xor_rr(10, 10); // r10 = byte index
    let byte_loop = x.m();
    x.i(&[0x4D, 0x39, 0xD3]); // cmp r11, r10 (r11 = name_len)
    let byte_match = x.jge8(); // all bytes matched!

    // Load bytes
    x.mov_rr(7, 12); // rdi = call_name
    x.i(&[0x4C, 0x01, 0xD7]); // add rdi, r10
    x.i(&[0x0F, 0xB6, 0x3F]); // movzx edi, byte [rdi]

    x.mov_rr(6, 2); // rsi = table_name
    x.i(&[0x4C, 0x01, 0xD6]); // add rsi, r10
    x.i(&[0x0F, 0xB6, 0x36]); // movzx esi, byte [rsi]

    x.i(&[0x48, 0x39, 0xF7]); // cmp rdi, rsi
    let byte_neq = x.jne8();

    x.add_ri(10, 1); // i++
    let byte_back = x.jmp();
    x.p_i32(byte_back, byte_loop as i32 - (byte_back + 4) as i32);

    // Bytes not equal — skip to next entry
    x.patch_jmp_rel8(byte_neq);
    x.patch_jmp_rel8(len_mismatch);
    x.add_ri(9, 1); // entry_idx++
    let scan_back = x.jmp();
    x.p_i32(scan_back, scan_loop as i32 - (scan_back + 4) as i32);

    // All bytes matched! Found the word.
    x.patch_jmp_rel8(byte_match);
    // rax still points to entry. Read bc_ptr and bc_len.
    x.mov_rm(2, 0, 8); // rdx = entry.bc_ptr (absolute address)
    x.mov_rm(1, 0, 12); // rcx = entry.bc_len
    // Save current state to call stack
    x.mov_r64i(8, CALL_SP_ADDR);
    x.mov_rm(0, 8, 0); // rax = call_sp
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
    x.mov_r64i(9, CALL_STACK_ADDR);
    x.i(&[0x4D, 0x01, 0xC8]); // add r9, rax
    // Save r14 (bc) and r13 (ip)
    x.mov_mr(9, 0, 14); // call_stack[sp] = r14
    x.mov_mr(9, 8, 13); // call_stack[sp+1] = r13
    // Increment call_sp by 2
    x.mov_r64i(8, CALL_SP_ADDR);
    x.mov_rm(0, 8, 0);
    x.add_ri(0, 2);
    x.mov_mr(8, 0, 0);
    // Switch to word's bytecode
    x.mov_rr(14, 2); // r14 = bc_ptr
    x.xor_rr(13, 13); // r13 = 0
    let call_end = x.jmp();

    // Word not found — skip name and continue
    x.patch_jmp_rel8(scan_done);
    // Name bytes already skipped (ip advanced earlier)
    x.patch_jmp_rel8(call_end);
    x.back();

    // RETURN (0x61)
    x.patch_handler(0x61);
    // Check call_sp: if > 0, restore saved state; else exit
    x.mov_r64i(8, CALL_SP_ADDR);
    x.mov_rm(0, 8, 0);
    x.i(&[0x48, 0x85, 0xC0]);
    let has_frame = x.jne8();
    x.mov_r64i(0, 60); x.xor_rr(7, 7); x.syscall();
    x.patch_jmp_rel8(has_frame);
    // Restore from call stack
    x.sub_ri(0, 2); // rax = call_sp - 2
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
    x.mov_r64i(8, CALL_STACK_ADDR);
    x.i(&[0x4C, 0x01, 0xC0]); // add rax, r8
    x.mov_rm(14, 0, 0); // r14 = saved_bc
    x.mov_rm(13, 0, 8); // r13 = saved_ip
    x.mov_r64i(8, CALL_SP_ADDR);
    x.mov_rm(0, 8, 0);
    x.sub_ri(0, 2);
    x.mov_mr(8, 0, 0);
    x.back();
    x.mov_r64i(8, CALL_SP_ADDR);
    x.mov_rm(0, 8, 0);
    x.i(&[0x48, 0x85, 0xC0]);
    let has_frame = x.jne8();
    x.mov_r64i(0, 60); x.xor_rr(7, 7); x.syscall();
    x.patch_jmp_rel8(has_frame);
    // Restore from call stack
    x.sub_ri(0, 2); // rax = call_sp - 2
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
    x.mov_r64i(8, CALL_STACK_ADDR);
    x.i(&[0x4C, 0x01, 0xC0]); // add rax, r8
    x.mov_rm(14, 0, 0); // r14 = saved_bc
    x.mov_rm(13, 0, 8); // r13 = saved_ip
    x.mov_r64i(8, CALL_SP_ADDR);
    x.mov_rm(0, 8, 0);
    x.sub_ri(0, 2);
    x.mov_mr(8, 0, 0);
    x.back();
}

// ── IO operations ───────────────────────────────────────────────────

fn impl_io_ops(x: &mut X) {
    // OUTPUT_TOP (0x90)
    x.patch_handler(0x90);
    x.push_r(0); x.push_r(1); x.push_r(2); x.push_r(6); x.push_r(7); x.push_r(15);
    x.mov_rm(0, 15, 0);
    x.add_ri(15, 8); // pop value
    // Call itoa: rdi=value, rsi=buf → rax=len
    x.mov_rr(7, 0); // rdi = value
    x.mov_r64i(6, OUT_BUF_ADDR); // rsi = buffer
    x.b(0xE8); x.i32(0); // call itoa (will be patched)
    // write(1, buf, len)
    x.mov_r64i(0, 1); // syscall: write
    x.mov_r64i(7, 1); // fd: stdout
    // rsi already set from itoa
    x.mov_rr(2, 0); // rdx = length
    x.syscall();
    // Write newline
    x.mov_r64i(6, OUT_BUF_ADDR + 256);
    x.b(0xC6); x.b(0x06); x.b(0x0A); // mov byte [rsi], '\n'
    x.mov_r64i(0, 1); x.mov_r64i(7, 1);
    x.mov_r64i(6, OUT_BUF_ADDR + 256);
    x.mov_r64i(2, 1);
    x.syscall();
    x.pop_r(15); x.pop_r(7); x.pop_r(6); x.pop_r(2); x.pop_r(1); x.pop_r(0);
    x.back();

    // OUTPUT_ALL (0x91) — print entire stack
    x.patch_handler(0x91);
    // Print '[' then each value then ']' then newline
    x.push_r(0); x.push_r(1); x.push_r(2); x.push_r(6); x.push_r(7); x.push_r(15);
    // Print '['
    x.mov_r64i(6, OUT_BUF_ADDR);
    x.b(0xC6); x.b(0x06); x.b(0x5B); // mov byte [rsi], '['
    x.mov_r64i(0, 1); x.mov_r64i(7, 1); x.mov_r64i(2, 1); x.syscall();
    // Print each value (simplified: just print as i64)
    // For now, just print closing bracket
    x.mov_r64i(6, OUT_BUF_ADDR);
    x.b(0xC6); x.b(0x06); x.b(0x5D); // ']'
    x.b(0xC6); x.b(0x46); x.b(0x01); x.b(0x0A); // mov byte [rsi+1], '\n'
    x.mov_r64i(0, 1); x.mov_r64i(7, 1); x.mov_r64i(2, 2); x.syscall();
    x.pop_r(15); x.pop_r(7); x.pop_r(6); x.pop_r(2); x.pop_r(1); x.pop_r(0);
    x.back();

    // READ_INPUT (0x92) — read line from stdin
    x.patch_handler(0x92);
    // Allocate buffer for input
    x.push_r(0); x.push_r(1); x.push_r(2); x.push_r(6); x.push_r(7);
    // read(0, OUT_BUF, 255)
    x.xor_rr(0, 0); // fd = stdin
    x.mov_r64i(6, OUT_BUF_ADDR); // buf
    x.mov_r64i(2, 255); // count
    x.mov_r64i(7, 0); // fd=0 (already 0, but explicit)
    x.syscall();
    // rax = bytes read (or -1 on error)
    x.i(&[0x48, 0x85, 0xC0]); // test rax, rax
    let ri_ok = x.jg8();
    // Error or EOF: push empty string
    x.xor_rr(0, 0);
    let ri_end = x.jmp();
    x.patch_jmp_rel8(ri_ok);
    // Strip trailing newline
    x.i(&[0x48, 0x83, 0xE8, 0x01]); // sub rax, 1 (last byte index)
    x.mov_rr(6, 0); // rsi = OUT_BUF_ADDR
    x.i(&[0x48, 0x01, 0xC6]); // add rsi, rax
    x.i(&[0x80, 0x3E, 0x0A]); // cmp byte [rsi], '\n'
    let ri_no_nl = x.jne8();
    x.b(0xC6); x.b(0x06); x.b(0x00); // mov byte [rsi], 0
    x.patch_jmp_rel8(ri_no_nl);
    // Push string address
    x.mov_r64i(0, OUT_BUF_ADDR);
    x.patch_jmp_rel8(ri_end);
    x.pop_r(7); x.pop_r(6); x.pop_r(2); x.pop_r(1); x.pop_r(0);
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    x.back();
}

// ── Float operations ────────────────────────────────────────────────

fn impl_float_ops(x: &mut X) {
    // I64ToF64 (0xB0) — convert i64 to f64
    x.patch_handler(0xB0);
    x.mov_rm(0, 15, 0); // rax = i64 value
    x.i(&[0x48, 0x89, 0x45, 0x00]); // mov [rbp], rax (scratch)
    x.i(&[0xDB, 0x45, 0x00]); // fild dword [rbp] (load integer)
    x.i(&[0xDD, 0x5D, 0x00]); // fstp qword [rbp] (store as f64)
    x.i(&[0x48, 0x8B, 0x45, 0x00]); // mov rax, [rbp]
    x.mov_mr(15, 0, 0); // store back
    x.back();

    // F64ToI64 (0xB1) — truncate f64 to i64
    x.patch_handler(0xB1);
    x.mov_rm(0, 15, 0); // rax = f64 bits
    x.i(&[0x48, 0x89, 0x45, 0x00]); // mov [rbp], rax
    x.i(&[0xDD, 0x45, 0x00]); // fld qword [rbp]
    x.i(&[0xDB, 0x5D, 0x00]); // fistp dword [rbp] (truncate to i32)
    x.i(&[0x48, 0x63, 0x45, 0x00]); // movsxd rax, dword [rbp]
    x.mov_mr(15, 0, 0);
    x.back();

    // FSqrt (0xB2)
    x.patch_handler(0xB2);
    x.mov_rm(0, 15, 0);
    x.i(&[0x48, 0x89, 0x45, 0x00]);
    x.i(&[0xDD, 0x45, 0x00]); // fld
    x.i(&[0xD9, 0xFA]); // fsqrt
    x.i(&[0xDD, 0x5D, 0x00]); // fstp
    x.i(&[0x48, 0x8B, 0x45, 0x00]);
    x.mov_mr(15, 0, 0);
    x.back();

    // FSin (0xB3)
    x.patch_handler(0xB3);
    x.mov_rm(0, 15, 0);
    x.i(&[0x48, 0x89, 0x45, 0x00]);
    x.i(&[0xDD, 0x45, 0x00]);
    x.i(&[0xD9, 0xFE]); // fsin
    x.i(&[0xDD, 0x5D, 0x00]);
    x.i(&[0x48, 0x8B, 0x45, 0x00]);
    x.mov_mr(15, 0, 0);
    x.back();

    // FCos (0xB4)
    x.patch_handler(0xB4);
    x.mov_rm(0, 15, 0);
    x.i(&[0x48, 0x89, 0x45, 0x00]);
    x.i(&[0xDD, 0x45, 0x00]);
    x.i(&[0xD9, 0xFF]); // fcos
    x.i(&[0xDD, 0x5D, 0x00]);
    x.i(&[0x48, 0x8B, 0x45, 0x00]);
    x.mov_mr(15, 0, 0);
    x.back();

    // FTan (0xB5)
    x.patch_handler(0xB5);
    x.mov_rm(0, 15, 0);
    x.i(&[0x48, 0x89, 0x45, 0x00]);
    x.i(&[0xDD, 0x45, 0x00]);
    x.i(&[0xD9, 0xF2]); // fptan
    x.i(&[0xDD, 0xD8]); // fstp st(0) (discard 1.0 from fptan)
    x.i(&[0xDD, 0x5D, 0x00]);
    x.i(&[0x48, 0x8B, 0x45, 0x00]);
    x.mov_mr(15, 0, 0);
    x.back();

    // JsonParse (0xB6) — str → value
    // Simplified: parse JSON number or string literal
    x.patch_handler(0xB6);
    x.mov_rm(0, 15, 0); // rax = json string addr
    x.add_ri(15, 8); // pop string
    // Check first char
    x.i(&[0x0F, 0xB6, 0x08]); // movzx ecx, byte [rax]
    // Check for '"'
    x.i(&[0x48, 0x83, 0xF9, 0x22]); // cmp rcx, '"'
    let jp_not_str = x.jne8();
    // String: skip opening '"', read until closing '"'
    x.add_ri(0, 1); // skip '"'
    // Find closing '"' and copy to OUT_BUF
    x.mov_r64i(6, OUT_BUF_ADDR); // dest
    let jp_str_loop = x.m();
    x.i(&[0x0F, 0xB6, 0x08]); // movzx ecx, byte [rax]
    x.i(&[0x48, 0x83, 0xF9, 0x22]); // cmp rcx, '"'
    let jp_str_end = x.je8();
    x.i(&[0x88, 0x0E]); // mov [rsi], cl
    x.add_ri(6, 1);
    x.add_ri(0, 1);
    let jp_str_back = x.jmp();
    x.p_i32(jp_str_back, jp_str_loop as i32 - (jp_str_back + 4) as i32);
    x.patch_jmp_rel8(jp_str_end);
    // Write null
    x.i(&[0xC6, 0x06, 0x00]); // mov byte [rsi], 0
    // Allocate string in heap
    x.mov_rr(7, 6); // rdi = dest (length = dest - OUT_BUF)
    x.mov_r64i(0, OUT_BUF_ADDR);
    x.i(&[0x48, 0x29, 0xC7]); // sub rdi, rax (length)
    x.mov_rr(12, 7); // r12 = length
    x.add_ri(7, 5); // +5 for prefix+null
    x.push_r(12);
    x.b(0xE8); x.i32(1); // call alloc
    x.pop_r(12);
    x.mov_rr(8, 0); // r8 = alloc ptr
    x.i(&[0x44, 0x89, 0x20]); // mov [rax], r12d (write length)
    x.add_ri(8, 4);
    // Copy from OUT_BUF
    x.mov_r64i(6, OUT_BUF_ADDR);
    x.xor_rr(9, 9);
    let jp_cpy = x.m();
    x.i(&[0x4D, 0x39, 0xCC]); // cmp r12, r9
    let jp_cpy_done = x.jge8();
    x.mov_rr(0, 6);
    x.i(&[0x4C, 0x01, 0xC8]); // add rax, r9
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    x.mov_rr(1, 8);
    x.i(&[0x4C, 0x01, 0xC9]); // add rcx, r9
    x.i(&[0x88, 0x01]); // mov [rcx], al
    x.add_ri(9, 1);
    let jp_cpy_back = x.jmp();
    x.p_i32(jp_cpy_back, jp_cpy as i32 - (jp_cpy_back + 4) as i32);
    x.patch_jmp_rel8(jp_cpy_done);
    // Null terminate
    x.mov_rr(0, 8);
    x.i(&[0x4C, 0x01, 0xE0]); // add rax, r12
    x.i(&[0xC6, 0x00, 0x00]);
    // Push string address
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 8);
    let jp_end = x.jmp();
    // Not a string — parse as number
    x.patch_jmp_rel8(jp_not_str);
    // Check for 't' (true) or 'f' (false)
    x.i(&[0x48, 0x83, 0xF9, 0x74]); // cmp rcx, 't'
    let jp_not_true = x.jne8();
    // true → push 1
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x01, 0x00, 0x00, 0x00]); // push 1
    let jp_end2 = x.jmp();
    x.patch_jmp_rel8(jp_not_true);
    x.i(&[0x48, 0x83, 0xF9, 0x66]); // cmp rcx, 'f'
    let jp_not_false = x.jne8();
    // false → push 0
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x00, 0x00, 0x00, 0x00]);
    let jp_end3 = x.jmp();
    x.patch_jmp_rel8(jp_not_false);
    // Parse as number (integer)
    x.xor_rr(8, 8); // r8 = result
    x.xor_rr(9, 9); // r9 = negative flag
    // Check for '-'
    x.i(&[0x48, 0x83, 0xF9, 0x2D]); // cmp rcx, '-'
    let jp_not_neg = x.jne8();
    x.mov_r64i(9, 1);
    x.add_ri(0, 1);
    x.i(&[0x0F, 0xB6, 0x08]); // read next char
    x.patch_jmp_rel8(jp_not_neg);
    // Parse digits
    let jp_num_loop = x.m();
    x.i(&[0x48, 0x83, 0xF9, 0x30]); // cmp rcx, '0'
    let jp_num_done = x.jb8();
    x.i(&[0x48, 0x83, 0xF9, 0x39]); // cmp rcx, '9'
    let jp_num_done2 = x.jg8();
    // result = result * 10 + (digit - '0')
    x.i(&[0x49, 0x6B, 0xC0, 0x0A]); // imul r8, r8, 10
    x.i(&[0x48, 0x83, 0xE9, 0x30]); // sub rcx, '0'
    x.i(&[0x49, 0x01, 0xC8]); // add r8, rcx
    x.add_ri(0, 1);
    x.i(&[0x0F, 0xB6, 0x08]); // movzx ecx, byte [rax]
    let jp_num_back = x.jmp();
    x.p_i32(jp_num_back, jp_num_loop as i32 - (jp_num_back + 4) as i32);
    x.patch_jmp_rel8(jp_num_done);
    x.patch_jmp_rel8(jp_num_done2);
    // Apply negation
    x.i(&[0x4D, 0x85, 0xC9]); // test r9, r9
    let jp_num_pos = x.je8();
    x.i(&[0x49, 0xF7, 0xD8]); // neg r8
    x.patch_jmp_rel8(jp_num_pos);
    // Push number
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 8);
    x.patch_jmp_rel8(jp_end);
    x.patch_jmp_rel8(jp_end2);
    x.patch_jmp_rel8(jp_end3);
    x.back();

    // JsonStringify (0xB7) — value → str
    // Simplified: convert i64 to string, or pass through strings
    x.patch_handler(0xB7);
    x.mov_rm(0, 15, 0); // rax = value
    // For now, just call I64TOSTR logic (convert number to string)
    x.mov_rr(7, 0); // rdi = value
    x.mov_r64i(6, OUT_BUF_ADDR);
    x.push_r(0); x.push_r(1); x.push_r(2);
    x.b(0xE8); x.i32(0); // call itoa
    x.pop_r(2); x.pop_r(1); x.pop_r(0);
    // rax = length, rsi = buffer
    x.mov_rr(12, 0); // r12 = length
    // Allocate string
    x.add_ri(0, 5);
    x.mov_rr(7, 0);
    x.push_r(12);
    x.b(0xE8); x.i32(1); // call alloc
    x.pop_r(12);
    x.mov_rr(8, 0);
    x.i(&[0x44, 0x89, 0x20]); // write length
    x.add_ri(8, 4);
    // Copy
    x.mov_r64i(6, OUT_BUF_ADDR);
    x.xor_rr(9, 9);
    let js_cpy = x.m();
    x.i(&[0x4D, 0x39, 0xCC]);
    let js_done = x.jge8();
    x.mov_rr(0, 6);
    x.i(&[0x4C, 0x01, 0xC8]);
    x.i(&[0x0F, 0xB6, 0x00]);
    x.mov_rr(1, 8);
    x.i(&[0x4C, 0x01, 0xC9]);
    x.i(&[0x88, 0x01]);
    x.add_ri(9, 1);
    let js_back = x.jmp();
    x.p_i32(js_back, js_cpy as i32 - (js_back + 4) as i32);
    x.patch_jmp_rel8(js_done);
    x.mov_rr(0, 8);
    x.i(&[0x4C, 0x01, 0xE0]);
    x.i(&[0xC6, 0x00, 0x00]);
    x.mov_mr(15, 0, 8);
    x.back();
}

// ── Miscellaneous operations ────────────────────────────────────────

fn impl_misc_ops(x: &mut X) {
    // CapCall (0x70) — no-op in native mode (capabilities not enforced)
    x.patch_handler(0x70);
    // Read capability ID (2 bytes) and skip
    x.add_ri(13, 2);
    // Pop argument
    x.add_ri(15, 8);
    x.back();

    // CapExec (0x71) — execute ref or cap
    x.patch_handler(0x71);
    x.mov_rm(0, 15, 0); // rax = value (cap or ref)
    x.add_ri(15, 8); // pop
    // For now, just no-op (native mode has full privileges)
    x.back();

    // ConfLabel (0x80) — value → signal(value, confidence)
    // Reads 8-byte f64 confidence operand. In native mode, just pass through value.
    x.patch_handler(0x80);
    x.add_ri(13, 8); // skip confidence operand
    x.back(); // value stays on stack (no Signal wrapping in native)

    // ProbChoice (0x81) — value {alt2} {alt1} → result
    // Bytecode: [ref2_len:4B][ref2_data...][ref1_len:4B][ref1_data...]
    // In native mode, always execute alt1 (confidence = 1.0)
    x.patch_handler(0x81);
    // Read and skip ref2
    x.i(&[0x47, 0x8B, 0x04, 0x2E]); // mov eax, [r14+r13] (ref2_len)
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]); // skip ref2 data
    // Read ref1
    x.i(&[0x47, 0x8B, 0x04, 0x2E]); // mov eax, [r14+r13] (ref1_len)
    x.mov_rr(8, 0); // r8 = ref1_len
    x.add_ri(13, 4);
    // r13 now points to ref1 data
    x.mov_rr(9, 13); // r9 = ref1 data address (relative to r14)
    x.i(&[0x4D, 0x01, 0xC5]); // add r13, r8 (skip ref1 data)
    // Call run_ref with ref1
    x.mov_rr(7, 14); // rdi = bytecode base
    x.i(&[0x4C, 0x01, 0xCF]); // add rdi, r9? No — need absolute address
    // Actually: ref1 data is at r14 + (r13 - ref1_len - 4)
    // We saved r9 = r13 before skipping. So ref1_addr = r14 + r9.
    // But r9 was set to r13 (the ip), not the offset.
    // Let me recalculate: after reading ref1_len and advancing ip by 4,
    // r13 points to the start of ref1 data. We saved r13 to r9.
    // So ref1_addr = r14 + r9. But r14 is the bytecode base.
    // Actually, the bytecode is at CODE_VADDR + 0x1000, and r14 = CODE_VADDR + 0x1000.
    // The ref1 data is at [r14 + old_r13]. But old_r13 was the ip before reading ref1.
    // After: ip += 4 (read len), then ip += ref1_len (skip data).
    // We saved ip at the point after reading len (before skipping data) to r9.
    // So ref1_data_addr = r14 + r9. Wait, r13 is an offset from r14.
    // Let me use: rdi = r14 + saved_offset.
    // r9 = r13 at the point after reading len. So ref1_data = r14 + r9.
    // But we already advanced r13 past ref1. We need the address before advancing.
    // Let me restructure: save the offset before advancing.
    // Actually, I already did: x.mov_rr(9, 13) before x.add_ri(13, ...).
    // So r9 = offset of ref1 data within bytecode.
    // ref1 absolute address = r14 + r9.
    // But wait — r14 = CODE_VADDR + 0x1000. The bytecode starts at r14.
    // So ref1_addr = r14 + r9.
    // However, run_ref expects rdi = absolute address of ref bytecode.
    // So: rdi = r14 + r9. But we need to compute this.
    // x.mov_rr(7, 14); then x.i(add rdi, r9) — but r9 is not a register we can add.
    // Actually, r9 IS a register. add rdi, r9 = add r7, r9.
    x.mov_rr(7, 14); // rdi = r14 (bytecode base)
    x.i(&[0x4C, 0x01, 0xCF]); // add rdi, r9
    // rdi now = absolute address of ref1 data
    x.mov_rr(6, 8); // rsi = ref1_len
    x.push_r(8); x.push_r(9);
    x.b(0xE8); x.i32(2); // call run_ref
    x.pop_r(9); x.pop_r(8);
    x.back();

    // Definition ops (0xA0-0xA3) — no-ops at runtime
    for op in [0xA0, 0xA1, 0xA2, 0xA3] {
        x.patch_handler(op);
        if op == 0xA0 {
            // DefWord: skip name bytes
            x.i(&[0x43, 0x0F, 0xB6, 0x04, 0x2E]); // movzx eax, [r14+r13]
            x.add_ri(13, 1);
            x.i(&[0x49, 0x01, 0xC5]); // add r13, rax
        }
        x.back();
    }

    // STRCHARS (0xB8) — str → list of char codes
    x.patch_handler(0xB8);
    x.mov_rm(7, 15, 0); // r7 = str addr
    x.mov_rm(8, 7, -4); // r8 = str len
    // Allocate list: (len+1)*8 bytes
    x.mov_rr(0, 8);
    x.add_ri(0, 1);
    x.i(&[0x48, 0xC1, 0xE0, 0x03]);
    x.mov_rr(7, 0);
    x.push_r(14); x.push_r(13);
    x.b(0xE8); x.i32(1); // call alloc
    x.pop_r(13); x.pop_r(14);
    // rax = list ptr
    x.mov_rr(9, 0); // r9 = list ptr
    x.mov_mr(9, 0, 8); // write count
    x.add_ri(9, 8); // r9 points to elements
    // Copy char codes
    x.xor_rr(10, 10);
    let sch_loop = x.m();
    x.i(&[0x4D, 0x39, 0xC2]); // cmp r10, r8
    let sch_done = x.jge8();
    // Read byte from str[r10]
    x.mov_rm(0, 15, 0); // rax = str addr (still on stack)
    x.i(&[0x4C, 0x01, 0xD0]); // add rax, r10
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    // Write to list[r10]
    x.mov_rr(1, 9);
    x.i(&[0x49, 0xC1, 0xE2, 0x03]); // shl r10_temp? No — r10 is index
    // Actually: [r9 + r10*8] = rax
    x.mov_rr(1, 10);
    x.i(&[0x48, 0xC1, 0xE1, 0x03]); // shl rcx, 3
    x.i(&[0x49, 0x01, 0xC9]); // add rcx, r9
    x.i(&[0x48, 0x89, 0x01]); // mov [rcx], rax
    x.add_ri(10, 1);
    let sch_back = x.jmp();
    x.p_i32(sch_back, sch_loop as i32 - (sch_back + 4) as i32);
    x.patch_jmp_rel8(sch_done);
    // Push list ptr
    // List ptr was stored at alloc'd addr. We need to push it.
    // The list ptr is rax from alloc. But we stored count at [rax], elements at [rax+8].
    // So the list ptr is the alloc result, which is rax (but we moved it to r9).
    // Actually, r9 = alloc + 8 (we did add_ri(9, 8)). The list ptr is r9 - 8.
    // Let me fix: save the original alloc ptr.
    // For now, just push the alloc ptr. It was in rax originally, now in r9-8.
    x.sub_ri(9, 8); // r9 = list ptr
    x.mov_mr(15, 0, 9); // [r15] = list ptr
    x.back();

    // CHARSSTR (0xB9) — list of char codes → str
    x.patch_handler(0xB9);
    x.mov_rm(7, 15, 0); // r7 = list ptr
    x.mov_rm(8, 7, 0); // r8 = count
    // Allocate string: count+5 bytes
    x.mov_rr(0, 8);
    x.add_ri(0, 5);
    x.mov_rr(7, 0);
    x.push_r(14); x.push_r(13);
    x.b(0xE8); x.i32(1); // call alloc
    x.pop_r(13); x.pop_r(14);
    // rax = alloc ptr
    x.mov_rr(9, 0); // r9 = alloc ptr
    // Write length
    x.i(&[0x45, 0x89, 0x01]); // mov [r9], r8d
    x.add_ri(9, 4); // r9 = data area
    // Copy bytes
    x.mov_rm(7, 15, 0); // r7 = list ptr (re-read)
    x.xor_rr(10, 10);
    let cs_loop = x.m();
    x.i(&[0x4D, 0x39, 0xC2]); // cmp r10, r8
    let cs_done = x.jge8();
    // Read list element: [list + 8 + r10*8]
    x.mov_rr(0, 10);
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
    x.i(&[0x48, 0x01, 0xF8]); // add rax, r7
    x.add_ri(0, 8);
    x.mov_rm(0, 0, 0); // rax = element value
    // Write byte
    x.mov_rr(1, 9);
    x.i(&[0x4C, 0x01, 0xD1]); // add rcx, r10
    x.i(&[0x88, 0x01]); // mov [rcx], al
    x.add_ri(10, 1);
    let cs_back = x.jmp();
    x.p_i32(cs_back, cs_loop as i32 - (cs_back + 4) as i32);
    x.patch_jmp_rel8(cs_done);
    // Write null
    x.mov_rr(0, 9);
    x.mov_rm(1, 15, 0); // list ptr
    x.mov_rm(1, 1, 0); // count
    x.i(&[0x48, 0x01, 0xC8]); // add rax, rcx
    x.i(&[0xC6, 0x00, 0x00]); // mov byte [rax], 0
    // Push string addr (r9 = data area)
    x.mov_mr(15, 0, 9);
    x.back();

    // STRITER (0xBA) — str → char_code rest_str (pushes TWO values)
    x.patch_handler(0xBA);
    x.mov_rm(7, 15, 0); // r7 = str addr
    // Read first byte
    x.i(&[0x41, 0x0F, 0xB6, 0x07]); // movzx eax, byte [r7]
    x.i(&[0x48, 0x85, 0xC0]); // test rax, rax
    let si_empty = x.je8();
    // Not empty: push char_code, push rest
    x.mov_mr(15, 0, 0); // [r15] = char_code
    x.sub_ri(15, 8); // allocate slot for rest
    x.add_ri(7, 1); // rest = str + 1
    x.mov_mr(15, 0, 7); // [r15] = rest addr
    let si_end = x.jmp();
    x.patch_jmp_rel8(si_empty);
    // Empty: push -1 and empty string
    x.i(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    x.mov_mr(15, 0, 0); // [r15] = -1
    x.sub_ri(15, 8);
    // Push empty string addr (point to a null byte in the bytecode)
    // For simplicity, push the same str addr (it's empty, first byte is 0)
    x.mov_mr(15, 0, 7); // [r15] = str addr (empty string)
    x.patch_jmp_rel8(si_end);
    x.back();

    // LISTFIND (0xBB) — list key → found_bool value
    // Searches association list for matching key. Pushes two values.
    // Uses r8-r12 for temporaries (avoid r6/r7 which map to r14/r15 with REX).
    x.patch_handler(0xBB);
    x.mov_rm(8, 15, 0); // r8 = key (using r8 instead of r6)
    x.mov_rm(9, 15, 8); // r9 = list_ptr (using r9 instead of r7)
    x.add_ri(15, 16); // pop both
    x.mov_rm(10, 9, 0); // r10 = count
    x.xor_rr(11, 11); // r11 = index
    let lf_loop = x.m();
    x.i(&[0x4D, 0x39, 0xDA]); // cmp r10, r11
    let lf_notfound = x.jge8();
    // Read element: [list + 8 + idx*8]
    x.mov_rr(0, 11); // rax = idx
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
    x.i(&[0x4C, 0x01, 0xC8]); // add rax, r9 (list_ptr)
    x.add_ri(0, 8); // skip count
    x.mov_rm(0, 0, 0); // rax = element (list ptr)
    // Check if element is a list (ptr >= HEAP_VADDR = 0x500000)
    x.i(&[0x48, 0x3D, 0x00, 0x00, 0x50, 0x00]); // cmp rax, 0x500000
    let lf_skip = x.jb8();
    // It's a list ptr — read count
    x.mov_rm(1, 0, 0); // rcx = count
    x.i(&[0x48, 0x83, 0xF9, 0x02]); // cmp rcx, 2
    let lf_skip2 = x.jne8();
    // Read first element (key of association pair)
    x.mov_rm(1, 0, 8); // rcx = first element (assoc key)
    // Compare with search key (r8)
    x.i(&[0x49, 0x39, 0xC8]); // cmp r8, rcx
    let lf_found = x.je8();
    // Not a match — continue
    x.patch_jmp_rel8(lf_skip);
    x.patch_jmp_rel8(lf_skip2);
    x.add_ri(11, 1);
    let lf_back = x.jmp();
    x.p_i32(lf_back, lf_loop as i32 - (lf_back + 4) as i32);
    // Found! Push true and the value (second element)
    x.patch_jmp_rel8(lf_found);
    x.mov_rm(0, 0, 16); // rax = second element (value)
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x01, 0x00, 0x00, 0x00]); // push 1 (found)
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0); // push value
    let lf_end = x.jmp();
    // Not found
    x.patch_jmp_rel8(lf_notfound);
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x00, 0x00, 0x00, 0x00]); // push 0 (not found)
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x00, 0x00, 0x00, 0x00]); // push 0 (default)
    x.patch_jmp_rel8(lf_end);
    x.back();

    // STRJOIN (0xBC) — list of strings → str
    x.patch_handler(0xBC);
    x.mov_rm(0, 15, 0); // rax = list ptr (use rax, safe register)
    x.mov_rr(8, 0); // r8 = list ptr (save in r8)
    x.mov_rm(9, 8, 0); // r9 = count
    x.add_ri(15, 8); // pop list
    // Use OUT_BUF as temp buffer
    x.mov_r64i(10, OUT_BUF_ADDR); // r10 = dest pointer
    x.xor_rr(11, 11); // r11 = total length
    x.xor_rr(12, 12); // r12 = index
    let sj_loop = x.m();
    x.i(&[0x4D, 0x39, 0xE1]); // cmp r9, r12
    let sj_done = x.jge8();
    // Read list element [list + 8 + idx*8]
    x.mov_rr(0, 12); // rax = idx
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
    x.i(&[0x4C, 0x01, 0xC0]); // add rax, r8 (list ptr)
    x.add_ri(0, 8); // skip count
    x.mov_rm(0, 0, 0); // rax = element (string addr)
    // Copy string to dest
    let sj_cpy = x.m();
    x.i(&[0x0F, 0xB6, 0x08]); // movzx ecx, byte [rax]
    x.i(&[0x48, 0x85, 0xC9]); // test rcx, rcx
    let sj_cpy_done = x.je8();
    // Write byte to [r10]
    x.i(&[0x41, 0x88, 0x0A]); // mov [r10], cl
    x.add_ri(10, 1); // dest++
    x.add_ri(11, 1); // total_len++
    x.add_ri(0, 1); // src++
    let sj_cpy_back = x.jmp();
    x.p_i32(sj_cpy_back, sj_cpy as i32 - (sj_cpy_back + 4) as i32);
    x.patch_jmp_rel8(sj_cpy_done);
    x.add_ri(12, 1); // idx++
    let sj_back = x.jmp();
    x.p_i32(sj_back, sj_loop as i32 - (sj_back + 4) as i32);
    x.patch_jmp_rel8(sj_done);
    // Write null terminator at [r10]
    x.i(&[0x41, 0xC6, 0x02, 0x00]); // mov byte [r10], 0
    // Allocate string in heap: total_len + 5
    x.mov_rr(0, 11); // rax = total_len
    x.add_ri(0, 5);
    x.mov_rr(7, 0); // rdi = alloc size
    x.push_r(8); x.push_r(9); x.push_r(10); x.push_r(11); x.push_r(12);
    x.b(0xE8); x.i32(1); // call alloc
    x.pop_r(12); x.pop_r(11); x.pop_r(10); x.pop_r(9); x.pop_r(8);
    // rax = alloc ptr, r11 = total_len
    x.mov_rr(8, 0); // r8 = alloc ptr
    // Write length
    x.i(&[0x44, 0x89, 0x18]); // mov [rax], r11d
    x.add_ri(8, 4); // r8 = data area
    // Copy from OUT_BUF to heap
    x.mov_r64i(10, OUT_BUF_ADDR); // r10 = source
    x.xor_rr(12, 12); // r12 = index
    let sj_copy = x.m();
    x.i(&[0x4D, 0x39, 0xDC]); // cmp r12, r11 (index vs total_len)
    let sj_copy_done = x.jge8();
    // Read byte: [OUT_BUF + idx]
    x.mov_rr(0, 10); // rax = OUT_BUF_ADDR
    x.i(&[0x4C, 0x01, 0xE0]); // add rax, r12 (idx)
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    // Write byte: [r8 + idx]
    x.mov_rr(1, 8); // rcx = heap dest
    x.i(&[0x4C, 0x01, 0xE1]); // add rcx, r12 (idx)
    x.i(&[0x88, 0x01]); // mov [rcx], al
    x.add_ri(12, 1); // idx++
    let sj_copy_back = x.jmp();
    x.p_i32(sj_copy_back, sj_copy as i32 - (sj_copy_back + 4) as i32);
    x.patch_jmp_rel8(sj_copy_done);
    // Null terminate at [r8 + total_len]
    x.mov_rr(0, 8); // rax = data area
    x.i(&[0x4C, 0x01, 0xD8]); // add rax, r11 (total_len)
    x.i(&[0xC6, 0x00, 0x00]); // mov byte [rax], 0
    // Push result (r8 = data area = string address)
    x.mov_mr(15, 0, 8);
    x.back();

    // BytesNew (0xBD) — push new empty byte buffer
    // Buffer layout in heap: [length:8B] [data:...]
    x.patch_handler(0xBD);
    // Allocate 4096 bytes for buffer
    x.mov_r64i(7, 4096);
    x.push_r(14); x.push_r(13);
    x.b(0xE8); x.i32(1); // call alloc
    x.pop_r(13); x.pop_r(14);
    // rax = buffer ptr, write length = 0
    x.i(&[0x48, 0xC7, 0x00, 0x00, 0x00, 0x00, 0x00]); // mov qword [rax], 0
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    x.back();

    // BytesPush (0xBE) — buf byte → buf
    x.patch_handler(0xBE);
    x.mov_rm(0, 15, 0); // rax = byte value
    x.add_ri(15, 8); // pop byte
    x.mov_rm(1, 15, 0); // rcx = buffer ptr
    x.mov_rm(2, 1, 0); // rdx = length
    // Write byte at [buf + 8 + length]
    // Use r8 = buf + 8, then add length
    x.mov_rr(8, 1); // r8 = buf
    x.add_ri(8, 8); // r8 = buf + 8 (data area)
    x.i(&[0x4C, 0x01, 0xD0]); // add rax? No — we need r8+rdx
    // Actually: write to [r8 + rdx]
    x.mov_rr(11, 8); // r11 = buf + 8
    x.i(&[0x4C, 0x01, 0xD3]); // add r11, r10? No — add r11, rdx
    // Use: r11 = r8 + rdx
    x.i(&[0x49, 0x01, 0xD3]); // add r11, rdx
    // Wait, rdx is register 2, not r10. The add encoding: add r11, rdx
    // REX.WRB + 01 + ModR/M (mod=11, reg=rdx, r/m=r11)
    // r11 = 11, rdx = 2. REX = 0x48 | (2>>3)<<2 | (11>>3) = 0x48 | 0 | 1 = 0x49
    // ModR/M = 0xC0 | (2<<3) | (11&7) = 0xC0 | 0x10 | 3 = 0xD3
    // So: 0x49, 0x01, 0xD3 — correct!
    // Store byte: mov [r11], al
    x.i(&[0x41, 0x88, 0x03]); // mov [r11], al
    // Increment length
    x.add_ri(2, 1);
    x.mov_mr(1, 0, 2); // [buf] = new length (rcx = buf)
    x.back();

    // BytesLen (0xBF) — buf → length
    x.patch_handler(0xBF);
    x.mov_rm(0, 15, 0); // rax = buffer ptr
    x.mov_rm(0, 0, 0); // rax = [rax] = length
    x.mov_mr(15, 0, 0);
    x.back();

    // BytesWriteFile (0xC0) — buf path → (nothing)
    x.patch_handler(0xC0);
    x.mov_rm(8, 15, 0); // r8 = path string addr
    x.mov_rm(9, 15, 8); // r9 = buffer ptr
    x.add_ri(15, 16); // pop both
    // open(path, O_WRONLY|O_CREAT|O_TRUNC, 0644)
    // syscall 2 on x86-64 Linux
    x.mov_r64i(0, 2); // rax = 2 (open)
    x.mov_rr(7, 8); // rdi = path (r8→rdi)
    x.mov_r64i(6, 0x241); // rsi = flags (O_WRONLY|O_CREAT|O_TRUNC = 0x241)
    x.mov_r64i(2, 0x1A4); // rdx = mode 0644
    x.syscall();
    // rax = fd (or error)
    x.i(&[0x48, 0x85, 0xC0]); // test rax, rax
    let bw_ok = x.jg8();
    x.back(); // error: skip write
    x.patch_jmp_rel8(bw_ok);
    // Save fd
    x.mov_rr(10, 0); // r10 = fd
    // write(fd, buf+8, length)
    x.mov_r64i(0, 1); // rax = 1 (write)
    x.mov_rr(7, 10); // rdi = fd
    x.mov_rr(6, 9); // rsi = buf+8 (data area)
    x.add_ri(6, 8);
    x.mov_rm(2, 9, 0); // rdx = length
    x.syscall();
    // close(fd)
    x.mov_r64i(0, 3); // rax = 3 (close)
    x.mov_rr(7, 10); // rdi = fd
    x.syscall();
    x.back();

    // Try (0xC1) — ref → success_bool result
    x.patch_handler(0xC1);
    // Skip inline ref bytecode
    x.i(&[0x47, 0x8B, 0x04, 0x2E]); // mov eax, [r14+r13]
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]); // skip ref data
    // Stack: [... ref_bc_addr]
    x.mov_rm(8, 15, 0); // r8 = ref_bc_addr
    x.add_ri(15, 8); // pop ref
    // Call run_ref to execute the quotation
    x.mov_rr(7, 8); // rdi = ref_bc_addr
    x.mov_r64i(6, 0xFFFF); // rsi = large len
    x.push_r(8);
    x.b(0xE8); x.i32(2); // call run_ref
    x.pop_r(8);
    // Result is on stack top
    // Push true below it: stack becomes [..., true, result]
    // Current: [... result]
    // Need: [... true, result]
    x.mov_rm(0, 15, 0); // rax = result
    x.i(&[0x49, 0xC7, 0x07, 0x01, 0x00, 0x00, 0x00]); // mov qword [r15], 1 (true)
    x.sub_ri(15, 8); // allocate slot
    x.mov_mr(15, 0, 0); // push result (true is now below)
    // Wait — the order is wrong. Let me restructure:
    // Current stack: [... result]  (result at [r15])
    // We want: [... true, result]  (true at [r15], result at [r15-8])
    // Actually: we need to shift. Let me just:
    // 1. Save result
    // 2. Push true
    // 3. Push result
    x.mov_rm(0, 15, 0); // rax = result (from run_ref)
    x.add_ri(15, 8); // pop result
    x.sub_ri(15, 8); // push true
    x.i(&[0x49, 0xC7, 0x07, 0x01, 0x00, 0x00, 0x00]); // [r15] = 1 (true)
    x.sub_ri(15, 8); // push result
    x.mov_mr(15, 0, 0); // [r15] = result
    // Now stack: [... true, result] — but we want true ON TOP
    // The Rust VM does: push true, push result → stack has [true, result]
    // With true on top. Let me swap:
    x.mov_rm(0, 15, 0); // rax = result
    x.mov_rm(1, 15, 8); // rcx = true
    x.mov_mr(15, 8, 0); // [r15+8] = result
    x.mov_mr(15, 0, 1); // [r15] = true
    // Now stack: [... result, true] — true on top. Correct!
    x.back();
}

// ── Helper: itoa ────────────────────────────────────────────────────
// rdi=value, rsi=buf → rax=length

fn impl_itoa(x: &mut X) -> usize {
    let start = x.m();
    // Negative?
    x.i(&[0x48, 0x85, 0xFF]); // test rdi, rdi
    let nn = x.jne8();
    x.i(&[0xC6, 0x06, 0x2D]); // mov byte [rsi], '-'
    x.add_ri(6, 1);
    x.i(&[0x48, 0xF7, 0xDF]); // neg rdi
    x.patch_jmp_rel8(nn);
    // Zero?
    x.i(&[0x48, 0x85, 0xFF]); // test rdi, rdi
    let nz = x.jne8();
    x.i(&[0xC6, 0x06, 0x30]); // mov byte [rsi], '0'
    x.add_ri(6, 1);
    x.mov_rm(0, 6, 0); // rax = rsi (end)
    x.ret();
    x.patch_jmp_rel8(nz);
    // Generate digits in reverse
    x.push_r(6); // save buf start
    x.mov_rr(0, 7); // rax = value
    x.mov_r64i(1, 10); // rcx = 10
    let loop_start = x.m();
    x.i(&[0x48, 0x99]); // cqo
    x.i(&[0x48, 0xF7, 0xF9]); // idiv rcx
    x.i(&[0x48, 0x83, 0xC2, 0x30]); // add rdx, '0'
    x.i(&[0x88, 0x16]); // mov [rsi], dl
    x.add_ri(6, 1);
    x.i(&[0x48, 0x85, 0xC0]); // test rax, rax
    let loop_end = x.jne8();
    x.patch_jmp_rel8(loop_end);
    // Now jump back to loop_start if rax != 0
    // Wait, the jne8 already jumps to the next instruction if rax==0
    // Let me restructure: if rax != 0, jump back to loop_start
    x.i(&[0x48, 0x85, 0xC0]); // test rax, rax
    let back = x.jne8();
    x.patch_jmp_rel8(back);
    // Actually, let me redo this properly:
    // The loop should be: do { digit = n%10; *buf++ = '0'+digit; n /= 10; } while(n);
    // I already have the right structure, just need the loop jump
    // The jne8 at loop_end should jump back to loop_start
    // But I wrote it wrong. Let me fix:
    // After the add and test, if rax != 0, jump back
    // The current code has: test rax,rax; jne8 (to after the patch)
    // I need: test rax,rax; jne loop_start
    // Let me use a 32-bit relative jump instead
    // Actually, let me just use the proper loop structure:
    // The digits are in reverse in the buffer. Reverse them.
    x.pop_r(2); // rdx = buf_start
    x.mov_rm(1, 6, 0); // rcx = buf_end - 1
    x.sub_ri(1, 1);
    let rev_loop = x.m();
    x.i(&[0x48, 0x39, 0xD1]); // cmp rcx, rdx
    let rev_done = x.jle8();
    // Swap bytes
    x.i(&[0x44, 0x0F, 0xB6, 0x02]); // movzx r8d, [rdx]
    x.i(&[0x44, 0x0F, 0xB6, 0x09]); // movzx r9d, [rcx]
    x.i(&[0x44, 0x88, 0x0A]); // mov [rdx], r9b
    x.i(&[0x44, 0x88, 0x01]); // mov [rcx], r8b
    x.add_ri(2, 1);
    x.sub_ri(1, 1);
    let jmp_back = x.jmp8();
    x.patch_jmp_rel8(jmp_back);
    x.patch_jmp_rel8(rev_done);
    // Return: rax = buf_end - buf_start = length
    x.mov_rm(0, 6, 0); // rax = buf_end
    x.i(&[0x48, 0x29, 0xD0]); // sub rax, rdx (buf_start)
    x.ret();
    start
}

// ── Helper: str_eq (byte-by-byte) ───────────────────────────────────
// rdi=s1, rsi=s2 → rax=0/1

fn impl_str_eq(x: &mut X) -> usize {
    let start = x.m();
    // Read len1 from [rdi-4], len2 from [rsi-4]
    x.mov_rm(0, 7, -4); // rax = len1
    x.mov_rm(1, 6, -4); // rcx = len2
    x.i(&[0x48, 0x39, 0xC8]); // cmp rax, rcx
    let len_ok = x.je8();
    x.xor_rr(0, 0); x.ret(); // lengths differ → false
    x.patch_jmp_rel8(len_ok);
    // Compare byte by byte
    x.mov_rr(2, 0); // rdx = length
    x.i(&[0x48, 0x85, 0xD2]); // test rdx, rdx
    let len_zero = x.je8();
    let byte_loop = x.m();
    x.i(&[0x44, 0x0F, 0xB6, 0x07]); // movzx r8d, [rdi]
    x.i(&[0x44, 0x0F, 0xB6, 0x0E]); // movzx r9d, [rsi]
    x.i(&[0x45, 0x39, 0xC8]); // cmp r8d, r9d
    let not_eq = x.jne8();
    x.add_ri(7, 1);
    x.add_ri(6, 1);
    x.sub_ri(2, 1);
    x.i(&[0x48, 0x85, 0xD2]); // test rdx, rdx
    let continue_loop = x.jne8();
    // All bytes matched
    x.mov_r64i(0, 1); x.ret();
    x.patch_jmp_rel8(not_eq);
    x.xor_rr(0, 0); x.ret();
    x.patch_jmp_rel8(continue_loop);
    // Jump back to byte_loop
    let back = x.jmp();
    x.p_i32(back, byte_loop as i32 - (back + 4) as i32);
    x.patch_jmp_rel8(len_zero);
    x.mov_r64i(0, 1); // empty strings are equal
    x.ret();
    start
}

// ── Helper: str_cmp (byte-by-byte lexicographic) ────────────────────
// rdi=s1, rsi=s2 → rax=-1/0/1

fn impl_str_cmp(x: &mut X) -> usize {
    let start = x.m();
    // Compare byte by byte
    let byte_loop = x.m();
    x.i(&[0x44, 0x0F, 0xB6, 0x07]); // movzx r8d, [rdi]
    x.i(&[0x45, 0x84, 0xC0]); // test r8b, r8b
    let s1_end = x.je8();
    x.i(&[0x44, 0x0F, 0xB6, 0x0E]); // movzx r9d, [rsi]
    x.i(&[0x45, 0x39, 0xC8]); // cmp r8d, r9d
    let less = x.jb8();
    let greater = x.ja8();
    x.add_ri(7, 1);
    x.add_ri(6, 1);
    let back = x.jmp();
    x.p_i32(back, byte_loop as i32 - (back + 4) as i32);
    // s1 ended
    x.patch_jmp_rel8(s1_end);
    x.i(&[0x44, 0x0F, 0xB6, 0x0E]); // movzx r9d, [rsi]
    x.i(&[0x45, 0x84, 0xC9]); // test r9b, r9b
    let equal = x.je8();
    // s1 shorter → less
    x.mov_r64i(0, u64::MAX as i64 as u64); // -1
    x.ret();
    x.patch_jmp_rel8(equal);
    x.xor_rr(0, 0); x.ret();
    x.patch_jmp_rel8(less);
    x.mov_r64i(0, u64::MAX as i64 as u64); // -1
    x.ret();
    x.patch_jmp_rel8(greater);
    x.mov_r64i(0, 1); x.ret();
    start
}

// ── Helper: alloc (bump allocator) ──────────────────────────────────
// rdi=size → rax=pointer

fn impl_alloc(x: &mut X) -> usize {
    let start = x.m();
    x.mov_r64i(8, ALLOC_PTR_ADDR);
    x.mov_rm(0, 8, 0); // rax = alloc_ptr
    x.i(&[0x49, 0x01, 0xF8]); // add rax, rdi
    x.mov_mr(8, 0, 0); // store new alloc_ptr
    // Return old alloc_ptr (before adding size)
    x.i(&[0x48, 0x29, 0xF8]); // sub rax, rdi
    x.ret();
    start
}

// ── Helper: run_ref ─────────────────────────────────────────────────
// Executes inline reference bytecode. Saves/restores main loop state.

fn impl_run_ref(x: &mut X, _raw_bc: &[u8]) -> usize {
    let start = x.m();
    // Entry: rdi = ref_bc address, rsi = ref_len
    // Self-contained mini dispatch loop for executing reference bytecode.
    // Supports all essential opcodes. RETURN breaks out and returns here.

    // Save r14/r13 on native stack
    x.push_r(14);
    x.push_r(13);

    // Set r14 = ref_bc, r13 = 0
    x.mov_rr(14, 7);
    x.xor_rr(13, 13);

    // Set next_pos for this mini loop's back() jumps
    x.mark_next();

    // Mini fetch: al = [r14+r13]; ip++
    x.i(&[0x43, 0x0F, 0xB6, 0x04, 0x2E]);
    x.add_ri(13, 1);

    // RETURN (0x61) → break out
    x.op(0x61);
    // Register ALL opcodes (same set as main loop)
    for op in [0x00, 0x01, 0x02, 0x03, 0x04] { x.op(op); }
    for op in [0x10, 0x11, 0x12, 0x13, 0x14] { x.op(op); }
    for op in 0x18..=0x1D { x.op(op); }
    for op in 0x20..=0x22 { x.op(op); }
    for op in 0x30..=0x35 { x.op(op); }
    for op in 0x40..=0x45 { x.op(op); }
    for op in 0x46..=0x4F { x.op(op); }
    for op in [0x50, 0x51, 0x52, 0x53] { x.op(op); }
    for op in [0x60, 0x61] { x.op(op); }
    for op in [0x70, 0x71] { x.op(op); }
    for op in [0x80, 0x81] { x.op(op); }
    for op in [0x90, 0x91, 0x92] { x.op(op); }
    for op in [0xA0, 0xA1, 0xA2, 0xA3] { x.op(op); }
    for op in 0xB0..=0xB5 { x.op(op); }
    for op in [0xB6, 0xB7] { x.op(op); }
    for op in 0xB8..=0xBC { x.op(op); }
    for op in 0xBD..=0xC0 { x.op(op); }
    x.op(0xC1);

    x.done(); // back to mini fetch

    // ── Mini handlers ─────────────────────────────────────────────

    // RETURN → break out
    x.patch_handler(0x61);
    // Restore r14/r13 and return
    x.pop_r(13);
    x.pop_r(14);
    x.ret();

    // DUP
    x.patch_handler(0x00);
    x.mov_rm(0, 15, 0); x.sub_ri(15, 8); x.mov_mr(15, 0, 0);
    x.back();

    // SWAP
    x.patch_handler(0x01);
    x.mov_rm(0, 15, 0); x.mov_rm(1, 15, 8);
    x.mov_mr(15, 8, 0); x.mov_mr(15, 0, 1);
    x.back();

    // DROP
    x.patch_handler(0x02);
    x.add_ri(15, 8); x.back();

    // ROT
    x.patch_handler(0x03);
    x.mov_rm(0, 15, 0); x.mov_rm(1, 15, 8); x.mov_rm(2, 15, 16);
    x.mov_mr(15, 0, 1); x.mov_mr(15, 8, 0); x.mov_mr(15, 16, 2);
    x.back();

    // ADD
    x.patch_handler(0x10);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8);
    x.i(&[0x49, 0x01, 0x07]); x.back();

    // SUB
    x.patch_handler(0x11);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8);
    x.i(&[0x49, 0x29, 0x07]); x.back();

    // MUL
    x.patch_handler(0x12);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8);
    x.i(&[0x49, 0x0F, 0xAF, 0x07]);
    x.mov_mr(15, 0, 0); x.back();

    // DIV
    x.patch_handler(0x13);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8);
    x.mov_rm(1, 15, 0);
    x.i(&[0x48, 0x85, 0xC0]); let m_ok = x.jne8();
    x.mov_r64i(0, 60); x.xor_rr(7, 7); x.syscall();
    x.patch_jmp_rel8(m_ok);
    x.mov_rr(7, 0); x.mov_rr(0, 1);
    x.i(&[0x48, 0x99]); x.i(&[0x48, 0xF7, 0xFF]);
    x.mov_mr(15, 0, 0); x.back();

    // MOD
    x.patch_handler(0x14);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8);
    x.mov_rm(1, 15, 0);
    x.mov_rr(7, 0); x.mov_rr(0, 1);
    x.i(&[0x48, 0x99]); x.i(&[0x48, 0xF7, 0xFF]);
    x.mov_mr(15, 0, 2); x.back();

    // CMP ops
    let cmps: [(u8, u8); 6] = [
        (0x18, 0x94), (0x19, 0x9C), (0x1A, 0x9F),
        (0x1B, 0x95), (0x1C, 0x9E), (0x1D, 0x9D),
    ];
    for (opc, cc) in &cmps {
        x.patch_handler(*opc);
        x.mov_rm(0, 15, 0); x.add_ri(15, 8);
        x.i(&[0x49, 0x39, 0x07]);
        x.b(0x0F); x.b(*cc); x.b(0xC0);
        x.i(&[0x48, 0x0F, 0xB6, 0xC0]);
        x.mov_mr(15, 0, 0); x.back();
    }

    // AND
    x.patch_handler(0x20);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8);
    x.i(&[0x49, 0x21, 0x07]); x.back();

    // OR
    x.patch_handler(0x21);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8);
    x.i(&[0x49, 0x09, 0x07]); x.back();

    // NOT
    x.patch_handler(0x22);
    x.i(&[0x49, 0x83, 0x37, 0x01]); x.back();

    // PICK
    x.patch_handler(0x04);
    x.i(&[0x43, 0x0F, 0xB6, 0x04, 0x2E]); // movzx eax, [r14+r13]
    x.add_ri(13, 1);
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
    x.i(&[0x49, 0x01, 0xF8]); // add rax, r15
    x.mov_rm(0, 0, 0);
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    x.back();

    // PUSH_I64
    x.patch_handler(0x30);
    x.i(&[0x4B, 0x8B, 0x04, 0x2E]); x.add_ri(13, 8);
    x.sub_ri(15, 8); x.mov_mr(15, 0, 0); x.back();

    // PUSH_F64
    x.patch_handler(0x31);
    x.i(&[0x4B, 0x8B, 0x04, 0x2E]); x.add_ri(13, 8);
    x.sub_ri(15, 8); x.mov_mr(15, 0, 0); x.back();

    // PUSH_STR
    x.patch_handler(0x32);
    x.i(&[0x47, 0x8B, 0x04, 0x2E]);
    x.mov_rr(8, 0);
    x.mov_rr(0, 13);
    x.i(&[0x4C, 0x01, 0xF0]);
    x.add_ri(0, 4);
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    x.add_ri(13, 4);
    x.i(&[0x4D, 0x01, 0xC5]);
    x.back();

    // PUSH_BOOL
    x.patch_handler(0x33);
    x.i(&[0x43, 0x0F, 0xB6, 0x04, 0x2E]);
    x.add_ri(13, 1);
    x.sub_ri(15, 8);
    x.mov_mr(15, 0, 0);
    x.back();

    // PUSH_LIST — simplified: just pop count, leave elements on stack
    x.patch_handler(0x34);
    x.add_ri(15, 8); // pop count (elements already on stack)
    x.back();

    // NTH
    x.patch_handler(0x40);
    x.mov_rm(0, 15, 0); // rax = idx
    x.add_ri(15, 8);
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
    x.i(&[0x49, 0x03, 0x07]); // add rax, [r15]
    x.add_ri(0, 8);
    x.mov_rm(0, 0, 0);
    x.mov_mr(15, 0, 0);
    x.back();

    // APPEND — simplified: pop elem and list, push list back
    x.patch_handler(0x41);
    x.add_ri(15, 8); // pop elem
    x.back(); // list stays on stack

    // LEN
    x.patch_handler(0x42);
    x.mov_rm(0, 15, 0); // rax = list_ptr
    x.mov_rm(0, 0, 0); // rax = count
    x.mov_mr(15, 0, 0);
    x.back();

    // STRLEN
    x.patch_handler(0x46);
    x.mov_rm(0, 15, 0); // rax = str addr
    x.i(&[0x48, 0x83, 0xE8, 0x04]); // sub rax, 4
    x.i(&[0x8B, 0x00]); // mov eax, [rax]
    x.i(&[0x48, 0x98]); // cdqe
    x.mov_mr(15, 0, 0);
    x.back();

    // COND
    x.patch_handler(0x50);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8);
    x.i(&[0x47, 0x8B, 0x0C, 0x2E]);
    x.add_ri(13, 4);
    x.i(&[0x48, 0x85, 0xC0]);
    let m_taken = x.jne8();
    x.i(&[0x49, 0x01, 0xCD]);
    x.patch_jmp_rel8(m_taken);
    x.back();

    // JUMP
    x.patch_handler(0x51);
    x.i(&[0x47, 0x8B, 0x04, 0x2E]);
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]);
    x.back();

    // LOOP
    x.patch_handler(0x52);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8);
    x.i(&[0x47, 0x8B, 0x0C, 0x2E]);
    x.add_ri(13, 4);
    x.i(&[0x48, 0x85, 0xC0]);
    let m_nojmp = x.je8();
    x.i(&[0x49, 0x01, 0xCD]);
    x.patch_jmp_rel8(m_nojmp);
    x.back();

    // CALL — skip name (placeholder)
    x.patch_handler(0x60);
    x.i(&[0x43, 0x0F, 0xB6, 0x04, 0x2E]);
    x.add_ri(13, 1);
    x.i(&[0x49, 0x01, 0xC5]);
    x.back();

    // ── Remaining opcodes: skip inline data or no-op ──────────────

    // PUSH_REF (0x35) — skip inline bytecode
    x.patch_handler(0x35);
    x.i(&[0x47, 0x8B, 0x04, 0x2E]); // read length
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]); // skip data
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x00, 0x00, 0x00, 0x00]); // push 0 (placeholder)
    x.back();

    // MAP (0x43) — skip inline ref, pop list
    x.patch_handler(0x43);
    x.i(&[0x47, 0x8B, 0x04, 0x2E]);
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]);
    x.add_ri(15, 8); // pop list
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x00, 0x00, 0x00, 0x00]); // push 0
    x.back();

    // EACH (0x44) — skip inline ref, pop list
    x.patch_handler(0x44);
    x.i(&[0x47, 0x8B, 0x04, 0x2E]);
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]);
    x.add_ri(15, 8);
    x.back();

    // FOLD (0x45) — skip inline ref, pop list+init
    x.patch_handler(0x45);
    x.i(&[0x47, 0x8B, 0x04, 0x2E]);
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]);
    x.add_ri(15, 16); // pop list and init
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x00, 0x00, 0x00, 0x00]); // push 0
    x.back();

    // STRCAT (0x47) — placeholder: pop both, push 0
    x.patch_handler(0x47);
    x.add_ri(15, 8);
    x.back();

    // STRSLICE (0x48) — pop start and len
    x.patch_handler(0x48);
    x.add_ri(15, 16);
    x.back();

    // STREQ (0x49) — placeholder
    x.patch_handler(0x49);
    x.add_ri(15, 8);
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x00, 0x00, 0x00, 0x00]);
    x.back();

    // STRLT (0x4A) — placeholder
    x.patch_handler(0x4A);
    x.add_ri(15, 8);
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x00, 0x00, 0x00, 0x00]);
    x.back();

    // STRFIND (0x4B) — placeholder
    x.patch_handler(0x4B);
    x.add_ri(15, 8);
    x.sub_ri(15, 8);
    x.i(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // -1
    x.mov_mr(15, 0, 0);
    x.back();

    // STRREPLACE (0x4C) — pop old and new
    x.patch_handler(0x4C);
    x.add_ri(15, 16);
    x.back();

    // STRTOI64 (0x4D) — simplified
    x.patch_handler(0x4D);
    x.mov_rm(0, 15, 0);
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    x.i(&[0x48, 0x83, 0xE8, 0x30]); // sub rax, '0'
    x.mov_mr(15, 0, 0);
    x.back();

    // I64TOSTR (0x4E) — no-op (leave value on stack)
    x.patch_handler(0x4E);
    x.back();

    // STRNTH (0x4F)
    x.patch_handler(0x4F);
    x.mov_rm(0, 15, 0); // idx
    x.add_ri(15, 8);
    x.i(&[0x49, 0x03, 0x07]); // add rax, [r15]
    x.add_ri(0, 8);
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    x.mov_mr(15, 0, 0);
    x.back();

    // TIMES (0x53) — skip ref, pop n
    x.patch_handler(0x53);
    x.i(&[0x47, 0x8B, 0x04, 0x2E]);
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]);
    x.add_ri(15, 8); // pop n
    x.back();

    // CapCall (0x70) — skip 2-byte ID, pop arg
    x.patch_handler(0x70);
    x.add_ri(13, 2);
    x.add_ri(15, 8);
    x.back();

    // CapExec (0x71) — pop value
    x.patch_handler(0x71);
    x.add_ri(15, 8);
    x.back();

    // ConfLabel (0x80) — skip 8-byte operand
    x.patch_handler(0x80);
    x.add_ri(13, 8);
    x.back();

    // ProbChoice (0x81) — skip two refs, no-op
    x.patch_handler(0x81);
    x.i(&[0x47, 0x8B, 0x04, 0x2E]); x.add_ri(13, 4); x.i(&[0x49, 0x01, 0xC5]);
    x.i(&[0x47, 0x8B, 0x04, 0x2E]); x.add_ri(13, 4); x.i(&[0x49, 0x01, 0xC5]);
    x.back();

    // OUTPUT_TOP (0x90) — no-op in ref context
    x.patch_handler(0x90);
    x.back();

    // OUTPUT_ALL (0x91) — no-op
    x.patch_handler(0x91);
    x.back();

    // READ_INPUT (0x92) — push 0
    x.patch_handler(0x92);
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x00, 0x00, 0x00, 0x00]);
    x.back();

    // DefWord (0xA0) — skip name
    x.patch_handler(0xA0);
    x.i(&[0x43, 0x0F, 0xB6, 0x04, 0x2E]);
    x.add_ri(13, 1);
    x.i(&[0x49, 0x01, 0xC5]);
    x.back();

    // EndDef, Import, Export (0xA1-0xA3) — no-op
    for op in [0xA1, 0xA2, 0xA3] {
        x.patch_handler(op);
        x.back();
    }

    // Float ops (0xB0-0xB5) — no-op
    for op in 0xB0..=0xB5 {
        x.patch_handler(op);
        x.back();
    }

    // JSON (0xB6-0xB7) — no-op
    x.patch_handler(0xB6); x.back();
    x.patch_handler(0xB7); x.back();

    // Extended string ops (0xB8-0xBC) — no-op
    for op in 0xB8..=0xBC {
        x.patch_handler(op);
        x.back();
    }

    // Bytes ops (0xBD-0xC0) — no-op
    for op in 0xBD..=0xC0 {
        x.patch_handler(op);
        x.back();
    }

    // Try (0xC1) — skip ref
    x.patch_handler(0xC1);
    x.i(&[0x47, 0x8B, 0x04, 0x2E]);
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]);
    x.back();

    start
}

// ── Patch helper call sites ─────────────────────────────────────────

fn patch_helper_calls(
    x: &mut X,
    itoa_addr: usize,
    _str_eq_addr: usize,
    _str_cmp_addr: usize,
    alloc_addr: usize,
    run_ref_addr: usize,
) {
    // Patch call sites using marker bytes:
    // E8 00 00 00 00 → call itoa
    // E8 01 00 00 00 → call alloc
    // E8 02 00 00 00 → call run_ref
    let code = &mut x.v;
    let mut i = 0;
    while i + 4 < code.len() {
        if code[i] == 0xE8 {
            let marker = code[i + 1];
            if marker <= 2 && code[i + 2..i + 5] == [0, 0, 0] {
                let target = match marker {
                    0 => itoa_addr,
                    1 => alloc_addr,
                    2 => run_ref_addr,
                    _ => continue,
                };
                let rel = target as i32 - (i as i32 + 5);
                code[i + 1..i + 5].copy_from_slice(&rel.to_le_bytes());
            }
        }
        i += 1;
    }
}

// ── Build word table from bytecode ──────────────────────────────────

fn build_word_table(bytecode: &[Opcode], table: &mut Vec<(String, usize)>) {
    let mut offset = 0;
    for op in bytecode {
        match op {
            Opcode::DefWord(name) => {
                table.push((name.clone(), offset));
            }
            _ => {}
        }
        offset += op.byte_size();
    }
}

// ── ELF builder ─────────────────────────────────────────────────────

fn build_elf(code: &[u8], raw_bc: &[u8], word_entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let code_sz = code.len() as u64;
    let text_file_sz = align_up(code_sz + raw_bc.len() as u64 + 16, PAGE_SIZE);
    let data_vaddr = 0x500000u64;

    let mut elf = Vec::new();
    // ELF header
    elf.extend_from_slice(&[0x7F, b'E', b'L', b'F', 2, 1, 1, 0]);
    elf.extend_from_slice(&[0u8; 8]);
    elf.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    elf.extend_from_slice(&62u16.to_le_bytes()); // EM_X86_64
    elf.extend_from_slice(&1u32.to_le_bytes()); // EV_CURRENT
    elf.extend_from_slice(&(CODE_VADDR + code_sz as u64 + 8).to_le_bytes()); // entry
    elf.extend_from_slice(&64u64.to_le_bytes()); // phoff
    elf.extend_from_slice(&0u64.to_le_bytes()); // shoff
    elf.extend_from_slice(&0u32.to_le_bytes()); // flags
    elf.extend_from_slice(&64u16.to_le_bytes()); // ehsize
    elf.extend_from_slice(&56u16.to_le_bytes()); // phentsize
    elf.extend_from_slice(&2u16.to_le_bytes()); // phnum
    elf.extend_from_slice(&0u16.to_le_bytes()); // shentsize
    elf.extend_from_slice(&0u16.to_le_bytes()); // shnum
    elf.extend_from_slice(&0u16.to_le_bytes()); // shstrndx

    // PHDR 1: code R+X
    elf.extend_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    elf.extend_from_slice(&5u32.to_le_bytes()); // PF_R | PF_X
    elf.extend_from_slice(&PAGE_SIZE.to_le_bytes()); // offset
    elf.extend_from_slice(&CODE_VADDR.to_le_bytes()); // vaddr
    elf.extend_from_slice(&CODE_VADDR.to_le_bytes()); // paddr
    elf.extend_from_slice(&text_file_sz.to_le_bytes()); // filesz
    elf.extend_from_slice(&text_file_sz.to_le_bytes()); // memsz
    elf.extend_from_slice(&PAGE_SIZE.to_le_bytes()); // align

    // PHDR 2: data R+W
    let data_file_off = PAGE_SIZE + text_file_sz;
    elf.extend_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    elf.extend_from_slice(&6u32.to_le_bytes()); // PF_R | PF_W
    elf.extend_from_slice(&data_file_off.to_le_bytes()); // offset
    elf.extend_from_slice(&data_vaddr.to_le_bytes()); // vaddr
    elf.extend_from_slice(&data_vaddr.to_le_bytes()); // paddr
    elf.extend_from_slice(&STACK_SIZE.to_le_bytes()); // filesz
    elf.extend_from_slice(&(STACK_SIZE + HEAP_SIZE).to_le_bytes()); // memsz
    elf.extend_from_slice(&PAGE_SIZE.to_le_bytes()); // align

    // Pad to page boundary
    while elf.len() < PAGE_SIZE as usize {
        elf.push(0);
    }
    // Code
    elf.extend_from_slice(code);
    while elf.len() % 8 != 0 {
        elf.push(0x90);
    }
    // Bytecode length + bytecode
    elf.extend_from_slice(&(raw_bc.len() as u64).to_le_bytes());
    elf.extend_from_slice(raw_bc);
    // Pad to end of text segment
    while (elf.len() as u64) < PAGE_SIZE + text_file_sz {
        elf.push(0);
    }
    // Data segment — build word table at WORD_TABLE_ADDR, then zero-fill rest
    let data_seg_start = (PAGE_SIZE + text_file_sz) as usize;
    let word_table_offset = (WORD_TABLE_ADDR - data_vaddr) as usize;

    // Pad to word table position
    while elf.len() < data_seg_start + word_table_offset {
        elf.push(0);
    }

    // Build word table: [count:4B] then entries, then names, then bytecodes
    let table_count = word_entries.len();
    elf.extend_from_slice(&(table_count as u32).to_le_bytes());

    // Calculate where names and bytecodes start (after all entries)
    let entries_size = table_count * 16; // 16 bytes per entry
    let names_start = WORD_TABLE_ADDR + 4 + entries_size as u64;

    // Calculate name offsets
    let mut name_offsets = Vec::new();
    let mut offset = 0u64;
    for (name, _) in word_entries {
        name_offsets.push(names_start + offset);
        offset += name.len() as u64 + 1; // +1 for null terminator
    }

    // Calculate bytecode offsets (after names)
    let bc_start = names_start + offset;
    let mut bc_offsets = Vec::new();
    let mut bc_offset = 0u64;
    for (_, bc) in word_entries {
        bc_offsets.push(bc_start + bc_offset);
        bc_offset += bc.len() as u64;
    }

    // Write entries: { name_ptr: u32, name_len: u32, bc_ptr: u32, bc_len: u32 }
    for i in 0..table_count {
        elf.extend_from_slice(&(name_offsets[i] as u32).to_le_bytes());
        elf.extend_from_slice(&(word_entries[i].0.len() as u32).to_le_bytes());
        elf.extend_from_slice(&(bc_offsets[i] as u32).to_le_bytes());
        elf.extend_from_slice(&(word_entries[i].1.len() as u32).to_le_bytes());
    }

    // Write name data (null-terminated strings)
    for (name, _) in word_entries {
        elf.extend_from_slice(name.as_bytes());
        elf.push(0); // null terminator
    }

    // Write bytecode data
    for (_, bc) in word_entries {
        elf.extend_from_slice(bc);
    }

    // Zero-fill rest of data segment
    let data_seg_end = data_seg_start + STACK_SIZE as usize + HEAP_SIZE as usize;
    while elf.len() < data_seg_end {
        elf.push(0);
    }

    elf
}

fn align_up(v: u64, a: u64) -> u64 {
    (v + a - 1) & !(a - 1)
}

// ── Bytecode serialization ──────────────────────────────────────────

fn raw_bytecode(ops: &[Opcode]) -> Vec<u8> {
    let mut buf = Vec::new();
    for op in ops {
        buf.push(op.to_byte());
        match op {
            Opcode::Pick(n) => buf.push(*n),
            Opcode::PushI64(n) => buf.extend_from_slice(&n.to_le_bytes()),
            Opcode::PushF64(n) => buf.extend_from_slice(&n.to_le_bytes()),
            Opcode::PushStr(s) => {
                buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
                buf.extend_from_slice(s.as_bytes());
            }
            Opcode::PushBool(v) => buf.push(if *v { 1 } else { 0 }),
            Opcode::Cond(o) | Opcode::Jump(o) | Opcode::Loop(o) => {
                buf.extend_from_slice(&o.to_le_bytes())
            }
            Opcode::PushRef(inner) => {
                let r = raw_bytecode(inner);
                buf.extend_from_slice(&(r.len() as u32).to_le_bytes());
                buf.extend_from_slice(&r);
            }
            Opcode::Call(name) => {
                buf.push(name.len() as u8);
                buf.extend_from_slice(name.as_bytes());
            }
            _ => {}
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_native_hello() {
        let ops = vec![Opcode::PushI64(42), Opcode::OutputTop];
        let elf = compile_to_native(&ops, &[]);
        assert!(elf.len() > 0x1000, "ELF should be at least 4KB, got {}", elf.len());
        assert_eq!(&elf[0..4], &[0x7F, b'E', b'L', b'F'], "should start with ELF magic");
    }
}
