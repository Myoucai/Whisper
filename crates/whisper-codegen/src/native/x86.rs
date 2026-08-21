//! x86-64 instruction encoder for Whisper native code generation.
//!
//! Encodes x86-64 instructions as byte sequences for direct ELF output.
//! Supports the minimal instruction set needed for a stack-based VM:
//! push, pop, mov, add, sub, imul, idiv, cmp, jmp, call, ret, syscall.

/// x86-64 registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Reg {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
    /// Low byte of RAX (for setcc instructions).
    Al = 0,
}

impl Reg {
    /// Return the 3-bit register encoding for REX prefix / ModRM byte.
    fn bits(self) -> u8 {
        self as u8 & 0x07
    }

    /// Whether this register needs a REX.B prefix bit.
    fn needs_rex_b(self) -> bool {
        (self as u8) >= 8
    }
}

/// Condition codes for conditional jumps and set instructions.
#[derive(Debug, Clone, Copy)]
pub enum CondCode {
    /// Equal (ZF=1)
    E = 0x4,
    /// Not equal (ZF=0)
    Ne = 0x5,
    /// Less than (SF≠OF)
    L = 0xC,
    /// Greater or equal (SF=OF)
    Ge = 0xD,
    /// Less or equal (ZF=1 or SF≠OF)
    Le = 0xE,
    /// Greater (ZF=0 and SF=OF)
    G = 0xF,
}

/// x86-64 instruction encoder.
/// Accumulates machine code bytes in a buffer.
pub struct Encoder {
    pub code: Vec<u8>,
}

impl Encoder {
    pub fn new() -> Self {
        Encoder { code: Vec::new() }
    }

    /// Current position in the code buffer (used for relative offsets).
    pub fn position(&self) -> usize {
        self.code.len()
    }

    /// Emit raw bytes.
    pub fn emit_bytes(&mut self, bytes: &[u8]) {
        self.code.extend_from_slice(bytes);
    }

    /// Emit a single byte.
    pub fn emit_u8(&mut self, b: u8) {
        self.code.push(b);
    }

