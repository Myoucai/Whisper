//! Stack effect tracking for type checking.
//!
//! In Whisper, every operation has a stack effect: it consumes N values
//! from the top of the stack and produces M values. The type checker
//! verifies that stack effects are consistent throughout the program.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::types::Type;

/// Atomic counter for generating unique type variables in stack effects.
/// Starts at 1000 to avoid collision with TypeInferer::next_var (starts at 0).
static STACK_EFFECT_VAR_COUNTER: AtomicU64 = AtomicU64::new(1000);

fn fresh_stack_var() -> Type {
    let id = STACK_EFFECT_VAR_COUNTER.fetch_add(1, Ordering::Relaxed);
    Type::TypeVar(id)
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
    /// Effect of A then B: A's outputs become B's inputs.
    pub fn compose(&self, other: &StackEffect) -> Option<StackEffect> {
        Some(StackEffect {
            inputs: self.inputs.clone(),
            outputs: other.outputs.clone(),
        })
    }
}
