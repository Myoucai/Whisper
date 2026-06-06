// Whisper WASM Runtime Loader
// Loads the compiled WASM interpreter and provides executeWasm()

const WhisperWasm = (() => {
  let wasmInstance = null;
  let wasmMemory = null;
  let loaded = false;

  // WASM interpreter binary — pre-compiled from whisper-codegen wasm_gen.rs
  // This is a minimal interpreter WASM module with dynamic bytecode loading.
  // For production, this would be built via: cargo build --target wasm32-unknown-unknown
  const WASM_BASE64 = ''; // placeholder — set by build step

  async function loadWasm(wasmUrl) {
    if (loaded) return wasmInstance;

    try {
      let wasmBytes;
      if (wasmUrl) {
        const response = await fetch(wasmUrl);
        wasmBytes = await response.arrayBuffer();
      } else if (WASM_BASE64) {
        const binary = atob(WASM_BASE64);
        wasmBytes = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i++) wasmBytes[i] = binary.charCodeAt(i);
      } else {
        return null; // WASM module not available
      }

      const env = {
        memory: new WebAssembly.Memory({ initial: 1, maximum: 4 }),
        println: (ptr) => {
          if (wasmMemory) {
            const bytes = new Uint8Array(wasmMemory.buffer, ptr);
            let str = '';
            for (let i = 0; i < bytes.length && bytes[i] !== 0; i++) {
              str += String.fromCharCode(bytes[i]);
            }
            if (wasmInstance._output) wasmInstance._output.push(str);
          }
        },
      };

      const imports = { env };
      const module = await WebAssembly.instantiate(wasmBytes, imports);
      wasmInstance = module.instance.exports;
      wasmMemory = wasmInstance.memory || env.memory;
      wasmInstance._output = [];
      loaded = true;
      return wasmInstance;
    } catch (e) {
      console.warn('WASM runtime not available:', e.message);
      return null;
    }
  }

  async function executeWasm(bytecodes, wasmUrl) {
    const inst = await loadWasm(wasmUrl);
    if (!inst) return null;

    inst._output = [];

    // Write bytecodes to WASM memory at offset 0x0010
    const mem = new Uint8Array(wasmMemory.buffer);
    const bcOffset = 0x0010;

    // Encode bytecodes into WASM memory
    let pos = bcOffset;
    for (const bc of bytecodes) {
      if (pos >= 0x2000) break;
      mem[pos++] = bc;
    }

    // Set bytecode length at 0x0008
    const lenOffset = 0x0008;
    const len = pos - bcOffset;
    new DataView(wasmMemory.buffer).setUint32(lenOffset, len, true);
    // Reset stack pointer to 0x2000
    new DataView(wasmMemory.buffer).setUint32(0x0000, 0x2000, true);
    // Reset instruction pointer to 0
    new DataView(wasmMemory.buffer).setUint32(0x0004, 0, true);

    // Execute
    try {
      if (inst.whisper_run) {
        const result = inst.whisper_run();
        return {
          result,
          output: inst._output || [],
        };
      }
      if (inst.whisper_run_f64) {
        const result = inst.whisper_run_f64();
        return {
          result,
          output: inst._output || [],
        };
      }
    } catch (e) {
      console.warn('WASM execution error:', e.message);
    }

    return null;
  }

  function isAvailable() {
    return typeof WebAssembly !== 'undefined';
  }

  return { loadWasm, executeWasm, isAvailable };
})();

if (typeof module !== 'undefined') module.exports = WhisperWasm;
