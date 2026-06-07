//! Opcode definitions for the Whisper VM.
//! Maps directly to the design document section 5.2 (binary format).
//!
//! Binary encoding:
//!   0x00-0x0F: stack ops
//!   0x10-0x17: arithmetic
//!   0x18-0x1F: comparison
//!   0x20-0x23: logic
//!   0x30-0x3F: literals (prefix + LEB128 data)
//!   0x40-0x47: list operations
//!   0x50-0x57: control flow
//!   0x60-0x67: call/return
//!   0x70-0x73: capability
//!   0x80-0x83: confidence
//!   0x90-0x93: IO
//!   0xA0-0xA3: definitions

use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub enum Opcode {
    // === Stack operations (0x00-0x0F) ===
    /// Duplicate top of stack: a → a a
    Dup,
    /// Swap top two elements: a b → b a
    Swap,
    /// Drop top of stack: a →
    Drop,
    /// Rotate top three: a b c → b c a
    Rot,
    /// Pick nth element (0-based from top): ... a_n ... a_0 → ... a_n ... a_0 a_n
    Pick(u8),

    // === Arithmetic (0x10-0x17) ===
    /// Addition: a b → (a+b)
    Add,
    /// Subtraction: a b → (a-b)
    Sub,
    /// Multiplication: a b → (a*b)
    Mul,
    /// Division: a b → (a/b)
    Div,
    /// Modulo: a b → (a%b) — context-disambiguated from Drop
    Mod,

    // === Comparison (0x18-0x1F) ===
    /// Equal: a b → (a==b)
    Eq,
    /// Less than: a b → (a<b)
    Lt,
    /// Greater than: a b → (a>b)
    Gt,
    /// Not equal: a b → (a!=b)
    Neq,
    /// Less or equal: a b → (a<=b)
    Le,
    /// Greater or equal: a b → (a>=b)
    Ge,

    // === Logic (0x20-0x23) ===
    /// Logical AND: a b → (a and b)
    And,
    /// Logical OR: a b → (a or b)
    Or,
    /// Logical NOT: a → (not a)
    Not,

    // === Literals (0x30-0x3F) ===
    /// Push i64 literal onto stack
    PushI64(i64),
    /// Push f64 literal onto stack
    PushF64(f64),
    /// Push string literal onto stack
    PushStr(Rc<str>),
    /// Push bool literal onto stack
    PushBool(bool),
    /// Push list (followed by element count and elements)
    PushList,
    /// Push quotation block with inline bytecode
    PushRef(Vec<Opcode>),

    // === List operations (0x40-0x47) ===
    /// Take nth element: list n → element
    Nth,
    /// Append element: list elem → new-list
    Append,
    /// List length: list → length
    Len,
    /// Map over list: list {fn} → new-list
    Map,
    /// Iterate over list: list {fn} →
    Each,
    /// Fold over list: list init {fn} → result
    Fold,

    // === String operations (0x46-0x4E) ===
    /// String length: str → i64
    StrLen,
    /// String concatenation: str1 str2 → str3
    StrCat,
    /// Substring: str start len → substr
    StrSlice,
    /// String equality: str1 str2 → bool
    StrEq,
    /// String less-than (lexicographic): str1 str2 → bool
    StrLt,
    /// Find substring: str pattern → i64  (index, or -1 if not found)
    StrFind,
    /// Replace all occurrences: str old new → str
    StrReplace,
    /// Parse string to i64: str → i64
    StrToI64,
    /// Format i64 to string: i64 → str
    I64ToStr,
    /// Get character code at index: str i64 → i64
    StrNth,
    /// Convert string to list of char codes: str → [i64]
    StrChars,
    /// Convert list of char codes to string: [i64] → str
    CharsStr,
    /// Iterate: pop str, push (first_char_code, rest_str). Empty → (-1, "")
    StrIter,
    /// Find key in assoc list: [[k v]…] key → [#t val] | [#f 0]
    ListFind,
    /// Join list of strings: [str…] → str
    StrJoin,

    // === Binary output (0xBD-0xC0) ===
    /// Create new byte buffer: → handle(i64)
    BytesNew,
    /// Push byte to buffer: handle byte → handle
    BytesPush,
    /// Get buffer length: handle → i64
    BytesLen,
    /// Write buffer to file: handle filename →
    BytesWriteFile,
    /// Try-catch: {body} → [#t result] | [#f "error"]
    Try,

    // === Control flow (0x50-0x57) ===
    /// Conditional: if false, jump by offset
    Cond(i32),
    /// Unconditional jump by offset
    Jump(i32),
    /// Loop: body, then check cond; jump back if true
    Loop(i32),
    /// Fixed-count loop: n {body} @times
    Times,

    // === Call/Return (0x60-0x67) ===
    /// Call word by name (looked up in VM word_dict at runtime)
    Call(String),
    /// Return from word/block
    Return,

    // === Capability (0x70-0x73) ===
    /// Call capability by id
    CapCall(u16),
    /// Execute ref/capability on stack
    CapExec,

    // === Float operations (0xB0-0xB5) ===
    /// Convert i64 to f64: i64 → f64
    I64ToF64,
    /// Convert f64 to i64 (truncate): f64 → i64
    F64ToI64,
    /// Square root: f64 → f64
    FSqrt,
    /// Sine: f64 → f64
    FSin,
    /// Cosine: f64 → f64
    FCos,
    /// Tangent: f64 → f64
    FTan,

    // === JSON (0xB6-0xB7) ===
    /// Parse JSON string to Whisper value: str → value
    JsonParse,
    /// Serialize Whisper value to JSON string: value → str
    JsonStringify,

    // === Confidence (0x80-0x83) ===
    /// Label following value with confidence
    ConfLabel(f64),
    /// Probabilistic choice: {alt1} {alt2} ?|
    ProbChoice,

    // === IO (0x90-0x93) ===
    /// Output top of stack
    OutputTop,
    /// Output entire stack
    OutputAll,
    /// Read input
    ReadInput,

    // === Definitions (0xA0-0xA3) ===
    /// Define a new word: name
    DefWord(String),
    /// End word definition
    EndDef,
    /// Import module: path
    Import,
    /// Export word: name
    Export,
}

