// Whisper Codegen - Code generation from AST to bytecode and native ELF

pub mod bytecode_gen;
pub mod cgen;
pub mod formatter;
pub mod native_gen;
pub mod optimizer;
pub mod wbin;

pub use bytecode_gen::BytecodeGenerator;
pub use cgen::transpile_to_c;
pub use cgen::transpile_via_whisper;
pub use formatter::format_ast;
pub use native_gen::compile_to_native;
pub use optimizer::optimize;
pub use wbin::WbinWriter;
