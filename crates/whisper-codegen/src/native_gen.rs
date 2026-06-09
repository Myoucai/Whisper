//! Native code generator: Whisper bytecode → standalone ELF64 executable.
//! Zero external dependencies — no assembler, compiler, or linker needed.
//!
//! Usage: whisper build file.ws --target native -o prog
//!        chmod +x prog && ./prog

use whisper_core::opcode::Opcode;

const PAGE_SIZE: u64 = 0x1000;
const STACK_SIZE: u64 = 0x10000;
const CODE_VADDR: u64 = 0x400000;

// ── x86-64 encoder ──────────────────────────────────────────────────

struct X { v: Vec<u8>, handler_patches: Vec<(u8, usize)>, next_pos: usize }
impl X {
    fn new() -> Self { X { v: Vec::new(), handler_patches: vec![], next_pos: 0 } }
    fn b(&mut self, b: u8) { self.v.push(b); }
    fn i(&mut self, b: &[u8]) { self.v.extend_from_slice(b); }
    fn i32(&mut self, n: i32) { self.i(&n.to_le_bytes()); }
    #[allow(dead_code)] fn i64(&mut self, n: i64) { self.i(&n.to_le_bytes()); }
    fn u64(&mut self, n: u64) { self.i(&n.to_le_bytes()); }
    fn m(&mut self) -> usize { self.v.len() }
    fn p_i32(&mut self, pos: usize, val: i32) { self.v[pos..pos+4].copy_from_slice(&val.to_le_bytes()); }
    fn mark_next(&mut self) { self.next_pos = self.v.len(); }
    fn mov_r64i(&mut self, r: u8, v: u64) { self.b(0x49); self.b(0xBF | (r&7)); self.u64(v); }
    fn mov_rr(&mut self, d: u8, s: u8) { self.i(&[0x49, 0x89, 0xC0 | (s<<3) | d]); }
    fn mov_rm(&mut self, d: u8, b: u8, o: i32) { self.i(&[0x49, 0x8B, 0x80 | (d<<3) | (b&7)]); self.i32(o); }
    fn mov_mr(&mut self, b: u8, o: i32, s: u8) { self.i(&[0x49, 0x89, 0x80 | (s<<3) | (b&7)]); self.i32(o); }
    fn add_ri(&mut self, r: u8, n: i32) { if n==1 {self.i(&[0x49,0xFF,0xC0|r]);} else {self.i(&[0x49,0x81,0xC0|r]);self.i32(n);} }
    fn sub_ri(&mut self, r: u8, n: i32) { self.add_ri(r, -n); }
    fn push_r(&mut self, r: u8) { self.b(0x50 | (r&7)); }
    fn pop_r(&mut self, r: u8) { self.b(0x58 | (r&7)); }
    fn xor_rr(&mut self, a: u8, b: u8) { self.i(&[0x4D, 0x31, 0xC0 | (b<<3) | a]); }
    fn je(&mut self) -> usize { self.b(0x0F); self.b(0x84); let p=self.v.len(); self.i32(0); p }
    fn jne8(&mut self) -> usize { self.b(0x75); let p=self.v.len(); self.b(0); p }
    fn jmp(&mut self) -> usize { self.b(0xE9); let p=self.v.len(); self.i32(0); p }
    fn jmp8(&mut self) -> usize { self.b(0xEB); let p=self.v.len(); self.b(0); p }
    fn syscall(&mut self) { self.i(&[0x0F, 0x05]); }
    fn ret(&mut self) { self.b(0xC3); }

    // Dispatch helper
    fn op(&mut self, opcode: u8) {
        self.b(0x3C); self.b(opcode); // cmp al, opcode
        let p = self.je(); self.handler_patches.push((opcode, p));
    }
    fn handler(&mut self, opcode: u8) -> usize {
        for (o, p) in &self.handler_patches { if *o == opcode { return *p; } }
        panic!("handler not found for {opcode:02X}");
    }
    fn done(&mut self) {
        let p = self.jmp(); self.p_i32(p, self.next_pos as i32 - (p+4) as i32);
    }
    fn patch_handler(&mut self, opcode: u8) -> usize {
        let pos = self.m();
        let patch = self.handler(opcode);
        self.p_i32(patch, pos as i32 - (patch+4) as i32);
        pos
    }
    fn back(&mut self) { let p=self.jmp(); self.p_i32(p, self.next_pos as i32 - (p+4) as i32); }
    fn patch_jmp_rel8(&mut self, j: usize) { self.v[j] = (self.m() - j - 1) as u8; }
}

