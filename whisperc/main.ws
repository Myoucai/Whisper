// Whisper Compiler v2.0 — nested token → bytecode
//
// Token format: [type_str, value]
//   type_str = "int" "float" "str" "bool" "op" "word" "{" ":" "??"
//   Special tokens:
//     ["{", [...inner_tokens...]]  — quote → PushRef(compile(inner))
//     [":", [name, [...body...]]]  — word definition
//
// Output: list of bytecode values (opcodes encoded as [byte, args...])

// ── Token access ──

: tk-type { 0 @nth } ;
: tk-val  { 1 @nth } ;

// ── Single opcode output ──
// Pushes a single-opcode list onto the output stack

: op-dup    { [0] } ;
: op-swap   { [1] } ;
: op-drop   { [2] } ;
: op-rot    { [3] } ;
: op-add    { [16] } ;
: op-sub    { [17] } ;
: op-mul    { [18] } ;
: op-div    { [19] } ;
: op-mod    { [20] } ;
: op-eq     { [24] } ;
: op-lt     { [25] } ;
: op-gt     { [26] } ;
: op-neq    { [27] } ;
: op-le     { [28] } ;
: op-ge     { [29] } ;
: op-and    { [32] } ;
: op-or     { [33] } ;
: op-not    { [34] } ;
: op-nth    { [64] } ;
: op-append { [65] } ;
: op-len    { [66] } ;
: op-map    { [67] } ;
: op-each   { [68] } ;
: op-fold   { [69] } ;
: op-strlen { [70] } ;
: op-strcat { [71] } ;
: op-strslice { [72] } ;
: op-streq  { [73] } ;
: op-strlt  { [74] } ;
: op-strfind { [75] } ;
: op-strreplace { [76] } ;
: op-strtoi64 { [77] } ;
: op-i64tostr { [78] } ;
: op-strnth { [79] } ;
: op-output { [144] } ;
: op-return { [97] } ;
: op-times  { [83] } ;

// ── Compile one token into bytecode ──
// Token is [type_str, value]; result is accumulated bytecode list

: compile-one {
    _ tk-type

    _ "int" streq ?? drop tk-val [48] swap append|]
    _ "float" streq ?? drop tk-val [49] swap append|]
    _ "str" streq ?? drop tk-val [50] swap append|]
    _ "bool" streq ?? drop tk-val [51] swap append|]
    _ "word" streq ?? drop tk-val [96] swap append|]

    // Operator: push just the opcode byte
    _ "op" streq ?? drop tk-val|]

    // Quote block: recursively compile inner, wrap in PushRef [0x35, [inner]]
    _ "{" streq ??
        drop tk-val compile              // compile inner tokens → list
        [53] swap append                // [0x35, [compiled_inner]]
    |]

    // Word definition: skip for now (handled separately)
    _ ":" streq ?? drop drop []|]

    // Default: pass through as-is
    |drop tk-val ]
    ] ] ] ] ] ] ] ]
} ;

// ── Compile a list of tokens into a flat bytecode list ──

: compile-loop {
    // acc tokens → bytecode
    _ len 0 = ??drop|]
    _ 0 @nth compile-one        // compile first token
    _ swap append               // add to accumulator
    _ 1 strslice compile-loop   // continue with rest
} ;

: compile {
    [] swap compile-loop
} ;

export compile
