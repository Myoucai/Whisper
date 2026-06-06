// Whisper Type Checker - Static type inference and stack effect validation

pub mod builtins;
pub mod infer;
pub mod stack_effect;
pub mod types;

pub use infer::TypeChecker;
pub use types::Type;
