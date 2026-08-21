//! Native code generator: Opcode → x86-64 machine code.
//!
//! Strategy: The Whisper VM is stack-based. We map the VM data stack
//! directly to the CPU stack. Each PushI64 becomes `push imm`, each
//! Add becomes `pop rbx; pop rax; add rax, rbx; push rax`, etc.
//!
//! For complex operations (string, list, map, IO), we call runtime
//! functions via `call <address>`. The runtime is a small C library
//! linked into the final executable.
//!
//! Function calling convention:
//! - Each Whisper word `: name { body } ;` compiles to a function `w_name`
//! - Arguments are passed on the CPU stack (same as the data stack)
//! - Return value is left on the stack
//! - Call/Return use x86 `call`/`ret`

use std::collections::HashMap;
use std::rc::Rc;

use crate::native::x86::{Encoder, Reg, CondCode};
use crate::native::elf;
use crate::opcode::Opcode;

/// Runtime function IDs for complex operations.
/// These are called via `call <address>` where the address is patched
/// after linking with the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeFunc {
    PrintI64,
    PrintF64,
    PrintBool,
    PrintStr,
    StrCat,
    StrLen,
    StrSlice,
    StrEq,
    StrLt,
    StrFind,
    StrReplace,
    StrToI64,
    I64ToStr,
    StrNth,
    StrChars,
    CharsStr,
    StrJoin,
    StrSplit,
    Ctos,
    ListNew,
    ListAppend,
    ListLen,
    ListNth,
    ListSlice,
    MapNew,
    MapGet,
    MapSet,
    MapDel,
    MapKeys,
    MapHas,
    Now,
    SleepMs,
    RandomF64,
    RandInt,
    JsonParse,
    JsonStringify,
    Alloc,
    Dealloc,
}

/// Collects string constants during code generation.
/// Each unique string gets a label for the .rodata section.
pub struct StringPool {
    strings: Vec<String>,
    indices: HashMap<String, usize>,
}

impl StringPool {
    pub fn new() -> Self {
        StringPool {
            strings: Vec::new(),
            indices: HashMap::new(),
        }
    }

    /// Add a string constant and return its index.
    pub fn intern(&mut self, s: &str) -> usize {
        if let Some(&idx) = self.indices.get(s) {
            return idx;
        }
        let idx = self.strings.len();
        self.strings.push(s.to_string());
        self.indices.insert(s.to_string(), idx);
        idx
    }

    /// Build the .rodata section: all strings null-terminated.
    pub fn build_rodata(&self) -> Vec<u8> {
        let mut data = Vec::new();
        for s in &self.strings {
            data.extend_from_slice(s.as_bytes());
            data.push(0); // null terminator
        }
        data
    }

    /// Get the byte offset of a string by its index.
    pub fn offset_of(&self, idx: usize) -> usize {
        let mut off = 0;
        for i in 0..idx {
            off += self.strings[i].len() + 1; // +1 for null terminator
        }
        off
    }
}

/// Native code generator.
///
/// Takes a list of Opcodes and produces x86-64 machine code.
/// Handles word definitions by compiling each into a separate function.
pub struct NativeCodeGen {
    /// String pool for string constants.
    pub strings: StringPool,
    /// Word definitions: name → compiled machine code.
    pub word_code: HashMap<String, Vec<u8>>,
    /// Main program code.
    pub main_code: Vec<u8>,
    /// Data section (runtime state: stack memory, etc.).
    pub data: Vec<u8>,
    /// Forward references to fix up (word name → list of patch positions).
    forward_refs: HashMap<String, Vec<usize>>,
    /// Current code buffer being written to.
    encoder: Encoder,
    /// Runtime function table: func → address in .text.
    /// Filled after all code is generated.
    runtime_addrs: HashMap<RuntimeFunc, u64>,
}

impl NativeCodeGen {
    pub fn new() -> Self {
        NativeCodeGen {
            strings: StringPool::new(),
            word_code: HashMap::new(),
            main_code: Vec::new(),
            data: Vec::new(),
            forward_refs: HashMap::new(),
            encoder: Encoder::new(),
            runtime_addrs: HashMap::new(),
        }
    }

