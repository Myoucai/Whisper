//! Static stack effects per opcode — the contract-checking substrate.
//!
//! `(pops, pushes)` counts are taken from the VM implementations and
//! the doc comments in `whisper-core/src/opcode.rs` (the authoritative
//! in-repo spec). `None` = effect not statically known (variable arity,
//! runs user code, or legacy serial form) — it makes the surrounding
//! depth unknown, which disables downstream checks conservatively but
//! never produces a false positive.

use whisper_core::opcode::Opcode;

/// Static stack effect: (values popped, values pushed).
pub fn stack_effect(op: &Opcode) -> Option<(usize, usize)> {
    use Opcode::*;
    Some(match op {
        // ── Stack ──────────────────────────────────────────────
        Dup => (1, 2),
        Swap => (2, 2),
        Drop => (1, 0),
        Rot => (3, 3),
        Pick(n) => (*n as usize + 1, *n as usize + 2),

        // ── Literals ───────────────────────────────────────────
        PushI64(_) | PushF64(_) | PushStr(_) | PushBool(_) | PushRef(_) => (0, 1),
        PushList => return None, // pops count + count elements (variable)

        // ── Arithmetic / comparison / logic (i64 & f64) ────────
        Add | Sub | Mul | Div | Mod | Shl | Shr | BitAnd | BitOr | BitXor | Atan2 => (2, 1),
        Eq | Lt | Gt | Neq | Le | Ge | And | Or => (2, 1),
        Not | BitNot => (1, 1),

        // ── Math (f64) ─────────────────────────────────────────
        I64ToF64 | F64ToI64 | FSqrt | FSin | FCos | FTan | Floor | Ceil | Round
        | Log | Exp => (1, 1),

        // ── Lists ──────────────────────────────────────────────
        Nth | Append | Prepend | Take | DropN => (2, 1),
        Len | Sort | Rev | Dedup => (1, 1),
        ListSlice => (3, 1), // list start len -> sub-list
        ListFind => (2, 2),  // [[k v]…] key -> [#t val] | [#f 0]
        ListCheck => (1, 1),

        // ── Higher-order (quote consumed; effect of quote opaque) ──
        Map | Filter => (2, 1), // list {fn} -> new-list
        Each => (2, 0),         // list {fn} ->
        Fold => (3, 1),         // list init {fn} -> result
        Times => (2, 0),        // n {body} @times

        // ── Strings ────────────────────────────────────────────
        StrLen | StrUpper | StrLower | StrStrip | StrToI64 | I64ToStr | Ctos
        | StrChars | CharsStr | StrJoin => (1, 1),
        StrCat | StrEq | StrLt | StrFind | StrMatch | StrFindRe | StrSplit => (2, 1),
        StrSlice => (3, 1),   // str start len -> substr
        StrNth => (2, 1),     // str i64 -> i64
        StrReplace => (3, 1), // str old new -> str
        StrIter => (1, 2),    // str -> (first_char_code, rest_str)
        StrCheck => (1, 1),

        // ── Maps / JSON ────────────────────────────────────────
        MapNew => (0, 1),
        MapGet | MapDel | MapHas => (2, 1),
        MapSet => (3, 1),
        MapKeys => (1, 1),
        JsonParse | JsonStringify => (1, 1),

        // ── Bytes buffers ──────────────────────────────────────
        BytesNew => (0, 1),
        BytesPush => (2, 1),   // handle byte -> handle
        BytesLen => (1, 1),    // handle -> i64
        BytesWriteFile => (2, 0), // handle filename ->

        // ── Binary struct pack ─────────────────────────────────
        Pack => (2, 1),   // [values] fmt -> bytes
        Unpack => (2, 1), // bytes fmt -> [values]

        // ── I/O & environment ──────────────────────────────────
        ReadFile => (1, 1),
        WriteFile => (2, 1), // path data -> bytes written
        ListDir => (1, 1),
        Mkdir => (1, 1),  // path -> 0|-1
        FileExists => (1, 1),
        EnvGet => (1, 1),
        ReadLine => (0, 1),
        OutputTop => (1, 0), // pops (the `_ .` idiom is Dup first)
        OutputAll => (0, 0), // prints, does not pop
        ReadInput => (0, 1),

        // ── KV store ───────────────────────────────────────────
        OpEffect => (1, 2),
        QuoteSize => (1, 1),
        WordSize => (1, 1),
        SetSeed => (1, 0),
        Rand => (0, 1),
        ConfOf => (1, 2),
        UnwrapSig => (1, 1),
        KvSet => (2, 0),
        KvGet => (1, 1),
        KvDel => (1, 0),
        KvKeys => (0, 1),

        // ── Time / random ──────────────────────────────────────
        Now => (0, 1),
        Sleep => (1, 0),
        Random => (0, 1),
        RandInt => (2, 1),
        StrfTime => (2, 1), // ts fmt -> str

        // ── FFI & buffers ──────────────────────────────────────
        FfiOpen => (1, 1),
        FfiSym => (2, 1),
        FfiCall => (3, 1),  // [args] fmt fnptr -> result
        BufAlloc => (1, 1),
        BufFree => (1, 0),
        BufPeek => (1, 1),  // ptr -> i64
        BufPoke => (2, 0),  // ptr val ->
        BufToStr => (2, 1), // ptr len -> str

        // ── Sockets ────────────────────────────────────────────
        Listen => (1, 1),
        Accept => (1, 1),
        Recv => (1, 1),
        Send => (2, 1),
        CloseSock => (1, 0),

        // ── Regex ──────────────────────────────────────────────
        // (StrMatch/StrFindRe above)

        // ── Formatting / type predicates ───────────────────────
        Fmt => (2, 1), // fmt value -> str
        I64Check | BoolCheck => (1, 1),
        Args => (0, 1),   // CLI arguments as [str]
        Range => (2, 1),  // start count -> [start..start+count-1]

        // ── Control flow ───────────────────────────────────────
        Return => (0, 0), // pops the frame, not stack values
        SplitCond => (2, 3), // tokens start -> [then, else, next]

        // ── Not statically known ───────────────────────────────
        // Word calls: net effect comes from `def_effects` when the
        // definition is available in the same compile unit (see
        // `BlockProgram::validate`); otherwise unknown.
        Call(_) => return None,
        // Runs a quote/capability with opaque effects:
        Try | CapExec | ProbChoice | CapCall(_) | ServeConcurrent => return None,
        // Legacy serial-definition form, never produced by block codegen:
        DefWord(_) | EndDef | Import | Export => return None,
        // Metadata labels:
        ConfLabel(_) => return None,
        // Terminators are block-level, never block instructions:
        Cond(_) | Jump(_) | Loop(_) => return None,
        // Converted to Jump(exit) at block level; never emitted:
        Leave(_) => return None,
    })
}
