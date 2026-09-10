/// Value type for the Whisper VM.
/// All data on the stack is represented as a Value.
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::opcode::Opcode;

#[derive(Clone, PartialEq)]
pub enum Value {
    /// 64-bit signed integer (default integer type)
    I64(i64),
    /// 64-bit floating point (IEEE 754)
    F64(f64),
    /// Boolean value
    Bool(bool),
    /// UTF-8 immutable string (reference-counted for cheap dup)
    Str(Rc<String>),
    /// Homogeneous list (reference-counted)
    List(Rc<Vec<Value>>),
    /// Quotation block - delayed execution bytecode
    Ref(Rc<[Opcode]>),
    /// Capability token (id, description) - type-safe, cannot mix with data
    Cap(u16, String),
    /// Signal with confidence tensor: (value, confidence 0.0-1.0)
    Signal(Box<Value>, f64),
    /// String-keyed map (reference-counted for cheap clone)
    Map(Rc<BTreeMap<String, Value>>),
}

impl Value {
    /// Return the type name as a string for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::I64(_) => "i64",
            Value::F64(_) => "f64",
            Value::Bool(_) => "bool",
            Value::Str(_) => "str",
            Value::List(_) => "[T]",
            Value::Ref(_) => "ref",
            Value::Cap(_, _) => "cap",
            Value::Signal(v, _) => v.type_name(), // transparent for type checks
            Value::Map(_) => "map",
        }
    }

    /// Extract the inner value, discarding Signal wrapper if present.
    pub fn unwrap_signal(self) -> Value {
        match self {
            Value::Signal(v, _) => *v,
            other => other,
        }
    }

    /// Get the confidence value, defaulting to 1.0 for non-Signal values.
    pub fn confidence(&self) -> f64 {
        match self {
            Value::Signal(_, c) => *c,
            _ => 1.0,
        }
    }

    /// Wrap a value with a confidence score.
    pub fn with_confidence(val: Value, conf: f64) -> Value {
        Value::Signal(Box::new(val), conf.clamp(0.0, 1.0))
    }

    /// Compare two values for equality (for Eq opcode).
    /// Lists and Maps compare structurally (element-wise); Ref blocks never
    /// compare equal (code identity, not value identity).
    pub fn equals(&self, other: &Value) -> bool {
        match (self.unwrap_signal_ref(), other.unwrap_signal_ref()) {
            (Value::I64(a), Value::I64(b)) => a == b,
            (Value::F64(a), Value::F64(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            // N-1: 跨标签按数值比（C 链 bool 即 i64，#t 1 = → 1）
            (Value::Bool(a), Value::I64(b)) => (*a as i64) == *b,
            (Value::I64(a), Value::Bool(b)) => *a == (*b as i64),
            (Value::Str(a), Value::Str(b)) => Rc::ptr_eq(a, b) || a == b,
            (Value::List(a), Value::List(b)) => {
                Rc::ptr_eq(a, b)
                    || (a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y)))
            }
            (Value::Map(a), Value::Map(b)) => {
                Rc::ptr_eq(a, b)
                    || (a.len() == b.len()
                        && a.iter()
                            .all(|(k, v)| b.get(k).map(|w| v.equals(w)).unwrap_or(false)))
            }
            _ => false,
        }
    }

    /// Get a reference to the unwrapped inner value (without Signal wrapper).
    pub fn unwrap_signal_ref(&self) -> &Value {
        match self {
            Value::Signal(v, _) => v,
            other => other,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::I64(n) => write!(f, "{n}"),
            Value::F64(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{}", if *b { "#t" } else { "#f" }),
            // lossy: Str may hold raw bytes (read-file is byte-semantic);
            // Display must not panic on invalid UTF-8
            Value::Str(s) => write!(f, "\"{}\"", String::from_utf8_lossy(s.as_bytes())),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Ref(_) => write!(f, "{{...}}"),
            Value::Cap(id, desc) => write!(f, "@{id}({desc})"),
            Value::Signal(v, c) => write!(f, "{v}:{c}"),
            Value::Map(m) => {
                write!(f, "{{")?;
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
        }
    }
}

/// Debug delegates to the lossy Display: trace/debug printing must
/// survive byte-semantic strings (raw binary from read-file).
impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