    /// Compile a list of Opcodes into x86-64 machine code.
    ///
    /// Returns (main_code, word_definitions, string_pool, data_section).
    pub fn compile(
        &mut self,
        opcodes: &[Opcode],
    ) -> Result<(Vec<u8>, HashMap<String, Vec<u8>>, Vec<u8>, Vec<u8>), String> {
        // First pass: collect word definitions
        let mut defs: HashMap<String, Vec<Opcode>> = HashMap::new();
        let mut main_ops: Vec<Opcode> = Vec::new();

        for op in opcodes {
            match op {
                Opcode::DefWord(name) => {
                    // Start collecting a definition
                    let _ = defs.entry(name.clone()).or_insert_with(Vec::new);
                }
                Opcode::EndDef => {
                    // End current definition — nothing to do here
                }
                _ => {
                    // Check if we're inside a definition
                    // For now, we'll handle this differently — compile flat
                    main_ops.push(op.clone());
                }
            }
        }

        // For the first version, compile everything as main code
        // (word definitions will be handled in a later iteration)
        self.encoder = Encoder::new();
        self.emit_prologue();
        for op in opcodes {
            self.emit_opcode(op)?;
        }
        self.emit_epilogue();

        let main_code = std::mem::take(&mut self.encoder.code);
        let word_code = std::mem::take(&mut self.word_code);
        let rodata = self.strings.build_rodata();
        let data = std::mem::take(&mut self.data);

        Ok((main_code, word_code, rodata, data))
    }

    /// Emit function prologue: save frame pointer, set up stack.
    fn emit_prologue(&mut self) {
        let e = &mut self.encoder;
        // push rbp
        e.push_r64(Reg::Rbp);
        // mov rbp, rsp
        e.mov_r64_r64(Reg::Rbp, Reg::Rsp);
    }

    /// Emit function epilogue: restore frame, return.
    fn emit_epilogue(&mut self) {
        let e = &mut self.encoder;
        // If there's a value on the stack, pop it into rax for return
        // (for main, this is the exit status)
        e.pop_r64(Reg::Rax);
        // mov rsp, rbp
        e.mov_r64_r64(Reg::Rsp, Reg::Rbp);
        // pop rbp
        e.pop_r64(Reg::Rbp);
        // ret
        e.ret();
    }

