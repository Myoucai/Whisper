// Whisper Codegen — code generation from AST to bytecode, .wbin and C

pub mod bytecode_gen;
pub mod block_ir;
pub mod stack_effect;
pub mod block_gen;
pub mod cgen;
pub mod formatter;
pub mod optimizer;
pub mod wbin;

pub use bytecode_gen::BytecodeGenerator;
pub use cgen::transpile_to_c;
pub use formatter::format_ast;
pub use optimizer::optimize;
pub use wbin::WbinWriter;
