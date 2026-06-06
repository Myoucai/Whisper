// Whisper Codegen - Code generation from AST to bytecode and WASM

pub mod bytecode_gen;
pub mod optimizer;
pub mod wasm_compiler;
pub mod wasm_gen;
mod wasm_utils;
pub mod wbin;

pub use bytecode_gen::BytecodeGenerator;
pub use optimizer::optimize;
pub use wasm_compiler::compile_direct;
pub use wasm_gen::WasmGenerator;
pub use wbin::WbinWriter;
