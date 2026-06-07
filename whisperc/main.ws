// Whisper Compiler v3.0 — nested quote compilation
//
// Token types:
//   0  = I64 literal   [0, value]
//   1  = F64 literal   [1, bits_as_i64]
//   2  = Str literal   [2, string]
//   3  = Operator      [3, opcode_byte]
//   4  = WordRef       [4, name_string]
//   5  = Quote block   [5, [...inner_tokens...]]  ← NEW: recursive compile
//   13 = Bool literal  [13, 0/1]
//   14 = ListCount     [14, count]
//   18 = Pre-compiled PushRef (legacy, passed through)

: tk-type { 0 @nth } ;
: tk-val  { 1 @nth } ;

: op-i64   { tk-val [48] ` append } ;
: op-f64   { tk-val [49] ` append } ;
: op-str   { tk-val [50] ` append } ;
: op-bool  { tk-val [51] ` append } ;
: op-list  { tk-val [52] ` append } ;
: op-op    { tk-val } ;
: op-call  { tk-val [96] ` append } ;
: op-ref   { tk-val } ;

// Quote block: recursively compile inner tokens, wrap in PushRef
: op-quote {
    tk-val compile                    // compile inner tokens → bytecode list
    [53] ` append                     // [0x35, [compiled_inner]]
} ;

: compile-one {
    _ tk-type
    0  = ??op-i64
    |_ tk-type 1  = ??op-f64
    |_ tk-type 2  = ??op-str
    |_ tk-type 3  = ??op-op
    |_ tk-type 4  = ??op-call
    |_ tk-type 5  = ??op-quote
    |_ tk-type 13 = ??op-bool
    |_ tk-type 14 = ??op-list
    |_ tk-type 18 = ??op-ref
    |drop drop ] ] ] ] ] ] ] ] ]
} ;

: compile {
    { compile-one } @map
} ;

export compile
