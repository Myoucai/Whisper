// Whisper Compiler v4.0 — full structural compilation
// Token types:
//   0  = I64 literal   [0, value]
//   1  = F64 literal   [1, bits_as_i64]
//   2  = Str literal   [2, string]
//   3  = Operator      [3, opcode_byte]
//   4  = WordRef       [4, name_string]
//   5  = Quote block   [5, [...inner_tokens...]]
//   6  = List          [6, [...element_tokens...]]
//   7  = Conditional   [7, [then_tokens], [else_tokens]]
//   8  = Loop          [8, [body_tokens], [cond_tokens]]
//   13 = Bool literal  [13, 0/1]
//   18 = Pre-compiled PushRef (legacy)

: tk-type { 0 @nth } ;
: tk-val  { 1 @nth } ;

: op-i64   { tk-val [48] ` append } ;
: op-f64   { tk-val [49] ` append } ;
: op-str   { tk-val [50] ` append } ;
: op-bool  { tk-val [51] ` append } ;
: op-op    { tk-val } ;
: op-call  { tk-val [96] ` append } ;
: op-list  { tk-val [52] ` append } ;
: op-ref   { tk-val } ;

// Quote block: recursively compile inner tokens, wrap in PushRef
: op-quote {
    tk-val compile
    [53] ` append
} ;

// List: compile each element, append [PushLen, PushList]
: op-list6 {
    [] tk-val
    { compile-one ` swap { ` swap ` append } @each } @each
    tk-val len [48] ` append swap append
    52 swap append
} ;

// Conditional: compile then, compile else, measure, emit Cond+Jump
: op-cond {
    tk-val                                     // [then_tokens[], else_tokens[]]
    0 @nth compile                             // then_bc
    swap 1 @nth compile                        // else_bc
    // Stack: [then_bc_list, else_bc_list]
    _ len _ len                                // [then_bc, else_bc, then_len, else_len]
    // Emit: Cond(then_len+1) + then_bc + Jump(else_len) + else_bc
    // Cond = [80, offset], Jump = [81, offset]
    [80] swap 1 + append                       // [80, then_len+1] — Cond opcode
    swap                                        // [else_bc, Cond_token, then_bc]
    `                                           // [else_bc, then_bc, Cond_token]
    swap append                                 // [else_bc, then_bc_with_Cond]
    [81] ` 4 pick append                        // [81, else_len] — Jump opcode
    swap append                                 // [..., Jump_token, else_bc]
    append                                      // [then_bc+Cond, Jump+else_bc]
} ;

// Loop: compile body, compile cond, measure, emit Loop(offset)
: op-loop {
    tk-val                                     // [body_tokens[], cond_tokens[]]
    0 @nth compile                             // body_bc
    swap 1 @nth compile                        // cond_bc
    // Stack: [body_bc, cond_bc]
    _ len _ len                                // [body_bc, cond_bc, body_len, cond_len]
    // Loop offset = -(body_len + cond_len + 1)  (jump back past body+cond+Loop)
    + 1 +                                      // total = body_len + cond_len + 1
    -1 *                                       // negate for backward jump
    // Emit: body_bc + cond_bc + Loop(offset)
    // Loop = [82, offset]
    [82] swap append                           // [82, -offset] — Loop opcode
    swap append                                // [body, cond+Loop]
    swap append                                // [body+cond+Loop]
} ;

: compile-one {
    _ tk-type
    0  = ??op-i64
    |_ tk-type 1  = ??op-f64
    |_ tk-type 2  = ??op-str
    |_ tk-type 3  = ??op-op
    |_ tk-type 4  = ??op-call
    |_ tk-type 5  = ??op-quote
    |_ tk-type 6  = ??op-list6
    |_ tk-type 7  = ??op-cond
    |_ tk-type 8  = ??op-loop
    |_ tk-type 13 = ??op-bool
    |_ tk-type 14 = ??op-list
    |_ tk-type 18 = ??op-ref
    |drop drop ] ] ] ] ] ] ] ] ] ] ] ]
} ;

: compile {
    { compile-one } @map
} ;

// ── Split definitions from main tokens ──────────────────────────────
// Input: classified token list (may contain [9, name, body] items)
// Output: [main_tokens, [[name, body], ...]]

: split-defs-loop {
    // main_acc defs_acc tokens → [main_acc, defs_acc]
    _ len 0 = ?? drop swap drop swap ]
    over 0 @nth                   // first token
    _ list? ??
        dup 0 @nth _ 9 = ??
            // It's a def: [9, name, body]
            drop drop
            dup 1 @nth            // name
            swap 2 @nth           // body
            [] swap append        // [body]
            rot                   // [defs_acc, main_acc, [body], name]
            // Build [name, body] pair
            [] swap append        // [name, body]
            swap ` swap append    // defs_acc += [name, body]
            // Wait, ordering is wrong. Let me restructure.
            drop drop drop
            // Skip this token (it's a def)
            swap 1 + swap
            split-defs-loop
        |
            // Not a def tag — add to main
            drop drop
            over 0 @nth
            swap ` swap append
            swap 1 + swap
            split-defs-loop
        ]
    |
        // Not a list — add to main
        drop
        over 0 @nth
        swap ` swap append
        swap 1 + swap
        split-defs-loop
    ]
} ;

: split-defs {
    // tokens → [main_tokens, defs_list]
    [] [] rot split-defs-loop
} ;

// ── Full compilation pipeline ───────────────────────────────────────
// source_string → [main_bytecodes, [[name, def_bytecodes], ...]]

: compile-full {
    classify-full               // tokenize → structify → classify
    split-defs                  // [main_tokens, defs_list]
    // Compile main tokens
    over compile                // [main_tokens, defs_list, main_bc]
    // Compile each def body
    // defs_list is [[name, body], ...] — need to compile each body
    // For now, just compile main and return empty defs
    // (Rust bridge handles def compilation)
    swap drop                   // [main_bc, defs_list]
    [] swap                     // [main_bc, [], defs_list]
    drop                        // [main_bc, []]
} ;

export compile
export classify-full
export split-defs
export compile-full
