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

impl TokenKind {
    /// Canonical source-level spelling for keyword tokens that lex from an
    /// identifier (or the `over` alias of `Pick(1)`).
    ///
    /// Invariant: this must stay in sync with the keyword map in
    /// `lexer.rs::read_word` — every identifier that lexes to a non-`Word`
    /// kind must round-trip back to its spelling here. `parser_tests` guards
    /// this: each keyword must be usable as a definition name.
    pub fn keyword_name(&self) -> Option<&'static str> {
        Some(match self {
            Self::Import => "import",
            Self::Export => "export",
            Self::Append => "append",
            Self::Prepend => "prepend",
            Self::Len => "len",
            Self::Drop => "drop",
            Self::Dup => "dup",
            Self::Swap => "swap",
            Self::Rot => "rot",
            Self::Pick(1) => "over",
            Self::Mod => "mod",
            Self::StrLen => "strlen",
            Self::StrCat => "strcat",
            Self::StrSlice => "strslice",
            Self::StrEq => "streq",
            Self::StrLt => "strlt",
            Self::StrFind => "strfind",
            Self::StrReplace => "strreplace",
            Self::StrToI64 => "strtoi64",
            Self::I64ToStr => "i64tostr",
            Self::StrNth => "strnth",
            Self::StrChars => "strchars",
            Self::CharsStr => "charsstr",
            Self::StrIter => "striter",
            Self::ListFind => "listfind",
            Self::StrJoin => "strjoin",
            Self::StrSplit => "strsplit",
            Self::Ctos => "ctos",
            Self::ListSlice => "list-slice",
            Self::Args => "args",
            Self::SplitCond => "split-cond",
            Self::ListCheck => "list?",
            Self::I64Check => "i64?",
            Self::StrCheck => "str?",
            Self::BoolCheck => "bool?",
            Self::Return => "return",
            Self::Listen => "listen",
            Self::Accept => "accept",
            Self::Recv => "recv",
            Self::Send => "send",
            Self::CloseSock => "close",
            Self::ReadFile => "read-file",
            Self::WriteFile => "write-file",
            Self::EnvGet => "getenv",
            Self::KvSet => "kv-set",
            Self::KvGet => "kv-get",
            Self::KvDel => "kv-del",
            Self::KvKeys => "kv-keys",
            Self::ServeConcurrent => "serve-concurrent",
            Self::ReadLine => "read-line",
            Self::StrUpper => "str-upper",
            Self::StrLower => "str-lower",
            Self::StrStrip => "str-strip",
            Self::Fmt => "fmt",
            Self::ListDir => "listdir",
            Self::Mkdir => "mkdir",
            Self::FileExists => "file-exists",
            Self::StrfTime => "strftime",
            Self::Leave => "leave",
            Self::StrMatch => "strmatch",
            Self::StrFindRe => "strfind-re",
            Self::Pack => "pack",
            Self::Unpack => "unpack",
            Self::Range => "range",
            Self::FfiOpen => "ffi-open",
            Self::FfiSym => "ffi-sym",
            Self::FfiCall => "ffi-call",
            Self::BufAlloc => "buf-alloc",
            Self::BufFree => "buf-free",
            Self::BufPeek => "buf-peek",
            Self::BufPoke => "buf-poke",
            Self::BufToStr => "buf->str",
            Self::MapNew => "map-new",
            Self::MapGet => "map-get",
            Self::MapSet => "map-set",
            Self::MapDel => "map-del",
            Self::MapKeys => "map-keys",
            Self::MapHas => "map-has",
            Self::BytesNew => "bytes-new",
            Self::BytesPush => "bytes-push",
            Self::BytesLen => "bytes-len",
            Self::BytesWriteFile => "bytes-write",
            Self::BytesToHex => "bytes->hex",
            Self::Try => "try",
            Self::Print => "print",
            Self::Exit => "exit",
            Self::CapExec => "call",
            Self::I64ToF64 => "i64tof64",
            Self::F64ToI64 => "f64toi64",
            Self::FSqrt => "fsqrt",
            Self::FSin => "fsin",
            Self::FCos => "fcos",
            Self::FTan => "ftan",
            Self::JsonParse => "json-parse",
            Self::JsonStringify => "json-stringify",
            Self::Now => "now",
            Self::Sleep => "sleep",
            Self::Random => "random",
            Self::RandInt => "randint",
            Self::Floor => "floor",
            Self::Ceil => "ceil",
            Self::Round => "round",
            Self::Log => "log",
            Self::Exp => "exp",
            Self::Atan2 => "atan2",
            Self::BitAnd => "bitand",
            Self::BitOr => "bitor",
            Self::BitXor => "bitxor",
            Self::BitNot => "bitnot",
            Self::Sort => "sort",
            Self::Rev => "rev",
            Self::Dedup => "dedup",
            Self::Take => "take",
            Self::DropN => "drop-n",
            _ => return None,
        })
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
    Dup,      // _
    Swap,     // `
    Drop,     // % (context: stack)
    Rot,      // @
    Pick(u8), // $n

