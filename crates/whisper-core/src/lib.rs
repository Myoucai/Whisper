// Whisper Core - Core data types and stack-based virtual machine
// Provides Value, Opcode, Vm, Signal, and Capability primitives

pub mod capability;
pub mod opcode;
pub mod signal;
pub mod value;
pub mod vm;

/// Common error type for whisper-core operations
pub type Result<T> = std::result::Result<T, VmError>;

#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error("Stack underflow: expected {expected} values, got {actual}")]
    StackUnderflow { expected: usize, actual: usize },

    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },

    #[error("Undefined word: {0}")]
    UndefinedWord(String),

    #[error("Capability not bound: @{0}")]
    CapabilityNotBound(u16),

    #[error("Capability denied: {0}")]
    CapabilityDenied(String),

    #[error("Division by zero")]
    DivisionByZero,

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Program error: {0}")]
    ProgramError(String),
}
