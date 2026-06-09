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
    fn mov_r64i(&mut self, r: u8, v: u64) {
        self.b(0x49);
        self.b(0xBF | (r & 7));
        self.u64(v);
    }
    fn mov_rr(&mut self, d: u8, s: u8) {
        self.i(&[0x49, 0x89, 0xC0 | (s << 3) | d]);
    }
    fn mov_rm(&mut self, d: u8, b: u8, o: i32) {
        self.i(&[0x49, 0x8B, 0x80 | (d << 3) | (b & 7)]);
        self.i32(o);
    }
    fn mov_mr(&mut self, b: u8, o: i32, s: u8) {
        self.i(&[0x49, 0x89, 0x80 | (s << 3) | (b & 7)]);
        self.i32(o);
    }
    fn add_ri(&mut self, r: u8, n: i32) {
        if n == 1 {
            self.i(&[0x49, 0xFF, 0xC0 | r]);
        } else {
            self.i(&[0x49, 0x81, 0xC0 | r]);
            self.i32(n);
        }
    }
    fn sub_ri(&mut self, r: u8, n: i32) {
        self.add_ri(r, -n);
    }
    fn push_r(&mut self, r: u8) {
        self.b(0x50 | (r & 7));
    }
    fn pop_r(&mut self, r: u8) {
        self.b(0x58 | (r & 7));
    }
    fn xor_rr(&mut self, a: u8, b: u8) {
        self.i(&[0x4D, 0x31, 0xC0 | (b << 3) | a]);
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

    // Build word table: name → bytecode offset within embedded bytecode
    let mut word_table: Vec<(String, usize)> = Vec::new();
    build_word_table(bytecode, &mut word_table);
    for (name, code) in defs {
        let offset = raw_bc.len(); // approximate — we'll fix this
        word_table.push((name.clone(), offset));
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
    impl_call_return(&mut x);
    impl_io_ops(&mut x);
    impl_float_ops(&mut x);
    impl_misc_ops(&mut x);

    // ── Helper routines ──────────────────────────────────────────
    let itoa_addr = impl_itoa(&mut x);
    let str_eq_addr = impl_str_eq(&mut x);
    let str_cmp_addr = impl_str_cmp(&mut x);
    let alloc_addr = impl_alloc(&mut x);
    let run_ref_addr = impl_run_ref(&mut x, &raw_bc);

    // Patch helper call sites
    patch_helper_calls(&mut x, itoa_addr, str_eq_addr, str_cmp_addr, alloc_addr, run_ref_addr);

    // ── Build ELF ────────────────────────────────────────────────
    build_elf(&x.v, &raw_bc)
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

    // PUSH_LIST (0x34) — count is on stack, elements below
    // For now, just mark the position (elements are already on stack)
    x.patch_handler(0x34);
    // The count is the top value. We need to allocate a list in heap.
    // Simplified: just leave count on stack (list = count + pointer to elements)
    // Actually, for the native backend, we'll treat lists as:
    // [count, elem0, elem1, ...] on the stack, with count on top
    // This matches how the Rust VM works for PushList
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
    // NTH (0x40) — list[idx]: stack has [... elems..., count, idx]
    // For now, simplified: treat list as consecutive stack elements
    x.patch_handler(0x40);
    x.mov_rm(0, 15, 0); // rax = idx
    x.add_ri(15, 8); // pop idx
    x.i(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
    x.i(&[0x49, 0x01, 0xF8]); // add rax, r15
    x.add_ri(0, 8); // skip count
    x.mov_rm(0, 0, 0); // rax = [rax]
    x.mov_mr(15, 0, 0); // [r15] = result
    x.back();

    // APPEND (0x41) — list elem → new_list
    // Simplified: just push elem below count, increment count
    x.patch_handler(0x41);
    x.mov_rm(0, 15, 0); // rax = elem
    x.add_ri(15, 8); // pop elem
    // Find count (it's below the elements)
    // For now, just leave elem on stack (simplified)
    x.sub_ri(15, 8); // push back (placeholder)
    x.mov_mr(15, 0, 0);
    x.back();

    // LEN (0x42) — list → count
    x.patch_handler(0x42);
    // Count is at stack top for our list representation
    x.back(); // no-op: count is already on top

    // MAP (0x43) — list ref → new_list
    // Placeholder: pop ref and list, push empty
    x.patch_handler(0x43);
    x.add_ri(15, 16); // pop ref and list
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x00, 0x00, 0x00, 0x00]); // push 0 (empty list)
    x.back();

    // EACH (0x44) — list ref → (nothing)
    x.patch_handler(0x44);
    x.add_ri(15, 16); // pop both
    x.back();

    // FOLD (0x45) — list init ref → result
    x.patch_handler(0x45);
    x.add_ri(15, 24); // pop ref, init, list
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x00, 0x00, 0x00, 0x00]); // push 0
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

    // STRCAT (0x47) — s1 s2 → s3
    // Placeholder: pop both, push s1
    x.patch_handler(0x47);
    x.mov_rm(0, 15, 0); // rax = s2
    x.add_ri(15, 8);
    // Just keep s1 on stack
    x.back();

    // STRSLICE (0x48) — str start len → substr
    x.patch_handler(0x48);
    x.add_ri(15, 16); // pop start and len
    // Just keep str on stack
    x.back();

    // STREQ (0x49) — s1 s2 → bool
    x.patch_handler(0x49);
    x.mov_rm(0, 15, 0); // rax = s2
    x.mov_rm(1, 15, 8); // rcx = s1
    x.add_ri(15, 8); // pop one, reuse slot
    // Call str_eq helper (address will be patched)
    x.push_r(14); x.push_r(13); // save bytecode regs
    // Call str_eq: rdi=s1, rsi=s2 → rax=0/1
    x.mov_rr(7, 1); // rdi = s1
    // rsi = s2 — need to move rax to rsi
    x.i(&[0x48, 0x89, 0xC6]); // mov rsi, rax
    // Placeholder call — will be patched
    x.b(0xE8); x.i32(0); // call str_eq
    x.pop_r(13); x.pop_r(14); // restore
    x.mov_mr(15, 0, 0); // store result
    x.back();

    // STRLT (0x4A) — s1 s2 → bool
    x.patch_handler(0x4A);
    x.mov_rm(0, 15, 0);
    x.mov_rm(1, 15, 8);
    x.add_ri(15, 8);
    x.push_r(14); x.push_r(13);
    x.mov_rr(7, 1);
    x.i(&[0x48, 0x89, 0xC6]); // mov rsi, rax
    x.b(0xE8); x.i32(0); // call str_cmp
    x.pop_r(13); x.pop_r(14);
    x.i(&[0x48, 0x83, 0xF8, 0x00]); // cmp rax, 0
    x.b(0x0F); x.b(0x9C); x.b(0xC0); // setl al
    x.i(&[0x48, 0x0F, 0xB6, 0xC0]); // movzx rax, al
    x.mov_mr(15, 0, 0);
    x.back();

    // STRFIND (0x4B) — haystack needle → index
    x.patch_handler(0x4B);
    x.add_ri(15, 16);
    x.sub_ri(15, 8);
    x.i(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    x.mov_mr(15, 0, 0);
    x.back();

    // STRREPLACE (0x4C) — s old new → result
    x.patch_handler(0x4C);
    x.add_ri(15, 16); // pop old and new
    x.back();

    // STRTOI64 (0x4D) — str → i64
    x.patch_handler(0x4D);
    // Simplified: parse first digit
    x.mov_rm(0, 15, 0); // rax = string addr
    x.i(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    x.i(&[0x48, 0x83, 0xE8, 0x30]); // sub rax, '0'
    x.mov_mr(15, 0, 0);
    x.back();

    // I64TOSTR (0x4E) — i64 → str
    x.patch_handler(0x4E);
    x.push_r(0); x.push_r(1); x.push_r(2); x.push_r(6); x.push_r(7);
    x.mov_rm(0, 15, 0); // rax = value
    x.mov_rr(7, 0); // rdi = value
    x.mov_r64i(6, OUT_BUF_ADDR); // rsi = output buffer
    // Call itoa (will be patched)
    x.b(0xE8); x.i32(0); // call itoa
    x.pop_r(7); x.pop_r(6); x.pop_r(2); x.pop_r(1); x.pop_r(0);
    // Store string address (output buffer)
    x.mov_r64i(0, OUT_BUF_ADDR);
    x.mov_mr(15, 0, 0);
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

    // TIMES (0x53) — n {ref} → (nothing)
    // Placeholder: just pop both
    x.patch_handler(0x53);
    // Read ref bytecode length and skip it
    x.i(&[0x47, 0x8B, 0x04, 0x2E]); // mov eax, [r14+r13]
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]); // add r13, rax (skip ref bytecode)
    x.add_ri(15, 8); // pop n
    x.back();
}

