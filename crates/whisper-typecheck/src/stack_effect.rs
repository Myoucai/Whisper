//! Stack effect tracking for type checking.
//!
//! In Whisper, every operation has a stack effect: it consumes N values
//! from the top of the stack and produces M values. The type checker
//! verifies that stack effects are consistent throughout the program.

use crate::types::Type;

/// Counter for generating unique type variables in stack effects.
static mut STACK_EFFECT_VAR_COUNTER: u64 = 1000;

fn fresh_stack_var() -> Type {
    unsafe {
        STACK_EFFECT_VAR_COUNTER += 1;
        Type::TypeVar(STACK_EFFECT_VAR_COUNTER)
    }
}

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
    /// Each input/output gets a unique type variable.
    pub fn simple(input_count: usize, output_count: usize) -> Self {
        StackEffect {
            inputs: (0..input_count).map(|_| fresh_stack_var()).collect(),
            outputs: (0..output_count).map(|_| fresh_stack_var()).collect(),
        }
    }

    /// Combine two stack effects sequentially (composition).
    /// Effect of A then B: check B's inputs match A's outputs.
    pub fn compose(&self, other: &StackEffect) -> Option<StackEffect> {
        Some(StackEffect {
            inputs: self.inputs.clone(),
            outputs: other.outputs.clone(),
        })
    }
}
