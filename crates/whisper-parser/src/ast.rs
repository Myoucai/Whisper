//! AST node definitions for the Whisper language.
//!
//! Whisper is inherently linear (stack-based), so the AST is a flat
//! sequence of nodes rather than a deep tree. Quotations and lists
//! introduce nesting.

use whisper_core::value::Value;

/// A confidence score (0.0 to 1.0). Wraps f64 for Eq compatibility.
#[derive(Debug, Clone, Copy)]
pub struct Confidence(pub f64);

impl PartialEq for Confidence {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

/// A single AST node in a Whisper program.
#[derive(Debug, Clone)]
pub enum AstNode {
    /// A literal value pushed onto the stack.
    Literal(Value),

    /// Reference to a user-defined word.
    WordRef(String),

    /// A built-in operator.
    Op(Operator),

    /// A quotation block: { ... }
    Quote(Vec<AstNode>),

    /// A list literal: [ ... ]
    List(Vec<AstNode>),

    /// Conditional: cond ??then-expr|else-expr]
    Cond {
        then_branch: Vec<AstNode>,
        else_branch: Option<Vec<AstNode>>,
    },

    /// Single-branch conditional: cond {then} ?->
    CondArrow { then_branch: Vec<AstNode> },

    /// Loop: {body} {cond} #
    Loop {
        body: Vec<AstNode>,
        condition: Vec<AstNode>,
    },

    /// Fixed-count loop: n {body} @times
    Times { body: Vec<AstNode> },

    /// Word definition: : name { body } ;
    Def { name: String, body: Vec<AstNode> },

    /// Import module: import "path"
    Import(String),

    /// Export word: export name
    Export(String),

    /// Confidence label: { ... } :0.93
    ConfidenceLabel {
        body: Vec<AstNode>,
        confidence: Confidence,
    },

    /// Probabilistic choice: {alt1} {alt2} ?|
    ProbChoice {
        alt1: Vec<AstNode>,
        alt2: Vec<AstNode>,
    },
}

/// Built-in operators in the Whisper language.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Operator {
    // Stack
    Dup,
    Swap,
    Drop,
    Rot,
    Pick(u8),

    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,

    // Comparison
    Eq,
    Lt,
    Gt,
    Neq,
    Le,
    Ge,

    // Logic
    And,
    Or,
    Not,

    // List
    Nth,
    Append,
    Len,
    Map,
    Each,
    Fold,

    // String
    StrLen,
    StrCat,
    StrSlice,
    StrEq,
    StrLt,
    StrFind,
    StrReplace,
    StrToI64,
    I64ToStr,
    StrNth,
    StrChars,
    CharsStr,
    StrIter,
    ListFind,
    StrJoin,
    BytesNew,
    BytesPush,
    BytesLen,
    BytesWriteFile,
    Try,

    // Float
    I64ToF64,
    F64ToI64,
    FSqrt,
    FSin,
    FCos,
    FTan,

    // JSON
    JsonParse,
    JsonStringify,

    // Control flow (runtime)
    CondQ,
    CondArrow,
    Hash,
    AtTimes,

    // Capability
    CapCall(u16),
    CapExec,

    // Confidence
    ConfLabel(f64),
    ProbChoice,

    // IO
    OutputTop,
    OutputAll,
    ReadInput,
}