    /// Emit a 32-bit little-endian value.
    pub fn emit_i32(&mut self, v: i32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    /// Emit a 64-bit little-endian value.
    pub fn emit_i64(&mut self, v: i64) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    // ── REX prefix helpers ──────────────────────────────────────────

    /// Emit REX.W prefix (64-bit operand size).
    fn rex_w(&mut self) {
        self.emit_u8(0x48);
    }

    /// Emit REX.W + B prefix (for r8-r15 as ModRM r/m).
    fn rex_w_b(&mut self) {
        self.emit_u8(0x49);
    }

    /// Emit REX.W + R prefix (for r8-r15 as ModRM reg).
    fn rex_w_r(&mut self) {
        self.emit_u8(0x4C);
    }

    /// Emit REX.W + R + B prefix (both reg and r/m are r8-r15).
    fn rex_w_r_b(&mut self) {
        self.emit_u8(0x4D);
    }

    // ── ModRM byte helpers ──────────────────────────────────────────

    /// ModRM byte: mod=11 (register-to-register), reg, rm.
    fn modrm_rr(&mut self, reg: u8, rm: u8) {
        self.emit_u8(0xC0 | (reg & 0x07) << 3 | (rm & 0x07));
    }

    /// ModRM byte: mod=00 (memory [rm]), reg, rm.
    fn modrm_indirect(&mut self, reg: u8, rm: u8) {
        self.emit_u8((reg & 0x07) << 3 | (rm & 0x07));
        // Special case: [rsp] requires SIB byte
        if (rm & 0x07) == 4 {
            self.emit_u8(0x24); // SIB: scale=0, index=rsp(none), base=rsp
        }
    }

    /// ModRM byte: mod=01 (memory [rm + disp8]), reg, rm, disp8.
    fn modrm_disp8(&mut self, reg: u8, rm: u8, disp: i8) {
        self.emit_u8(0x40 | (reg & 0x07) << 3 | (rm & 0x07));
        if (rm & 0x07) == 4 {
            self.emit_u8(0x24);
        }
        self.emit_u8(disp as u8);
    }

    /// ModRM byte: mod=10 (memory [rm + disp32]), reg, rm, disp32.
    fn modrm_disp32(&mut self, reg: u8, rm: u8, disp: i32) {
        self.emit_u8(0x80 | (reg & 0x07) << 3 | (rm & 0x07));
        if (rm & 0x07) == 4 {
            self.emit_u8(0x24);
        }
        self.emit_i32(disp);
    }

    // ── Instructions ────────────────────────────────────────────────

    /// `push reg` — Push 64-bit register onto stack.
    pub fn push_r64(&mut self, reg: Reg) {
        if reg.needs_rex_b() {
            self.emit_u8(0x41);
        }
        self.emit_u8(0x50 + reg.bits());
    }

    /// `pop reg` — Pop 64-bit value from stack into register.
    pub fn pop_r64(&mut self, reg: Reg) {
        if reg.needs_rex_b() {
            self.emit_u8(0x41);
        }
        self.emit_u8(0x58 + reg.bits());
    }

    /// `mov reg, imm64` — Load 64-bit immediate into register.
    pub fn mov_r64_imm64(&mut self, reg: Reg, imm: i64) {
        if reg.needs_rex_b() {
            self.emit_u8(0x49);
        } else {
            self.rex_w();
        }
        self.emit_u8(0xB8 + reg.bits());
        self.emit_i64(imm);
    }

    /// `mov dst, src` — Register to register (64-bit).
    pub fn mov_r64_r64(&mut self, dst: Reg, src: Reg) {
        if src.needs_rex_b() && dst.needs_rex_b() {
            self.rex_w_r_b();
        } else if src.needs_rex_b() {
            self.rex_w_r();
        } else if dst.needs_rex_b() {
            self.rex_w_b();
        } else {
            self.rex_w();
        }
        self.modrm_rr(src as u8, dst as u8);
    }

    /// `mov [dst], src` — Store 64-bit register to memory [dst].
    pub fn mov_store_indirect(&mut self, dst: Reg, src: Reg) {
        if src.needs_rex_b() || dst.needs_rex_b() {
            // REX handling for extended registers
            let mut rex = 0x48u8;
            if src.needs_rex_b() { rex |= 0x04; }
            if dst.needs_rex_b() { rex |= 0x01; }
            self.emit_u8(rex);
        } else {
            self.rex_w();
        }
        self.emit_u8(0x89);
        self.modrm_indirect(src as u8, dst as u8);
    }

    /// `mov dst, [src]` — Load 64-bit value from memory [src] into dst.
    pub fn mov_load_indirect(&mut self, dst: Reg, src: Reg) {
        if src.needs_rex_b() || dst.needs_rex_b() {
            let mut rex = 0x48u8;
            if dst.needs_rex_b() { rex |= 0x04; }
            if src.needs_rex_b() { rex |= 0x01; }
            self.emit_u8(rex);
        } else {
            self.rex_w();
        }
        self.emit_u8(0x8B);
        self.modrm_indirect(dst as u8, src as u8);
    }

    /// `mov dst, [src + disp32]` — Load 64-bit from [src + offset].
    pub fn mov_load_disp(&mut self, dst: Reg, src: Reg, disp: i32) {
        self.rex_w();
        self.emit_u8(0x8B);
        self.modrm_disp32(dst as u8, src as u8, disp);
    }

    /// `mov [dst + disp32], src` — Store 64-bit to [dst + offset].
    pub fn mov_store_disp(&mut self, dst: Reg, disp: i32, src: Reg) {
        self.rex_w();
        self.emit_u8(0x89);
        self.modrm_disp32(src as u8, dst as u8, disp);
    }

    /// `add dst, src` — 64-bit add.
    pub fn add_r64(&mut self, dst: Reg, src: Reg) {
        self.rex_w();
        self.emit_u8(0x01);
        self.modrm_rr(src as u8, dst as u8);
    }

    /// `sub dst, src` — 64-bit subtract.
    pub fn sub_r64(&mut self, dst: Reg, src: Reg) {
        self.rex_w();
        self.emit_u8(0x29);
        self.modrm_rr(src as u8, dst as u8);
    }

    /// `imul dst, src` — 64-bit signed multiply.
    pub fn imul_r64(&mut self, dst: Reg, src: Reg) {
        self.rex_w();
        self.emit_u8(0x0F);
        self.emit_u8(0xAF);
        self.modrm_rr(dst as u8, src as u8);
    }

    /// `cqo` — Sign-extend RAX into RDX:RAX (for idiv).
    pub fn cqo(&mut self) {
        self.rex_w();
        self.emit_u8(0x99);
    }

    /// `idiv src` — Signed divide RDX:RAX by src. Quotient in RAX, remainder in RDX.
    pub fn idiv_r64(&mut self, src: Reg) {
        self.rex_w();
        self.emit_u8(0xF7);
        self.modrm_rr(7, src as u8); // /7 = idiv
    }

    /// `neg dst` — Two's complement negate.
    pub fn neg_r64(&mut self, dst: Reg) {
        self.rex_w();
        self.emit_u8(0xF7);
        self.modrm_rr(3, dst as u8); // /3 = neg
    }

    /// `cmp r1, r2` — Compare two 64-bit registers.
    pub fn cmp_r64(&mut self, r1: Reg, r2: Reg) {
        self.rex_w();
        self.emit_u8(0x39);
        self.modrm_rr(r2 as u8, r1 as u8);
    }

    /// `cmp r, imm32` — Compare register with sign-extended 32-bit immediate.
    pub fn cmp_r64_imm32(&mut self, reg: Reg, imm: i32) {
        self.rex_w();
        if reg.bits() == 0 {
            // RAX short form
            self.emit_u8(0x3D);
        } else {
            self.emit_u8(0x81);
            self.modrm_rr(7, reg as u8);
        }
        self.emit_i32(imm);
    }

    /// `test r, r` — Logical AND, set flags.
    pub fn test_r64(&mut self, r1: Reg, r2: Reg) {
        self.rex_w();
        self.emit_u8(0x85);
        self.modrm_rr(r2 as u8, r1 as u8);
    }

    /// `setcc dl` — Set byte based on condition code.
    pub fn setcc(&mut self, cc: CondCode, dst: Reg) {
        self.emit_u8(0x0F);
        self.emit_u8(0x90 | cc as u8);
        self.modrm_rr(0, dst as u8);
    }

    /// `movzx r64, r8` — Zero-extend byte to 64-bit.
    pub fn movzx_r64_r8(&mut self, dst: Reg, src: Reg) {
        self.rex_w();
        self.emit_u8(0x0F);
        self.emit_u8(0xB6);
        self.modrm_rr(dst as u8, src as u8);
    }

    /// `jmp rel32` — Unconditional jump with 32-bit relative offset.
    pub fn jmp_rel32(&mut self, offset: i32) {
        self.emit_u8(0xE9);
        self.emit_i32(offset);
    }

    /// `jcc rel32` — Conditional jump with 32-bit relative offset.
    pub fn jcc_rel32(&mut self, cc: CondCode, offset: i32) {
        self.emit_u8(0x0F);
        self.emit_u8(0x80 | cc as u8);
        self.emit_i32(offset);
    }

    /// `call rel32` — Call with 32-bit relative offset.
    pub fn call_rel32(&mut self, offset: i32) {
        self.emit_u8(0xE8);
        self.emit_i32(offset);
    }

    /// `call r64` — Call address in register.
    pub fn call_r64(&mut self, reg: Reg) {
        self.emit_u8(0xFF);
        self.modrm_rr(2, reg as u8); // /2 = call
    }

    /// `ret` — Return from function.
    pub fn ret(&mut self) {
        self.emit_u8(0xC3);
    }

    /// `syscall` — System call.
    pub fn syscall(&mut self) {
        self.emit_u8(0x0F);
        self.emit_u8(0x05);
    }

    /// `nop` — No operation (for alignment).
    pub fn nop(&mut self) {
        self.emit_u8(0x90);
    }

    /// `xor r, r` — Zero a register.
    pub fn xor_r64(&mut self, dst: Reg, src: Reg) {
        self.rex_w();
        self.emit_u8(0x31);
        self.modrm_rr(src as u8, dst as u8);
    }

    /// `or dst, src` — Bitwise OR.
    pub fn or_r64(&mut self, dst: Reg, src: Reg) {
        self.rex_w();
        self.emit_u8(0x09);
        self.modrm_rr(src as u8, dst as u8);
    }

    /// `lea dst, [rip + disp32]` — Load effective address (PC-relative).
    pub fn lea_rip(&mut self, dst: Reg, disp: i32) {
        self.rex_w();
        self.emit_u8(0x8D);
        // ModRM: mod=00, reg=dst, rm=101 (RIP-relative)
        self.emit_u8((dst.bits() << 3) | 0x05);
        self.emit_i32(disp);
    }

    /// `int3` — Debug breakpoint.
    pub fn int3(&mut self) {
        self.emit_u8(0xCC);
    }

    // ── Patching helpers ────────────────────────────────────────────

    /// Patch a 32-bit relative offset at the given position.
    /// Used to fix up forward jump targets after we know the destination.
    pub fn patch_i32(&mut self, pos: usize, value: i32) {
        let bytes = value.to_le_bytes();
        self.code[pos..pos + 4].copy_from_slice(&bytes);
    }

    /// Get current position for later patching.
    pub fn here(&self) -> usize {
        self.code.len()
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop() {
        let mut e = Encoder::new();
        e.push_r64(Reg::Rax);
        e.pop_r64(Reg::Rbx);
        assert_eq!(e.code, vec![0x50, 0x5B]);
    }

    #[test]
    fn test_push_r8() {
        let mut e = Encoder::new();
        e.push_r64(Reg::R8);
        assert_eq!(e.code, vec![0x41, 0x50]);
    }

    #[test]
    fn test_mov_imm64() {
        let mut e = Encoder::new();
        e.mov_r64_imm64(Reg::Rax, 42);
        assert_eq!(e.code[0], 0x48); // REX.W
        assert_eq!(e.code[1], 0xB8); // mov rax, imm64
        assert_eq!(i64::from_le_bytes(e.code[2..10].try_into().unwrap()), 42);
    }

    #[test]
    fn test_add() {
        let mut e = Encoder::new();
        e.add_r64(Reg::Rax, Reg::Rbx);
        assert_eq!(e.code, vec![0x48, 0x01, 0xD8]);
    }

    #[test]
    fn test_ret() {
        let mut e = Encoder::new();
        e.ret();
        assert_eq!(e.code, vec![0xC3]);
    }

    #[test]
    fn test_syscall() {
        let mut e = Encoder::new();
        e.syscall();
        assert_eq!(e.code, vec![0x0F, 0x05]);
    }
}
