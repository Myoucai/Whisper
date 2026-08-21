//! C transpiler backend for Whisper.
//!
//! Translates Whisper bytecodes to C source code that can be compiled
//! with gcc/clang to produce native executables. The self-hosted
//! compiler chain (whisperc/) emits C directly and lives outside this
//! crate — see whisperc/seed/ and scripts/verify-fixedpoint.sh.

mod transpiler;

pub use transpiler::transpile_to_c;