impl Opcode {
    /// Return the opcode byte for .wbin serialization.
    pub fn to_byte(&self) -> u8 {
        match self {
            // Stack ops
            Opcode::Dup => 0x00,
            Opcode::Swap => 0x01,
            Opcode::Drop => 0x02,
            Opcode::Rot => 0x03,
            Opcode::Pick(_) => 0x04,

            // Arithmetic
            Opcode::Add => 0x10,
            Opcode::Sub => 0x11,
            Opcode::Mul => 0x12,
            Opcode::Div => 0x13,
            Opcode::Mod => 0x14,

            // Comparison
            Opcode::Eq => 0x18,
            Opcode::Lt => 0x19,
            Opcode::Gt => 0x1A,
            Opcode::Neq => 0x1B,
            Opcode::Le => 0x1C,
            Opcode::Ge => 0x1D,

            // Logic
            Opcode::And => 0x20,
            Opcode::Or => 0x21,
            Opcode::Not => 0x22,

            // Literals
            Opcode::PushI64(_) => 0x30,
            Opcode::PushF64(_) => 0x31,
            Opcode::PushStr(_) => 0x32,
            Opcode::PushBool(_) => 0x33,
            Opcode::PushList => 0x34,
            Opcode::PushRef(_) => 0x35,

            // List ops
            Opcode::Nth => 0x40,
            Opcode::Append => 0x41,
            Opcode::Len => 0x42,
            Opcode::Map => 0x43,
            Opcode::Each => 0x44,
            Opcode::Fold => 0x45,

            // String ops
            Opcode::StrLen => 0x46,
            Opcode::StrCat => 0x47,
            Opcode::StrSlice => 0x48,
            Opcode::StrEq => 0x49,
            Opcode::StrLt => 0x4A,
            Opcode::StrFind => 0x4B,
            Opcode::StrReplace => 0x4C,
            Opcode::StrToI64 => 0x4D,
            Opcode::I64ToStr => 0x4E,
            Opcode::StrNth => 0x4F,
            Opcode::StrChars => 0xB8,
            Opcode::CharsStr => 0xB9,
            Opcode::StrIter => 0xBA,
            Opcode::ListFind => 0xBB,
            Opcode::StrJoin => 0xBC,
            Opcode::BytesNew => 0xBD,
            Opcode::BytesPush => 0xBE,
            Opcode::BytesLen => 0xBF,
            Opcode::BytesWriteFile => 0xC0,
            Opcode::Try => 0xC1,

            // Control flow
            Opcode::Cond(_) => 0x50,
            Opcode::Jump(_) => 0x51,
            Opcode::Loop(_) => 0x52,
            Opcode::Times => 0x53,

            // Call/Return
            Opcode::Call(_) => 0x60,
            Opcode::Return => 0x61,

            // Capability
            Opcode::CapCall(_) => 0x70,
            Opcode::CapExec => 0x71,

            // Float ops
            Opcode::I64ToF64 => 0xB0,
            Opcode::F64ToI64 => 0xB1,
            Opcode::FSqrt => 0xB2,
            Opcode::FSin => 0xB3,
            Opcode::FCos => 0xB4,
            Opcode::FTan => 0xB5,
            Opcode::JsonParse => 0xB6,
            Opcode::JsonStringify => 0xB7,

            // Confidence
            Opcode::ConfLabel(_) => 0x80,
            Opcode::ProbChoice => 0x81,

            // IO
            Opcode::OutputTop => 0x90,
            Opcode::OutputAll => 0x91,
            Opcode::ReadInput => 0x92,

            // Definitions
            Opcode::DefWord(_) => 0xA0,
            Opcode::EndDef => 0xA1,
            Opcode::Import => 0xA2,
            Opcode::Export => 0xA3,
        }
    }

