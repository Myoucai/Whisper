//! C transpiler backend for Whisper.
//!
//! Translates Whisper bytecodes to C source code that can be compiled
//! with gcc/clang to produce native executables.
//!
//! Two implementations:
//! 1. `transpile_to_c` — Rust-based transpiler (complete, used for --target c)
//! 2. `transpile_via_whisper` — Whisper-based transpiler (self-hosting, used for --target c-whisper)

mod bridge;
mod transpiler;

pub use bridge::transpile_via_whisper;
pub use transpiler::transpile_to_c;