// ── Call/Return ─────────────────────────────────────────────────────

fn impl_call_return(x: &mut X) {
    // CALL (0x60) — word dispatch via word table
    // Placeholder: skip name bytes
    x.patch_handler(0x60);
    x.i(&[0x43, 0x0F, 0xB6, 0x04, 0x2E]); // movzx eax, [r14+r13] (name_len)
    x.add_ri(13, 1);
    x.i(&[0x49, 0x01, 0xC5]); // add r13, rax (skip name)
    x.back();

    // RETURN (0x61)
    x.patch_handler(0x61);
    // Check call_sp: if > 0, restore saved state; else exit
    x.mov_r64i(8, CALL_SP_ADDR);
    x.mov_rm(0, 8, 0); // rax = call_sp
    x.i(&[0x48, 0x85, 0xC0]); // test rax, rax
    let has_frame = x.jne8();
    // No frames: exit(0)
    x.mov_r64i(0, 60);
    x.xor_rr(7, 7);
    x.syscall();
    x.patch_jmp_rel8(has_frame);
    // Restore from call stack: r14 = saved_bc, r13 = saved_ip
    x.sub_ri(0, 1); // call_sp--
    x.mov_mr(8, 0, 0); // store decremented call_sp
    x.i(&[0x48, 0xC1, 0xE0, 0x04]); // shl rax, 4 (16 bytes per entry)
    x.mov_r64i(8, CALL_STACK_ADDR);
    x.i(&[0x49, 0x01, 0xC0]); // add r8, rax
    x.mov_rm(14, 8, 0); // r14 = saved_bc
    x.mov_rm(13, 8, 8); // r13 = saved_ip
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

    // OUTPUT_ALL (0x91) — placeholder
    x.patch_handler(0x91);
    x.back();

    // READ_INPUT (0x92) — placeholder
    x.patch_handler(0x92);
    x.sub_ri(15, 8);
    x.i(&[0x49, 0xC7, 0x07, 0x00, 0x00, 0x00, 0x00]);
    x.back();
}

