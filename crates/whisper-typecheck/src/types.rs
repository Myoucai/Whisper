/// Type definitions for the Whisper type system.
///
/// Whisper uses a global constraint-solving approach:
/// - Type variables are unified through Union-Find
/// - Stack effects are tracked for every operation
/// - Subtyping for Signal<T> <: T

/// A Whisper type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// 64-bit signed integer
    I64,
    /// 64-bit floating point
    F64,
    /// Boolean
    Bool,
    /// UTF-8 string (immutable)
    Str,
    /// Homogeneous list of type T
    List(Box<Type>),
    /// Quotation: input stack -> output stack
    Ref(Vec<Type>, Vec<Type>),
    /// Capability token
    Cap(u16),
    /// Signal with confidence: Signal(T)
    Signal(Box<Type>),
    /// Type variable for inference
    TypeVar(u64),
    /// Union type (for conditionals, etc.)
    Union(Box<Type>, Box<Type>),
}

impl Type {
    /// Return a human-readable name for the type.
    pub fn name(&self) -> String {
        match self {
            Type::I64 => "i64".into(),
            Type::F64 => "f64".into(),
            Type::Bool => "bool".into(),
            Type::Str => "str".into(),
            Type::List(t) => format!("[{}]", t.name()),
            Type::Ref(inputs, outputs) => {
                let in_str: Vec<_> = inputs.iter().map(|t| t.name()).collect();
                let out_str: Vec<_> = outputs.iter().map(|t| t.name()).collect();
                format!("[{}] → [{}]", in_str.join(" "), out_str.join(" "))
            }
            Type::Cap(id) => format!("cap({id})"),
            Type::Signal(t) => format!("signal({})", t.name()),
            Type::TypeVar(n) => format!("T{n}"),
            Type::Union(a, b) => format!("{} | {}", a.name(), b.name()),
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}
