//! Token types for the Whisper lexer.
//! Each token carries a Span for source-level error reporting.

/// Source location information for error reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Span {
            start,
            end,
            line,
            column,
        }
    }
}

/// A token produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub lexeme: String,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span, lexeme: String) -> Self {
        Token { kind, span, lexeme }
    }
}

/// All token kinds in the Whisper language.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Integer(i64),
    Float(f64),
    String(String),
    BoolTrue,
    BoolFalse,

    // Stack operators
    Dup,       // _
    Swap,      // `
    Drop,      // % (context: stack)
    Rot,       // @
    Pick(u8),  // $n

    // Arithmetic
    Plus,
    Minus,
    Star,
    Slash,
    Percent,   // % (modulo)
    Mod,       // mod keyword (alias for %)

    // Comparison
    Eq,
    Lt,
    Gt,
    Neq,
    Le,
    Ge,

    // Logic
    And,       // &
    Or,        // |
    Not,       // !

    // Delimiters
    LBracket,  // [
    RBracket,  // ]
    LBrace,    // {
    RBrace,    // }

    // Control flow
    CondQ,       // ??
    CondArrow,   // ?->
    Hash,        // #
    AtTimes,     // @times
    AtMap,       // @map
    AtEach,      // @each
    AtFold,      // @fold
    AtNth,       // @nth

    // Definitions
    Colon,       // :
    Semicolon,   // ;
    Import,
    Export,

    // IO
    Dot,         // .
    Comma,       // ,
    DotDot,      // ..

    // Capability
    CapCall(u16), // @n (n is a number)
    Bang,         // !

    // Confidence
    ConfLabel(f64),  // :0.xx
    ProbChoice,      // ?|

    // List operations
    Append,
    Len,

    // String operations
    StrLen,
    StrCat,
    StrSlice,

    // Word reference (any identifier not matching keywords)
    Word(String),

    // Special
    Eof,
    Error(String),
}
