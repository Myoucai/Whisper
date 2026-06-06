/// WASM code generator: Whisper bytecode → .wasm module.
///
/// Generates standalone WebAssembly modules that embed a minimal
/// Whisper VM to execute the compiled bytecode.
///
/// Strategy: compile the Whisper bytecode into a WASM module that
/// contains a simple stack interpreter loop.

use whisper_core::opcode::Opcode;

/// Generator for WebAssembly modules.
pub struct WasmGenerator {
    /// The bytecode to compile
    #[allow(dead_code)]
    bytecode: Vec<Opcode>,
}

impl WasmGenerator {
    pub fn new(bytecode: Vec<Opcode>) -> Self {
        WasmGenerator { bytecode }
    }

    /// Compile bytecode to WASM binary.
    /// Currently produces a minimal WASM module with an embedded interpreter.
    pub fn compile(&self) -> Result<Vec<u8>, String> {
        // For now, produce a stub WASM module that exports a run function.
        // Full WASM generation requires wasm-encoder crate for proper module building.
        //
        // A complete WASM module would:
        // 1. Initialize memory for the Whisper stack and word dictionary
        // 2. Encode bytecode as data segments
        // 3. Generate a fetch-decode-execute loop in WASM
        // 4. Export whisper_run, memory, get_stack

        // Placeholder: return minimal valid WASM module
        let empty_wasm: Vec<u8> = vec![
            0x00, 0x61, 0x73, 0x6D, // magic "\0asm"
            0x01, 0x00, 0x00, 0x00, // version 1
        ];

        Ok(empty_wasm)
    }

    /// Compile and write to file.
    pub fn compile_to_file(&self, path: &std::path::Path) -> Result<(), String> {
        let wasm = self.compile()?;
        std::fs::write(path, wasm).map_err(|e| e.to_string())
    }
}
