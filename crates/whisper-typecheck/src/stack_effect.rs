/// Stack effect tracking for type checking.
///
/// In Whisper, every operation has a stack effect: it consumes N values
/// from the top of the stack and produces M values. The type checker
/// verifies that stack effects are consistent throughout the program.

use crate::types::Type;

/// Represents a stack effect: inputs (consumed) and outputs (produced).
#[derive(Debug, Clone)]
pub struct StackEffect {
    pub inputs: Vec<Type>,
    pub outputs: Vec<Type>,
}

impl StackEffect {
    pub fn new(inputs: Vec<Type>, outputs: Vec<Type>) -> Self {
        StackEffect { inputs, outputs }
    }

    /// Create a stack effect from a simple function type.
    /// e.g., [i64 i64] → [i64] for addition
    pub fn simple(input_count: usize, output_count: usize) -> Self {
        StackEffect {
            inputs: vec![Type::TypeVar(0); input_count],
            outputs: vec![Type::TypeVar(0); output_count],
        }
    }

    /// Combine two stack effects sequentially (composition).
    /// Effect of A then B: check B's inputs match A's outputs.
    pub fn compose(&self, other: &StackEffect) -> Option<StackEffect> {
        // The outputs of self become the inputs of other
        // For now, just concatenate effects
        Some(StackEffect {
            inputs: self.inputs.clone(),
            outputs: other.outputs.clone(),
        })
    }
}