    // Arithmetic
    Plus,
    Minus,
    Star,
    Slash,
    Percent, // % (modulo)
    Mod,     // mod keyword (alias for %)

    // Comparison
    Eq,
    Lt,
    Gt,
    Neq,
    Le,
    Ge,

    // Logic
    And, // &
    Or,  // |
    Not, // !

    // Delimiters
    LBracket, // [
    RBracket, // ]
    LBrace,   // {
    RBrace,   // }

    // Control flow
    CondQ,     // ??
    CondArrow, // ?->
    Hash,      // #
    AtTimes,   // @times
    AtMap,     // @map
    AtEach,    // @each
    AtFold,    // @fold
    AtNth,     // @nth
    AtFilter,  // @filter
    Filter,    // filter keyword

    // Definitions
    Colon,     // :
    Semicolon, // ;
    Import,
    Export,

    // IO
    Dot,    // .
    Comma,  // ,
    DotDot, // ..

    // Capability
    CapCall(u16), // @n (n is a number)
    /// `call` — invoke the quotation (or capability token) on top of the
    /// stack. Backs `Operator::CapExec`.
    ///
    /// This variant used to be `Bang`, i.e. the design intended `!` to exec a
    /// quotation. The lexer then gave `!` to logical not (spec §3.2, frozen
    /// syntax) and never produced `Bang` again — so the opcode stayed fully
    /// implemented in the VM while being unreachable from source. A word is
    /// the only spelling left, and `call` is what concatenative languages use.
    CapExec,

    // Confidence
    ConfLabel(f64), // :0.xx
    ProbChoice,     // ?|

    // List operations
    Append,
    Prepend,
    Len,

    // String operations
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
    StrSplit,
    Ctos,
    Chr, // alias for ctos (legacy)
    ListSlice,
    Shl,
    Shr,
    Args,
    MapNew,
    MapGet,
    MapSet,
    MapDel,
    MapKeys,
    MapHas,
    BytesNew,
    BytesPush,
    BytesLen,
    BytesWriteFile,
    BytesToHex,
    Try,
    Print,
    Exit,
    SplitCond,
    ListCheck,
    I64Check,
    StrCheck,
    BoolCheck,
    Return,
    Listen,
    Accept,
    Recv,
    Send,
    CloseSock,
    ReadFile,
    WriteFile,
    EnvGet,
    OpEffect,
    QuoteSize,
    WordSize,
    SetSeed,
    Rand,
    ConfOf,
    UnwrapSig,
    KvSet,
    KvGet,
    KvDel,
    KvKeys,
    ServeConcurrent,
    ReadLine,
    StrUpper,
    StrLower,
    StrStrip,
    Fmt,
    ListDir,
    Mkdir,
    FileExists,
    StrfTime,
    Leave,
    StrMatch,
    StrFindRe,
    Pack,
    Unpack,
    Range,
    FfiOpen,
    FfiSym,
    FfiCall,
    BufAlloc,
    BufFree,
    BufPeek,
    BufPoke,
    BufToStr,

    // Float operations
    I64ToF64,
    F64ToI64,
    FSqrt,
    FSin,
    FCos,
    FTan,

    // JSON
    JsonParse,
    JsonStringify,

    // Time
    Now,
    Sleep,

    // Random
    Random,
    RandInt,

    // Float math
    Floor,
    Ceil,
    Round,
    Log,
    Exp,
    Atan2,

    // Bitwise
    BitAnd,
    BitOr,
    BitXor,
    BitNot,

    // Sort
    Sort,
    Rev,
    Dedup,

    // List take/drop
    Take,
    DropN,

    // Word reference (any identifier not matching keywords)
    Word(String),

    // Special
    Eof,
    Error(String),
}
