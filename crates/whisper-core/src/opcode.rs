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

    // === Extended stack/list/string ops (0x05-0x0A) ===
    /// Filter list by predicate: list {pred} -> filtered-list
    Filter,
    /// Split string by delimiter: str delimiter -> [substrings]
    StrSplit,
    /// Convert char code to single-char string: i64 -> str
    Ctos,
    /// Slice a list: list start len -> sub-list
    ListSlice,
    /// Left shift: i64 i64 -> i64
    Shl,
    /// Right shift: i64 i64 -> i64
    Shr,
    /// Push CLI arguments as a list of strings: → [str]
    Args,
    /// Split a `?? then | else ]` token sequence at depth 1.
    /// tokens start → [then, else, next]  (seed-compatible, opcode 0xF0)
    SplitCond,
    /// Type predicate: value → bool (is this a list?)  (seed opcode 228)
    ListCheck,
    /// Type predicate: value → bool (is this an i64?)  (seed opcode 229)
    I64Check,
    /// Type predicate: value → bool (is this a string?)  (seed opcode 230)
    StrCheck,
    /// Type predicate: value → bool (is this a bool?)  (seed opcode 231)
    BoolCheck,

    // === Native IO (v1.0.1 vocabulary, opcodes 236-244) ===
    /// port -> server handle | -1
    Listen,
    /// server handle -> conn handle | -1 (blocking)
    Accept,
    /// conn -> request str (HTTP-style read; "" on close)
    Recv,
    /// conn str -> bytes sent | -1
    Send,
    /// handle -> (close socket)
    CloseSock,
    /// path -> contents | "" on error
    ReadFile,
    /// path data -> bytes written | -1
    WriteFile,
    /// name -> value | "" if unset
    EnvGet,

    // === Shared KV + concurrent serve (v1.0.2 vocabulary) ===
    /// key value -> (store; process-global, shared across threads)
    KvSet,
    /// key -> value | "" (missing yields empty string)
    KvGet,
    /// key -> (remove)
    KvDel,
    /// -> [keys]
    KvKeys,
    /// port {handler} -> (serve; never returns. VM: serial accept loop;
    /// C chain: one thread per connection)
    ServeConcurrent,

    // === P0 vocabulary (v1.0.3) ===
    /// -> str (one line from stdin, newline stripped; EOF yields "")
    ReadLine,
    /// str -> str (ASCII a-z to A-Z, other bytes untouched)
    StrUpper,
    /// str -> str (ASCII A-Z to a-z, other bytes untouched)
    StrLower,
    /// str -> str (trim ASCII whitespace at both ends)
    StrStrip,
    /// str value -> str (replace first {} with value as text)
    Fmt,

    // === P1 vocabulary (v1.0.4) ===
    /// path -> [names] (empty list on error)
    ListDir,
    /// path -> 0 | -1
    Mkdir,
    /// path -> #t | #f
    FileExists,
    /// ts fmt -> str (UTC; %Y %m %d %H %M %S)
    StrfTime,
    /// (inside {body} {cond} #) exit innermost loop; Leave(0) elsewhere
    /// is a no-op fall-through
    Leave(i32),

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
    /// Push string literal onto stack (Rc<String> — allocated once at compile time)
    PushStr(Rc<String>),
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
    /// Prepend element: elem list → new-list
    Prepend,

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

    // === Map operations (0xC2-0xC7) ===
    /// Create empty map: → map
    MapNew,
    /// Get value from map: map key → value (or 0 if not found)
    MapGet,
    /// Set key-value in map: map key value → new-map
    MapSet,
    /// Delete key from map: map key → new-map
    MapDel,
    /// Get all keys: map → [str]
    MapKeys,
    /// Check if key exists: map key → bool
    MapHas,

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

    // === Time (0xD0-0xD1) ===
    /// Get current unix timestamp: → i64
    Now,
    /// Sleep for milliseconds: i64 →
    Sleep,

    // === Random (0xD2-0xD3) ===
    /// Random float 0.0-1.0: → f64
    Random,
    /// Random integer in range: i64 i64 → i64
    RandInt,

    // === Float math (0xD4-0xD9) ===
    /// Floor: f64 → f64
    Floor,
    /// Ceil: f64 → f64
    Ceil,
    /// Round: f64 → f64
    Round,
    /// Natural log: f64 → f64
    Log,
    /// e^x: f64 → f64
    Exp,
    /// Arctangent of y/x: f64 f64 → f64
    Atan2,

    // === Bitwise (0xDA-0xDD) ===
    /// Bitwise AND: i64 i64 → i64
    BitAnd,
    /// Bitwise OR: i64 i64 → i64
    BitOr,
    /// Bitwise XOR: i64 i64 → i64
    BitXor,
    /// Bitwise NOT: i64 → i64
    BitNot,

    // === Sort (0xDF) ===
    /// Sort a list in ascending order: [T] → [T]
    Sort,
    /// Reverse a list: [T] → [T]
    Rev,
    /// Remove consecutive duplicates: [T] → [T]
    Dedup,
    /// Take first N elements: list n → sub-list
    Take,
    /// Drop first N elements: list n → sub-list
    DropN,

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
            Opcode::Filter => 0x05,
            Opcode::StrSplit => 0x06,
            Opcode::Ctos => 0x07,
            Opcode::ListSlice => 0x08,
            Opcode::Shl => 0x09,
            Opcode::Shr => 0x0A,
            Opcode::Args => 0x0B,
            // Seed-runtime natives (byte values mirror whisperc/runtime.c)
            Opcode::SplitCond => 0xF0,
            Opcode::Listen => 236,
            Opcode::Accept => 237,
            Opcode::Recv => 238,
            Opcode::Send => 239,
            Opcode::CloseSock => 241,
            Opcode::ReadFile => 242,
            Opcode::WriteFile => 243,
            Opcode::EnvGet => 244,
            Opcode::KvSet => 245,
            Opcode::KvGet => 246,
            Opcode::KvDel => 247,
            Opcode::KvKeys => 248,
            Opcode::ServeConcurrent => 255,
            Opcode::ReadLine => 249,
            Opcode::StrUpper => 250,
            Opcode::StrLower => 251,
            Opcode::StrStrip => 252,
            Opcode::Fmt => 253,
            Opcode::ListDir => 200,
            Opcode::Mkdir => 201,
            Opcode::FileExists => 202,
            Opcode::StrfTime => 203,
            Opcode::Leave(_) => 254,
            Opcode::ListCheck => 0xE4,
            Opcode::I64Check => 0xE5,
            Opcode::StrCheck => 0xE6,
            Opcode::BoolCheck => 0xE7,

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
            Opcode::Prepend => 0xDE,

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

            // Map ops
            Opcode::MapNew => 0xC2,
            Opcode::MapGet => 0xC3,
            Opcode::MapSet => 0xC4,
            Opcode::MapDel => 0xC5,
            Opcode::MapKeys => 0xC6,
            Opcode::MapHas => 0xC7,

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

            // Time
            Opcode::Now => 0xD0,
            Opcode::Sleep => 0xD1,

            // Random
            Opcode::Random => 0xD2,
            Opcode::RandInt => 0xD3,

            // Float math
            Opcode::Floor => 0xD4,
            Opcode::Ceil => 0xD5,
            Opcode::Round => 0xD6,
            Opcode::Log => 0xD7,
            Opcode::Exp => 0xD8,
            Opcode::Atan2 => 0xD9,

            // Bitwise
            Opcode::BitAnd => 0xDA,
            Opcode::BitOr => 0xDB,
            Opcode::BitXor => 0xDC,
            Opcode::BitNot => 0xDD,
            Opcode::Sort => 0xDF,
            Opcode::Rev => 0xE0,
            Opcode::Dedup => 0xE1,
            Opcode::Take => 0xE2,
            Opcode::DropN => 0xE3,

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

    /// Return the total byte size of this opcode including its operand.
    pub fn byte_size(&self) -> usize {
        match self {
            Opcode::Pick(_) => 2,
            Opcode::PushI64(_) => 9,
            Opcode::PushF64(_) => 9,
            Opcode::PushStr(s) => 5 + s.len(),
            Opcode::PushBool(_) => 2,
            Opcode::PushList => 1,
            Opcode::PushRef(inner) => 5 + inner.iter().map(|op| op.byte_size()).sum::<usize>(),
            Opcode::Cond(_) | Opcode::Jump(_) | Opcode::Loop(_) => 5,
            Opcode::Call(name) => 2 + name.len(),
            Opcode::CapCall(_) => 3,
            Opcode::ConfLabel(_) => 9,
            Opcode::DefWord(name) => 2 + name.len(),
            _ => 1,
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
            Opcode::Filter => "filter",
            Opcode::StrSplit => "strsplit",
            Opcode::Ctos => "ctos",
            Opcode::ListSlice => "listslice",
            Opcode::Shl => "shl",
            Opcode::Shr => "shr",
            Opcode::Args => "args",
            Opcode::SplitCond => "split-cond",
            Opcode::ListCheck => "list?",
            Opcode::I64Check => "i64?",
            Opcode::StrCheck => "str?",
            Opcode::BoolCheck => "bool?",
            Opcode::Listen => "listen",
            Opcode::Accept => "accept",
            Opcode::Recv => "recv",
            Opcode::Send => "send",
            Opcode::CloseSock => "close",
            Opcode::ReadFile => "read-file",
            Opcode::WriteFile => "write-file",
            Opcode::EnvGet => "getenv",
            Opcode::KvSet => "kv-set",
            Opcode::KvGet => "kv-get",
            Opcode::KvDel => "kv-del",
            Opcode::KvKeys => "kv-keys",
            Opcode::ServeConcurrent => "serve-concurrent",
            Opcode::ReadLine => "read-line",
            Opcode::StrUpper => "str-upper",
            Opcode::StrLower => "str-lower",
            Opcode::StrStrip => "str-strip",
            Opcode::Fmt => "fmt",
            Opcode::ListDir => "listdir",
            Opcode::Mkdir => "mkdir",
            Opcode::FileExists => "file-exists",
            Opcode::StrfTime => "strftime",
            Opcode::Leave(_) => "leave",
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
            Opcode::Prepend => "prepend",
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
            Opcode::MapNew => "map_new",
            Opcode::MapGet => "map_get",
            Opcode::MapSet => "map_set",
            Opcode::MapDel => "map_del",
            Opcode::MapKeys => "map_keys",
            Opcode::MapHas => "map_has",
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
            Opcode::Now => "now",
            Opcode::Sleep => "sleep",
            Opcode::Random => "random",
            Opcode::RandInt => "randint",
            Opcode::Floor => "floor",
            Opcode::Ceil => "ceil",
            Opcode::Round => "round",
            Opcode::Log => "log",
            Opcode::Exp => "exp",
            Opcode::Atan2 => "atan2",
            Opcode::BitAnd => "bitand",
            Opcode::BitOr => "bitor",
            Opcode::BitXor => "bitxor",
            Opcode::BitNot => "bitnot",
            Opcode::Sort => "sort",
            Opcode::Rev => "rev",
            Opcode::Dedup => "dedup",
            Opcode::Take => "take",
            Opcode::DropN => "drop-n",
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
