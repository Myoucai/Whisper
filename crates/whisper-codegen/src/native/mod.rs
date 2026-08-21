//! Native code generation backend for Whisper.
//!
//! Translates Whisper bytecodes to x86-64 machine code and produces
//! Linux ELF executables.

pub mod x86;
pub mod elf;
pub mod codegen;

pub use codegen::compile_to_native;