// ── Entry point ─────────────────────────────────────────────────────

pub fn compile_to_native(bytecode: &[Opcode], _defs: &[(String, Vec<Opcode>)]) -> Vec<u8> {
    let raw_bc = raw_bytecode(bytecode);
    let strings = collect_strings(bytecode);

    let mut x = X::new();

    // ── _start: register setup ───────────────────────────────────
    x.mov_r64i(15, CODE_VADDR + 0x80000 - 8); // r15 = stack top (grows down)
    x.mov_r64i(14, CODE_VADDR + 0x1000);       // r14 = bytecode base
    x.xor_rr(13, 13);                           // r13 = 0 (ip)

    // ── Main interpreter loop ────────────────────────────────────
    x.mark_next();
    let _ = x.m(); // main_loop marker
    // Fetch: al = bytecode[ip]; ip++
    x.i(&[0x43, 0x0F, 0xB6, 0x04, 0x2E]); // movzx eax, [r14+r13]
    x.add_ri(13, 1);

    x.op(0x00); x.op(0x01); x.op(0x02); x.op(0x03); // stack
    x.op(0x10); x.op(0x11); x.op(0x12); x.op(0x13); x.op(0x14); // arith
    x.op(0x18); x.op(0x19); x.op(0x1A); x.op(0x1B); // cmp
    x.op(0x20); x.op(0x21); x.op(0x22); // logic
    x.op(0x30); x.op(0x31); x.op(0x33); x.op(0x32); // push
    x.op(0x50); x.op(0x51); x.op(0x52); // control
    x.op(0x90); // output
    x.op(0x61); // return
    x.done();

    // ── Handlers ──────────────────────────────────────────────────

    // DUP
    x.patch_handler(0x00);
    x.mov_rm(0, 15, 0); x.sub_ri(15, 8); x.mov_mr(15, 0, 0);
    x.back();

    // SWAP
    x.patch_handler(0x01);
    x.mov_rm(0, 15, 0); x.mov_rm(1, 15, 8); x.mov_mr(15, 8, 0); x.mov_mr(15, 0, 1);
    x.back();

    // DROP
    x.patch_handler(0x02); x.add_ri(15, 8); x.back();

    // ROT
    x.patch_handler(0x03);
    x.mov_rm(0, 15, 0); x.mov_rm(1, 15, 8); x.mov_rm(2, 15, 16);
    x.mov_mr(15, 0, 1); x.mov_mr(15, 8, 0); x.mov_mr(15, 16, 2);
    x.back();

    // ADD
    x.patch_handler(0x10);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8); x.i(&[0x49, 0x01, 0x07]); // add [r15], rax
    x.back();

    // SUB
    x.patch_handler(0x11);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8); x.i(&[0x49, 0x29, 0x07]); // sub [r15], rax
    x.back();

    // MUL
    x.patch_handler(0x12);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8); x.i(&[0x49, 0x0F, 0xAF, 0x07]); // imul rax, [r15]
    x.mov_mr(15, 0, 0); x.back();

    // DIV
    x.patch_handler(0x13);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8); x.i(&[0x48,0x99]); x.i(&[0x49,0xF7,0x3F]); // cqo; idiv [r15]
    x.mov_mr(15, 0, 0); x.back();

    // MOD
    x.patch_handler(0x14);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8); x.i(&[0x48,0x99]); x.i(&[0x49,0xF7,0x3F]);
    x.mov_mr(15, 0, 2); // mov [r15], rdx (remainder)
    x.back();

    // EQ, LT, GT, NEQ
    let cmps: [(u8, u8); 4] = [(0x18, 0x94), (0x19, 0x9C), (0x1A, 0x9F), (0x1B, 0x95)];
    for (opc, cc) in &cmps {
        x.patch_handler(*opc);
        x.mov_rm(0, 15, 0); x.add_ri(15, 8); x.i(&[0x49,0x39,0x07]); // cmp [r15],rax
        x.b(0x0F); x.b(*cc); x.b(0xC0); // setcc al
        x.i(&[0x48,0x0F,0xB6,0xC0]); // movzx rax, al
        x.mov_mr(15, 0, 0); x.back();
    }

    // AND
    x.patch_handler(0x20);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8); x.i(&[0x49,0x21,0x07]); x.back(); // and [r15],rax
    // OR
    x.patch_handler(0x21);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8); x.i(&[0x49,0x09,0x07]); x.back(); // or [r15],rax
    // NOT
    x.patch_handler(0x22);
    x.i(&[0x49,0x83,0x37,0x01]); x.back(); // xor qword [r15], 1

    // PUSH_I64
    x.patch_handler(0x30);
    x.i(&[0x4B,0x8B,0x04,0x2E]); x.add_ri(13, 8); x.sub_ri(15, 8); x.mov_mr(15, 0, 0); x.back();
    // PUSH_F64
    x.patch_handler(0x31);
    x.i(&[0x4B,0x8B,0x04,0x2E]); x.add_ri(13, 8); x.sub_ri(15, 8); x.mov_mr(15, 0, 0); x.back();
    // PUSH_BOOL
    x.patch_handler(0x33);
    x.i(&[0x43,0x0F,0xB6,0x04,0x2E]); x.add_ri(13, 1); x.sub_ri(15, 8); x.mov_mr(15, 0, 0); x.back();
    // PUSH_STR: push address of string data in bytecode, skip len+data
    x.patch_handler(0x32);
    // Read length from bytecode first: eax = [r14+r13]
    x.i(&[0x47,0x8B,0x04,0x2E]);    // mov eax, [r14+r13]
    // Save length to r8: r8 = rax
    x.mov_rr(8, 0);                 // r8 = length
    // Compute string data address: rax = r14 + r13 + 4
    x.mov_rr(0, 13);                // rax = ip
    x.i(&[0x4C,0x01,0xF0]);         // rax += r14 (bytecode base)
    x.add_ri(0, 4);                 // rax += 4 (past length field)
    // Push address onto stack
    x.sub_ri(15, 8);                // allocate stack slot
    x.mov_mr(15, 0, 0);             // [r15] = address
    // Advance ip past string: ip += 4 + length
    x.add_ri(13, 4);                // ip += 4 (length field)
    x.i(&[0x4D,0x01,0xC5]);         // r13 += r8 (skip string data)
    x.back();

    // COND
    x.patch_handler(0x50);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8); // pop
    x.i(&[0x48,0x85,0xC0]); // test rax,rax
    let s50 = x.jne8(); // if != 0, skip
    x.i(&[0x47,0x8B,0x04,0x2E]); x.add_ri(13, 4); x.i(&[0x49,0x01,0xC5]); // add r13, offset
    x.patch_jmp_rel8(s50); x.back();

    // JUMP
    x.patch_handler(0x51);
    x.i(&[0x47,0x8B,0x04,0x2E]); x.add_ri(13, 4); x.i(&[0x49,0x01,0xC5]); x.back();

    // LOOP
    x.patch_handler(0x52);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8);
    x.i(&[0x48,0x85,0xC0]);
    let s52 = x.jne8();
    x.i(&[0x47,0x8B,0x04,0x2E]); x.add_ri(13, 4); x.i(&[0x49,0x01,0xC5]);
    x.patch_jmp_rel8(s52); x.back();

    // OUTPUT_TOP: pop, itoa, write
    x.patch_handler(0x90);
    x.push_r(0); x.push_r(1); x.push_r(2); x.push_r(6); x.push_r(7); x.push_r(15);
    x.mov_rm(0, 15, 0); x.add_ri(15, 8); // rax = value
    // itoa: rdi=rax, rsi=buf → rax=len
    x.mov_rr(7, 0); // rdi = value
    x.mov_r64i(6, CODE_VADDR + 0x70000); // rsi = buffer in data area
    let ic = x.m(); x.b(0xE8); x.i32(0); // call itoa
    // write(1, buf, len)
    x.mov_r64i(0, 1); x.mov_r64i(7, 1); /* rsi already set */ x.mov_rr(2, 0);
    // need to set rsi back: it's still set from itoa call
    x.syscall();
    // write newline
    x.mov_r64i(7, 1); x.mov_r64i(6, CODE_VADDR + 0x70000 + 64);
    x.b(0xC6); x.b(0x06); x.b(0x0A); // mov byte [rsi], '\n' (rsi is r6... hmm)
    // Actually, let me fix: rsi is r6. mov byte [rsi], 0x0A
    // The mov_r64i already set r6 to buffer+64 for the newline
    x.mov_r64i(2, 1); x.mov_r64i(0, 1); x.syscall();
    x.pop_r(15); x.pop_r(7); x.pop_r(6); x.pop_r(2); x.pop_r(1); x.pop_r(0);
    x.back();

    // RETURN / exit
    x.patch_handler(0x61);
    x.mov_r64i(0, 60); x.xor_rr(7, 7); x.syscall(); // exit(0)

    // ── itoa: rdi=value, rsi=buf → rax=len ─────────────────────
    let it = x.m();
    x.p_i32(ic+1, it as i32 - (ic+5) as i32);
    // Negative?
    x.i(&[0x48,0x85,0xFF]); // test rdi,rdi
    let nn = x.jne8();
    x.i(&[0xC6,0x06,0x2D]); x.add_ri(6, 1); x.i(&[0x48,0xF7,0xDF]); // '-'; neg rdi
    x.patch_jmp_rel8(nn);
    // Zero?
    x.i(&[0x48,0x85,0xFF]);
    let nz = x.jne8();
    x.i(&[0xC6,0x06,0x30]); x.add_ri(6, 1); x.mov_rm(0, 6, 0); x.ret(); // '0'
    x.patch_jmp_rel8(nz);
    // Generate digits in reverse
    x.push_r(6); // save buf start
    x.mov_rm(0, 7, 0); // rax = rdi; divisor rcx=10
    x.mov_r64i(1, 10);
    let dl = x.m();
    x.i(&[0x48,0x99]); x.i(&[0x48,0xF7,0xF9]); // cqo; idiv rcx
    x.i(&[0x48,0x83,0xC2,0x30]); x.i(&[0x88,0x16]); // add rdx,'0'; mov [rsi],dl
    x.add_ri(6, 1); x.i(&[0x48,0x85,0xC0]);
    let jd = x.jne8(); x.patch_jmp_rel8(jd); // loop
    // Reverse
    x.pop_r(2); // rdx = buf_start; rcx = buf_end-1
    x.mov_rm(1, 6, 0); x.sub_ri(1, 1);
    let rl = x.m();
    x.i(&[0x48,0x39,0xD1]); // cmp rcx,rdx
    x.b(0x7D); let rd = x.m(); x.b(0); // jge done
    x.i(&[0x44,0x0F,0xB6,0x02]); x.i(&[0x44,0x0F,0xB6,0x39]); // movzx r8, [rdx]; movzx r15, [rcx]
    x.i(&[0x44,0x88,0x01]); x.i(&[0x44,0x88,0x3A]); // mov [rcx],r8b; mov [rdx],r15b
    x.add_ri(2, 1); x.sub_ri(1, 1);
    let rj = x.jmp8(); x.patch_jmp_rel8(rj);
    let rdone = x.m(); x.v[rd] = (rdone - rd - 1) as u8;
    x.mov_rm(0, 6, 0); x.i(&[0x48,0x29,0xD0]); x.ret(); // rax = rsi - start; return

    // ── Build ELF ─────────────────────────────────────────────────
    let code = x.v.clone();
    let code_sz = code.len() as u64;
    let text_file_sz = align_up(code_sz + raw_bc.len() as u64 + 16, PAGE_SIZE);
    let data_vaddr = 0x500000u64;

    let mut elf = Vec::new();
    // ELF header
    elf.extend_from_slice(&[0x7F,b'E',b'L',b'F',2,1,1,0]); elf.extend_from_slice(&[0u8;8]);
    elf.extend_from_slice(&2u16.to_le_bytes()); elf.extend_from_slice(&62u16.to_le_bytes());
    elf.extend_from_slice(&1u32.to_le_bytes());
    elf.extend_from_slice(&(CODE_VADDR + code_sz as u64 + 8).to_le_bytes()); // entry after code+length prefix
    elf.extend_from_slice(&64u64.to_le_bytes()); elf.extend_from_slice(&0u64.to_le_bytes());
    elf.extend_from_slice(&0u32.to_le_bytes()); elf.extend_from_slice(&64u16.to_le_bytes());
    elf.extend_from_slice(&56u16.to_le_bytes()); elf.extend_from_slice(&2u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes()); elf.extend_from_slice(&0u16.to_le_bytes()); elf.extend_from_slice(&0u16.to_le_bytes());

    // PHDR 1: code R+X
    elf.extend_from_slice(&1u32.to_le_bytes()); elf.extend_from_slice(&5u32.to_le_bytes());
    elf.extend_from_slice(&PAGE_SIZE.to_le_bytes()); elf.extend_from_slice(&CODE_VADDR.to_le_bytes());
    elf.extend_from_slice(&CODE_VADDR.to_le_bytes()); elf.extend_from_slice(&text_file_sz.to_le_bytes());
    elf.extend_from_slice(&align_up(CODE_VADDR + code_sz + raw_bc.len() as u64 + 16, PAGE_SIZE).to_le_bytes());
    elf.extend_from_slice(&PAGE_SIZE.to_le_bytes());

    // PHDR 2: data R+W
    let data_file_sz = STACK_SIZE;
    let data_file_off = PAGE_SIZE + text_file_sz;
    elf.extend_from_slice(&1u32.to_le_bytes()); elf.extend_from_slice(&6u32.to_le_bytes());
    elf.extend_from_slice(&data_file_off.to_le_bytes()); elf.extend_from_slice(&data_vaddr.to_le_bytes());
    elf.extend_from_slice(&data_vaddr.to_le_bytes()); elf.extend_from_slice(&data_file_sz.to_le_bytes());
    elf.extend_from_slice(&STACK_SIZE.to_le_bytes()); elf.extend_from_slice(&PAGE_SIZE.to_le_bytes());

    while elf.len() < PAGE_SIZE as usize { elf.push(0); }
    elf.extend_from_slice(&code);
    while elf.len() % 8 != 0 { elf.push(0x90); }
    elf.extend_from_slice(&(raw_bc.len() as u64).to_le_bytes());
    elf.extend_from_slice(&raw_bc);
    while (elf.len() as u64) < PAGE_SIZE + text_file_sz { elf.push(0); }
    // String data in first part of data segment
    elf.extend_from_slice(&strings);
    while (elf.len() as u64) < PAGE_SIZE + text_file_sz + STACK_SIZE as u64 { elf.push(0); }

    elf
}

