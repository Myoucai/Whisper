//! Signal / Confidence type for native probability support.
//!
//! In Whisper, every value can carry a confidence score (0.0 to 1.0).
//! Operations on Signal values automatically propagate confidence:
//! - Arithmetic ops: confidence = product of input confidences
//! - Logic ops: confidence = product of input confidences
//! - Comparisons: confidence = product of input confidences

/// A value tagged with a confidence score.
///
/// This is the runtime representation of the `signal(T)` type.
/// Non-Signal values have implicit confidence of 1.0.
#[derive(Debug, Clone)]
pub struct Signal<T> {
    pub value: T,
    pub confidence: f64,
}

impl<T> Signal<T> {
    /// Create a new signal with the given confidence (clamped to [0.0, 1.0]).
    pub fn new(value: T, confidence: f64) -> Self {
        Signal {
            value,
            confidence: confidence.clamp(0.0, 1.0),
        }
    }

    /// Create a Signal with full confidence (1.0).
    pub fn certain(value: T) -> Self {
        Signal {
            value,
            confidence: 1.0,
        }
    }

    /// Combine two confidence values by multiplication (standard propagation).
    pub fn combine_confidence(c1: f64, c2: f64) -> f64 {
        (c1 * c2).clamp(0.0, 1.0)
    }
}

impl<T> std::ops::Deref for Signal<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