    /// Emit code for a single Opcode.
    fn emit_opcode(&mut self, op: &Opcode) -> Result<(), String> {
        let e = &mut self.encoder;
        match op {
            // ── Literals ────────────────────────────────────────
            Opcode::PushI64(n) => {
                e.mov_r64_imm64(Reg::Rax, *n);
                e.push_r64(Reg::Rax);
            }
            Opcode::PushF64(n) => {
                let bits = n.to_bits() as i64;
                e.mov_r64_imm64(Reg::Rax, bits);
                e.push_r64(Reg::Rax);
            }
            Opcode::PushBool(b) => {
                e.mov_r64_imm64(Reg::Rax, if *b { 1 } else { 0 });
                e.push_r64(Reg::Rax);
            }
            Opcode::PushStr(s) => {
                let idx = self.strings.intern(s);
                // Will be patched with actual .rodata offset
                // For now, push the string index (resolved at link time)
                e.mov_r64_imm64(Reg::Rax, idx as i64);
                e.push_r64(Reg::Rax);
            }
            Opcode::PushList => {
                // Pop count, then that many elements, create list
                // For now, just pop the count (list construction via runtime)
                e.pop_r64(Reg::Rdi);
                e.call_rel32(0); // placeholder — patched to runtime
            }
            Opcode::PushRef(_code) => {
                // Quote block — for now, push a placeholder
                e.xor_r64(Reg::Rax, Reg::Rax);
                e.push_r64(Reg::Rax);
            }

            // ── Stack operations ────────────────────────────────
            Opcode::Dup => {
                e.pop_r64(Reg::Rax);
                e.push_r64(Reg::Rax);
                e.push_r64(Reg::Rax);
            }
            Opcode::Swap => {
                e.pop_r64(Reg::Rax);
                e.pop_r64(Reg::Rbx);
                e.push_r64(Reg::Rax);
                e.push_r64(Reg::Rbx);
            }
            Opcode::Drop => {
                e.pop_r64(Reg::Rax);
            }
            Opcode::Rot => {
                // a b c → b c a
                e.pop_r64(Reg::Rax);  // c
                e.pop_r64(Reg::Rbx);  // b
                e.pop_r64(Reg::Rcx);  // a
                e.push_r64(Reg::Rbx); // b
                e.push_r64(Reg::Rax); // c
                e.push_r64(Reg::Rcx); // a
            }
            Opcode::Pick(n) => {
                let offset = (*n as i32 + 1) * 8;
                e.mov_load_disp(Reg::Rax, Reg::Rsp, offset);
                e.push_r64(Reg::Rax);
            }

            // ── Arithmetic ──────────────────────────────────────
            Opcode::Add => {
                e.pop_r64(Reg::Rbx);
                e.pop_r64(Reg::Rax);
                e.add_r64(Reg::Rax, Reg::Rbx);
                e.push_r64(Reg::Rax);
            }
            Opcode::Sub => {
                e.pop_r64(Reg::Rbx); // b (subtracted)
                e.pop_r64(Reg::Rax); // a
                e.sub_r64(Reg::Rax, Reg::Rbx);
                e.push_r64(Reg::Rax);
            }
            Opcode::Mul => {
                e.pop_r64(Reg::Rbx);
                e.pop_r64(Reg::Rax);
                e.imul_r64(Reg::Rax, Reg::Rbx);
                e.push_r64(Reg::Rax);
            }
            Opcode::Div => {
                e.pop_r64(Reg::Rbx); // divisor
                e.pop_r64(Reg::Rax); // dividend
                e.cqo();             // sign-extend RAX → RDX:RAX
                e.idiv_r64(Reg::Rbx);
                e.push_r64(Reg::Rax); // quotient
            }
            Opcode::Mod => {
                e.pop_r64(Reg::Rbx);
                e.pop_r64(Reg::Rax);
                e.cqo();
                e.idiv_r64(Reg::Rbx);
                e.push_r64(Reg::Rdx); // remainder
            }

            // ── Comparison ──────────────────────────────────────
            Opcode::Eq => {
                e.pop_r64(Reg::Rbx);
                e.pop_r64(Reg::Rax);
                e.cmp_r64(Reg::Rax, Reg::Rbx);
                e.setcc(CondCode::E, Reg::Al);
                e.movzx_r64_r8(Reg::Rax, Reg::Al);
                e.push_r64(Reg::Rax);
            }
            Opcode::Lt => {
                e.pop_r64(Reg::Rbx);
                e.pop_r64(Reg::Rax);
                e.cmp_r64(Reg::Rax, Reg::Rbx);
                e.setcc(CondCode::L, Reg::Al);
                e.movzx_r64_r8(Reg::Rax, Reg::Al);
                e.push_r64(Reg::Rax);
            }
            Opcode::Gt => {
                e.pop_r64(Reg::Rbx);
                e.pop_r64(Reg::Rax);
                e.cmp_r64(Reg::Rax, Reg::Rbx);
                e.setcc(CondCode::G, Reg::Al);
                e.movzx_r64_r8(Reg::Rax, Reg::Al);
                e.push_r64(Reg::Rax);
            }
            Opcode::Neq => {
                e.pop_r64(Reg::Rbx);
                e.pop_r64(Reg::Rax);
                e.cmp_r64(Reg::Rax, Reg::Rbx);
                e.setcc(CondCode::Ne, Reg::Al);
                e.movzx_r64_r8(Reg::Rax, Reg::Al);
                e.push_r64(Reg::Rax);
            }
            Opcode::Le => {
                e.pop_r64(Reg::Rbx);
                e.pop_r64(Reg::Rax);
                e.cmp_r64(Reg::Rax, Reg::Rbx);
                e.setcc(CondCode::Le, Reg::Al);
                e.movzx_r64_r8(Reg::Rax, Reg::Al);
                e.push_r64(Reg::Rax);
            }
            Opcode::Ge => {
                e.pop_r64(Reg::Rbx);
                e.pop_r64(Reg::Rax);
                e.cmp_r64(Reg::Rax, Reg::Rbx);
                e.setcc(CondCode::Ge, Reg::Al);
                e.movzx_r64_r8(Reg::Rax, Reg::Al);
                e.push_r64(Reg::Rax);
            }

            // ── Logic ───────────────────────────────────────────
            Opcode::And => {
                e.pop_r64(Reg::Rbx);
                e.pop_r64(Reg::Rax);
                // Both must be non-zero
                e.test_r64(Reg::Rax, Reg::Rax);
                let jmp1 = e.position();
                e.jcc_rel32(CondCode::E, 0); // if a==0, result is 0
                let patch1 = jmp1 + 2;
                e.test_r64(Reg::Rbx, Reg::Rbx);
                e.setcc(CondCode::Ne, Reg::Al);
                e.movzx_r64_r8(Reg::Rax, Reg::Al);
                let jmp2 = e.position();
                e.jmp_rel32(0);
                let patch2 = jmp2 + 1;
                // a==0 path
                let here = e.position();
                e.patch_i32(patch1, (here as i32) - (patch1 as i32) - 4);
                e.xor_r64(Reg::Rax, Reg::Rax);
                // merge
                let here2 = e.position();
                e.patch_i32(patch2, (here2 as i32) - (patch2 as i32) - 4);
                e.push_r64(Reg::Rax);
            }
            Opcode::Or => {
                e.pop_r64(Reg::Rbx);
                e.pop_r64(Reg::Rax);
                e.or_r64(Reg::Rax, Reg::Rbx);
                e.setcc(CondCode::Ne, Reg::Al);
                e.movzx_r64_r8(Reg::Rax, Reg::Al);
                e.push_r64(Reg::Rax);
            }
            Opcode::Not => {
                e.pop_r64(Reg::Rax);
                e.test_r64(Reg::Rax, Reg::Rax);
                e.setcc(CondCode::E, Reg::Al);
                e.movzx_r64_r8(Reg::Rax, Reg::Al);
                e.push_r64(Reg::Rax);
            }

            // ── Control flow ────────────────────────────────────
            Opcode::Cond(offset) => {
                e.pop_r64(Reg::Rax);
                e.test_r64(Reg::Rax, Reg::Rax);
                e.jcc_rel32(CondCode::E, *offset as i32);
            }
            Opcode::Jump(offset) => {
                e.jmp_rel32(*offset as i32);
            }
            Opcode::Loop(offset) => {
                e.pop_r64(Reg::Rax);
                e.test_r64(Reg::Rax, Reg::Rax);
                e.jcc_rel32(CondCode::Ne, *offset as i32);
            }

            // ── IO ──────────────────────────────────────────────
            Opcode::OutputTop => {
                e.pop_r64(Reg::Rdi);
                e.call_rel32(0); // placeholder for runtime_print_i64
            }
            Opcode::OutputAll => {
                // Print entire stack — complex, placeholder
            }
            Opcode::ReadInput => {
                e.call_rel32(0); // placeholder for runtime_read_line
            }

            // ── Unsupported — emit trap ─────────────────────────
            _ => {
                // For unsupported opcodes, emit int3 (trap)
                e.int3();
            }
        }
        Ok(())
    }

    /// Build a complete ELF executable from the compiled code.
    pub fn build_executable(
        &self,
        main_code: &[u8],
        _word_code: &HashMap<String, Vec<u8>>,
        rodata: &[u8],
        data: &[u8],
    ) -> Vec<u8> {
        // For now, just build the ELF with main code
        elf::build_elf64(main_code, rodata, data, 0)
    }
}

impl Default for NativeCodeGen {
    fn default() -> Self {
        Self::new()
    }
}

/// High-level entry point: compile Opcodes to a native ELF executable.
pub fn compile_to_native(opcodes: &[Opcode]) -> Result<Vec<u8>, String> {
    let mut gen = NativeCodeGen::new();
    let (main_code, word_code, rodata, data) = gen.compile(opcodes)?;
    Ok(gen.build_executable(&main_code, &word_code, &rodata, &data))
}