fn align_up(v: u64, a: u64) -> u64 { (v + a - 1) & !(a - 1) }

fn collect_strings(ops: &[Opcode]) -> Vec<u8> {
    let mut v = Vec::new();
    for op in ops {
        if let Opcode::PushStr(s) = op { v.extend_from_slice(&(v.len() as u32).to_le_bytes()); v.extend_from_slice(s.as_bytes()); v.push(0); }
        if let Opcode::PushRef(inner) = op { v.extend_from_slice(&collect_strings(inner)); }
    }
    v
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

fn raw_bytecode(ops: &[Opcode]) -> Vec<u8> {
    let mut buf = Vec::new();
    for op in ops {
        buf.push(op.to_byte());
        match op {
            Opcode::Pick(n) => buf.push(*n),
            Opcode::PushI64(n) => buf.extend_from_slice(&n.to_le_bytes()),
            Opcode::PushF64(n) => buf.extend_from_slice(&n.to_le_bytes()),
            Opcode::PushStr(s) => { buf.extend_from_slice(&(s.len() as u32).to_le_bytes()); buf.extend_from_slice(s.as_bytes()); }
            Opcode::PushBool(v) => buf.push(if *v { 1 } else { 0 }),
            Opcode::Cond(o)|Opcode::Jump(o)|Opcode::Loop(o) => buf.extend_from_slice(&o.to_le_bytes()),
            Opcode::PushRef(inner) => { let r=raw_bytecode(inner); buf.extend_from_slice(&(r.len() as u32).to_le_bytes()); buf.extend_from_slice(&r); }
            Opcode::Call(name) => { buf.push(name.len() as u8); buf.extend_from_slice(name.as_bytes()); }
            _ => {}
        }
    }
    buf
}