    /// Human-readable name for debugging/tracing.
    pub fn name(&self) -> &'static str {
        match self {
            Opcode::Dup => "dup",
            Opcode::Swap => "swap",
            Opcode::Drop => "drop",
            Opcode::Rot => "rot",
            Opcode::Pick(_) => "pick",
            Opcode::Add => "add",
            Opcode::Sub => "sub",
            Opcode::Mul => "mul",
            Opcode::Div => "div",
            Opcode::Mod => "mod",
            Opcode::Eq => "eq",
            Opcode::Lt => "lt",
            Opcode::Gt => "gt",
            Opcode::Neq => "neq",
            Opcode::Le => "le",
            Opcode::Ge => "ge",
            Opcode::And => "and",
            Opcode::Or => "or",
            Opcode::Not => "not",
            Opcode::PushI64(_) => "push_i64",
            Opcode::PushF64(_) => "push_f64",
            Opcode::PushStr(_) => "push_str",
            Opcode::PushBool(_) => "push_bool",
            Opcode::PushList => "push_list",
            Opcode::PushRef(_) => "push_ref",
            Opcode::Nth => "nth",
            Opcode::Append => "append",
            Opcode::Len => "len",
            Opcode::Map => "map",
            Opcode::Each => "each",
            Opcode::Fold => "fold",
            Opcode::StrLen => "strlen",
            Opcode::StrCat => "strcat",
            Opcode::StrSlice => "strslice",
            Opcode::StrEq => "streq",
            Opcode::StrLt => "strlt",
            Opcode::StrFind => "strfind",
            Opcode::StrReplace => "strreplace",
            Opcode::StrToI64 => "strtoi64",
            Opcode::I64ToStr => "i64tostr",
            Opcode::StrNth => "strnth",
            Opcode::StrChars => "strchars",
            Opcode::CharsStr => "charsstr",
            Opcode::StrIter => "striter",
            Opcode::ListFind => "listfind",
            Opcode::StrJoin => "strjoin",
            Opcode::BytesNew => "bytes_new",
            Opcode::BytesPush => "bytes_push",
            Opcode::BytesLen => "bytes_len",
            Opcode::BytesWriteFile => "bytes_write",
            Opcode::Try => "try",
            Opcode::Cond(_) => "cond",
            Opcode::Jump(_) => "jump",
            Opcode::Loop(_) => "loop",
            Opcode::Times => "times",
            Opcode::Call(_) => "call",
            Opcode::Return => "return",
            Opcode::CapCall(_) => "cap_call",
            Opcode::CapExec => "cap_exec",
            Opcode::I64ToF64 => "i64tof64",
            Opcode::F64ToI64 => "f64toi64",
            Opcode::FSqrt => "fsqrt",
            Opcode::FSin => "fsin",
            Opcode::FCos => "fcos",
            Opcode::FTan => "ftan",
            Opcode::JsonParse => "json_parse",
            Opcode::JsonStringify => "json_stringify",
            Opcode::ConfLabel(_) => "conf_label",
            Opcode::ProbChoice => "prob_choice",
            Opcode::OutputTop => "output_top",
            Opcode::OutputAll => "output_all",
            Opcode::ReadInput => "read_input",
            Opcode::DefWord(_) => "def_word",
            Opcode::EndDef => "end_def",
            Opcode::Import => "import",
            Opcode::Export => "export",
        }
    }
}
