// Whisper Type Checker - Static type inference and stack effect validation
//
// The checker uses the full Type system with a TypeInferer for
// constraint-solving across the entire program.

pub mod builtins;
pub mod checker;
pub mod infer;
pub mod stack_effect;
pub mod types;

pub use checker::TypeChecker;
pub use infer::TypeInferer;
pub use types::Type;
