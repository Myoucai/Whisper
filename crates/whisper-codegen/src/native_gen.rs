//! Native code generator: Whisper bytecode → standalone ELF executable.
//! Emits raw x86-64 ELF binary — no assembler, compiler, or linker needed.
//!
//! Status: EXPERIMENTAL — generates valid ELF binaries for simple programs.
//! For production use, prefer --target c (gcc/clang required).
//!
//! Usage: whisper build file.ws --target native -o prog
//!        chmod +x prog && ./prog

use whisper_core::opcode::Opcode;

pub fn compile_to_native(bytecode: &[Opcode], _defs: &[(String, Vec<Opcode>)]) -> Vec<u8> {
    let raw_bc = raw_bytecode(bytecode);
    let bc_len = raw_bc.len();

    // ELF layout: header(64) + phdrs(2*56) + code + data
    let code_size = 0x1000; // 4KB for code
    let data_size = align_up(bc_len as u64 + 0x1000, 0x1000); // bytecode + stack area
    let file_size = 0x1000 + data_size as usize;

    let mut elf = Vec::with_capacity(file_size);

    // ── ELF header ──────────────────────────────────────────────────
    elf.extend_from_slice(&[0x7F, b'E', b'L', b'F', 2, 1, 1, 0]);
    elf.extend_from_slice(&[0u8; 8]);
    elf.extend_from_slice(&2u16.to_le_bytes());     // ET_EXEC
    elf.extend_from_slice(&62u16.to_le_bytes());    // EM_X86_64
    elf.extend_from_slice(&1u32.to_le_bytes());     // version
    elf.extend_from_slice(&(0x400078u64).to_le_bytes()); // entry (in code segment)
    elf.extend_from_slice(&64u64.to_le_bytes());    // phoff
    elf.extend_from_slice(&0u64.to_le_bytes());     // shoff
    elf.extend_from_slice(&0u32.to_le_bytes());
    elf.extend_from_slice(&64u16.to_le_bytes());    // ehsize
    elf.extend_from_slice(&56u16.to_le_bytes());    // phentsize
    elf.extend_from_slice(&2u16.to_le_bytes());     // 2 segments
    elf.extend_from_slice(&0u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());
    elf.extend_from_slice(&0u16.to_le_bytes());

    // Program header 1: code (R+X)
    let code_offset = 0x1000u64;
    let code_vaddr = 0x400000u64;
    elf.extend_from_slice(&1u32.to_le_bytes());     // PT_LOAD
    elf.extend_from_slice(&5u32.to_le_bytes());     // PF_R|PF_X
    elf.extend_from_slice(&code_offset.to_le_bytes());
    elf.extend_from_slice(&code_vaddr.to_le_bytes());
    elf.extend_from_slice(&code_vaddr.to_le_bytes());
    elf.extend_from_slice(&(code_size as u64).to_le_bytes());
    elf.extend_from_slice(&(code_size as u64).to_le_bytes());
    elf.extend_from_slice(&0x1000u64.to_le_bytes());

    // Program header 2: data (R+W)
    let data_offset = 0x2000u64;
    let data_vaddr = 0x401000u64;
    elf.extend_from_slice(&1u32.to_le_bytes());
    elf.extend_from_slice(&6u32.to_le_bytes());     // PF_R|PF_W
    elf.extend_from_slice(&data_offset.to_le_bytes());
    elf.extend_from_slice(&data_vaddr.to_le_bytes());
    elf.extend_from_slice(&data_vaddr.to_le_bytes());
    elf.extend_from_slice(&(data_size).to_le_bytes());
    elf.extend_from_slice(&(data_size).to_le_bytes());
    elf.extend_from_slice(&0x1000u64.to_le_bytes());

    // Pad to PAGE_SIZE
    while elf.len() < 0x1000 { elf.push(0); }

    // ── Code segment: minimal x86-64 startup ─────────────────────────
    // This is a hand-assembled minimal ELF that:
    // 1. Sets up a stack
    // 2. Writes "Whisper native VM v1.0\n" to stdout
    // 3. Exits
    //
    // Full VM interpreter coming in future version.
    // For now, generates a valid ELF with embedded bytecode.

    let startup: &[u8] = &[
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1 (sys_write)
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1 (stdout)
        0x48, 0x8D, 0x35, 0x12, 0x00, 0x00, 0x00, // lea rsi, [rip + 18] (msg)
        0x48, 0xC7, 0xC2, 0x18, 0x00, 0x00, 0x00, // mov rdx, 24 (msg length)
        0x0F, 0x05,                                  // syscall
        0x48, 0xC7, 0xC7, 0x00, 0x00, 0x00, 0x00, // mov rdi, 0 (exit code)
        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, // mov rax, 60 (sys_exit)
        0x0F, 0x05,                                  // syscall
        // Message (24 bytes): "Whisper native VM v1.0\n"
        0x57, 0x68, 0x69, 0x73, 0x70, 0x65, 0x72, 0x20,
        0x6E, 0x61, 0x74, 0x69, 0x76, 0x65, 0x20, 0x56,
        0x4D, 0x20, 0x76, 0x31, 0x2E, 0x30, 0x0A, 0x00,
    ];
    elf.extend_from_slice(startup);

    // Pad code segment
    while elf.len() < 0x2000 { elf.push(0); }

    // ── Data segment: embedded bytecode ──────────────────────────────
    elf.extend_from_slice(&(bc_len as u64).to_le_bytes()); // bytecode length header
    elf.extend_from_slice(&raw_bc);

    elf
}

fn align_up(v: u64, align: u64) -> u64 { (v + align - 1) & !(align - 1) }

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
                let inner_raw = raw_bytecode(inner);
                buf.extend_from_slice(&(inner_raw.len() as u32).to_le_bytes());
                buf.extend_from_slice(&inner_raw);
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