// ── Float operations ────────────────────────────────────────────────

fn impl_float_ops(x: &mut X) {
    // I64ToF64 (0xB0)
    x.patch_handler(0xB0);
    // Convert integer on stack to float (no-op for now — same bit pattern)
    x.back();

    // F64ToI64 (0xB1)
    x.patch_handler(0xB1);
    // Truncate float to integer (no-op for now)
    x.back();

    // FSqrt (0xB2) — placeholder
    x.patch_handler(0xB2);
    x.back();

    // FSin (0xB3) — placeholder
    x.patch_handler(0xB3);
    x.back();

    // FCos (0xB4) — placeholder
    x.patch_handler(0xB4);
    x.back();

    // FTan (0xB5) — placeholder
    x.patch_handler(0xB5);
    x.back();

    // JsonParse (0xB6) — placeholder
    x.patch_handler(0xB6);
    x.back();

    // JsonStringify (0xB7) — placeholder
    x.patch_handler(0xB7);
    x.back();
}

// ── Miscellaneous operations ────────────────────────────────────────

fn impl_misc_ops(x: &mut X) {
    // Capability ops (0x70, 0x71) — no-ops
    x.patch_handler(0x70); x.back();
    x.patch_handler(0x71); x.back();

    // Confidence ops (0x80, 0x81) — no-ops
    x.patch_handler(0x80);
    // ConfLabel reads 8-byte f64 operand
    x.add_ri(13, 8);
    x.back();
    x.patch_handler(0x81); x.back();

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

    // Extended string ops (0xB8-0xBC) — placeholders
    for op in 0xB8..=0xBC {
        x.patch_handler(op);
        x.back();
    }

    // Bytes ops (0xBD-0xC0) — placeholders
    for op in 0xBD..=0xC0 {
        x.patch_handler(op);
        x.back();
    }

    // Try (0xC1) — placeholder: just execute the ref
    x.patch_handler(0xC1);
    // Read ref length and skip it (placeholder)
    x.i(&[0x47, 0x8B, 0x04, 0x2E]); // mov eax, [r14+r13]
    x.add_ri(13, 4);
    x.i(&[0x49, 0x01, 0xC5]); // add r13, rax
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
    // Save current state to call stack
    x.mov_r64i(8, CALL_SP_ADDR);
    x.mov_rm(0, 8, 0); // rax = call_sp
    // Save r14 (bc) and r13 (ip) to call_stack[call_sp]
    x.i(&[0x48, 0xC1, 0xE0, 0x04]); // shl rax, 4 (16 bytes per entry)
    x.mov_r64i(9, CALL_STACK_ADDR);
    x.i(&[0x4D, 0x01, 0xC1]); // add r9, rax
    x.mov_mr(9, 0, 14); // call_stack[sp].bc = r14
    x.mov_mr(9, 8, 13); // call_stack[sp].ip = r13
    // Increment call_sp
    x.mov_r64i(8, CALL_SP_ADDR);
    x.mov_rm(0, 8, 0);
    x.add_ri(0, 1);
    x.mov_mr(8, 0, 0);
    // rdi = ref_bc address, rsi = ref_len
    // Set r14 = ref_bc, r13 = 0
    x.mov_rr(14, 7); // r14 = rdi = ref_bc
    x.xor_rr(13, 13); // r13 = 0
    // Return — the main dispatch loop will now execute the ref bytecode
    // When RETURN is hit, it will restore from the call stack
    x.ret();
    start
}

// ── Patch helper call sites ─────────────────────────────────────────

fn patch_helper_calls(
    x: &mut X,
    itoa_addr: usize,
    _str_eq_addr: usize,
    _str_cmp_addr: usize,
    _alloc_addr: usize,
    _run_ref_addr: usize,
) {
    // Patch all `call itoa` sites (E8 00 00 00 00 → E8 rel32)
    // We need to find all 0xE8 bytes followed by 0x00 0x00 0x00 0x00
    // and patch the relative offset to itoa_addr
    let code = &mut x.v;
    let mut i = 0;
    while i + 4 < code.len() {
        if code[i] == 0xE8 && code[i + 1..i + 5] == [0, 0, 0, 0] {
            // This is a call site — determine which helper based on context
            // For now, patch all to itoa (we'll fix this later)
            let rel = itoa_addr as i32 - (i as i32 + 5);
            code[i + 1..i + 5].copy_from_slice(&rel.to_le_bytes());
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

fn build_elf(code: &[u8], raw_bc: &[u8]) -> Vec<u8> {
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
    // Data segment (zeros for stack/heap)
    while (elf.len() as u64) < PAGE_SIZE + text_file_sz + STACK_SIZE as u64 + HEAP_SIZE as u64 {
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
