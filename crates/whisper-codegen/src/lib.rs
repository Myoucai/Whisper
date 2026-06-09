// Whisper Codegen - Code generation from AST to bytecode and native ELF

pub mod bytecode_gen;
pub mod formatter;
pub mod native_gen;
pub mod optimizer;
pub mod wbin;

pub use bytecode_gen::BytecodeGenerator;
pub use formatter::format_ast;
pub use native_gen::compile_to_native;
pub use optimizer::optimize;
pub use wbin::WbinWriter;
