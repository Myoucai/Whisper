//! Minimal ELF64 writer for Linux x86-64 executables.
//!
//! Generates a statically-linked ELF executable with:
//! - .text section (machine code)
//! - .rodata section (string constants)
//! - Simple LOAD segment mapping

/// ELF64 header (64 bytes).
/// Offsets are from the start of the file.
struct ElfHeader {
    /// Entry point virtual address.
    e_entry: u64,
    /// Program header table file offset.
    e_phoff: u64,
    /// Number of program header entries.
    e_phnum: u16,
}

/// ELF64 program header (56 bytes).
/// Describes a loadable segment.
struct ProgramHeader {
    /// File offset of the segment.
    p_offset: u64,
    /// Virtual address of the segment.
    p_vaddr: u64,
    /// File size of the segment.
    p_filesz: u64,
    /// Memory size of the segment.
    p_memsz: u64,
    /// Flags: 5 = READ | EXEC, 6 = READ | WRITE
    p_flags: u32,
}

/// Build a minimal ELF64 executable.
///
/// `code` — machine code bytes (.text section).
/// `rodata` — read-only data bytes (.rodata section, string constants).
/// `data` — read-write data bytes (.data section, runtime state).
/// `entry_offset` — offset of entry point within .text.
///
/// Returns the complete ELF file as a byte vector.
pub fn build_elf64(code: &[u8], rodata: &[u8], data: &[u8], entry_offset: u64) -> Vec<u8> {
    // Layout:
    //   0x0000: ELF header (64 bytes)
    //   0x0040: Program header 0 — LOAD (code+rodata+data) (56 bytes)
    //   0x0078: .text (code)
    //   0x0078+code.len(): .rodata
    //   0x0078+code.len()+rodata.len(): .data (8-byte aligned)
    //
    // Virtual address base: 0x400000

    const BASE_ADDR: u64 = 0x40_0000;
    const ELF_HEADER_SIZE: usize = 64;
    const PH_SIZE: usize = 56;
    const PH_OFFSET: usize = ELF_HEADER_SIZE; // right after ELF header

    let header_end = ELF_HEADER_SIZE + PH_SIZE;
    let code_offset = align_up(header_end, 16); // align code to 16 bytes
    let rodata_offset = code_offset + code.len();
    let data_offset = align_up(rodata_offset + rodata.len(), 8);

    let total_file_size = data_offset + data.len();
    let total_memsz = align_up(total_file_size, 4096);

    let entry_addr = BASE_ADDR + code_offset as u64 + entry_offset;

    let mut buf = Vec::with_capacity(total_file_size);

    // ── ELF Header ──────────────────────────────────────────────
    // e_ident
    buf.extend_from_slice(&[
        0x7F, b'E', b'L', b'F', // magic
        2,                       // EI_CLASS: ELFCLASS64
        1,                       // EI_DATA: little-endian
        1,                       // EI_VERSION: EV_CURRENT
        0,                       // EI_OSABI: ELFOSABI_NONE
        0, 0, 0, 0, 0, 0, 0, 0, // EI_ABIVERSION + padding
    ]);
    buf.extend_from_slice(&2u16.to_le_bytes());   // e_type: ET_EXEC
    buf.extend_from_slice(&0x3Eu16.to_le_bytes()); // e_machine: EM_X86_64
    buf.extend_from_slice(&1u32.to_le_bytes());    // e_version: EV_CURRENT
    buf.extend_from_slice(&entry_addr.to_le_bytes()); // e_entry
    buf.extend_from_slice(&(PH_OFFSET as u64).to_le_bytes()); // e_phoff
    buf.extend_from_slice(&0u64.to_le_bytes());    // e_shoff (no section headers)
    buf.extend_from_slice(&0u32.to_le_bytes());    // e_flags
    buf.extend_from_slice(&(ELF_HEADER_SIZE as u16).to_le_bytes()); // e_ehsize
    buf.extend_from_slice(&(PH_SIZE as u16).to_le_bytes()); // e_phentsize
    buf.extend_from_slice(&1u16.to_le_bytes());    // e_phnum
    buf.extend_from_slice(&0u16.to_le_bytes());    // e_shentsize
    buf.extend_from_slice(&0u16.to_le_bytes());    // e_shnum
    buf.extend_from_slice(&0u16.to_le_bytes());    // e_shstrndx

    assert_eq!(buf.len(), ELF_HEADER_SIZE);

    // ── Program Header ──────────────────────────────────────────
    buf.extend_from_slice(&1u32.to_le_bytes());    // p_type: PT_LOAD
    buf.extend_from_slice(&5u32.to_le_bytes());    // p_flags: PF_R | PF_X
    buf.extend_from_slice(&(code_offset as u64).to_le_bytes()); // p_offset
    buf.extend_from_slice(&(BASE_ADDR + code_offset as u64).to_le_bytes()); // p_vaddr
    buf.extend_from_slice(&(BASE_ADDR + code_offset as u64).to_le_bytes()); // p_paddr
    buf.extend_from_slice(&((total_file_size - code_offset) as u64).to_le_bytes()); // p_filesz
    buf.extend_from_slice(&total_memsz.saturating_sub(code_offset as u64).to_le_bytes()); // p_memsz
    buf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_align

    assert_eq!(buf.len(), PH_OFFSET + PH_SIZE);

    // ── Padding to code start ───────────────────────────────────
    while buf.len() < code_offset {
        buf.push(0);
    }

    // ── .text section �machine code) ─────────────────────────────
    buf.extend_from_slice(code);

    // ── .rodata section (string constants) ──────────────────────
    buf.extend_from_slice(rodata);

    // ── Padding to data start ───────────────────────────────────
    while buf.len() < data_offset {
        buf.push(0);
    }

    // ── .data section (runtime state) ───────────────────────────
    buf.extend_from_slice(data);

    // ── Pad to page boundary ────────────────────────────────────
    while buf.len() < total_file_size {
        buf.push(0);
    }

    buf
}

/// Align `v` up to the nearest multiple of `align`.
fn align_up(v: usize, align: usize) -> usize {
    (v + align - 1) & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elf_header_magic() {
        let code = vec![0xC3]; // ret
        let elf = build_elf64(&code, &[], &[], 0);
        assert_eq!(&elf[0..4], &[0x7F, b'E', b'L', b'F']);
    }

    #[test]
    fn test_elf64_executable() {
        // mov rax, 60 (exit); xor rdi, rdi; syscall
        let code = vec![
            0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, // mov rax, 60
            0x48, 0x31, 0xFF,                            // xor rdi, rdi
            0x0F, 0x05,                                   // syscall
        ];
        let elf = build_elf64(&code, &[], &[], 0);
        // Verify it starts with ELF magic
        assert_eq!(&elf[0..4], &[0x7F, b'E', b'L', b'F']);
        // Verify it's ET_EXEC
        assert_eq!(u16::from_le_bytes([elf[16], elf[17]]), 2);
        // Verify x86-64
        assert_eq!(u16::from_le_bytes([elf[18], elf[19]]), 0x3E);
    }
}
