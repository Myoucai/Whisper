// Whisper Type Checker - Static type inference and stack effect validation

pub mod builtins;
pub mod checker;
pub mod infer;
pub mod stack_effect;
pub mod types;

pub use checker::TypeChecker;
pub use types::Type;
