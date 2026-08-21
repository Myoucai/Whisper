// main.ws — Compiler driver and pipeline assembly
// NOTE: compile-one, compile, compile-one helpers are in compile.ws.
// This file only defines the full pipeline (compile-full) and exports.

// ── Full compilation pipeline ───────────────────────────────────────
// source_string → [main_bytecodes, [[name, def_bytecodes], ...]]

: classify-full { tokenize structify classify-nested } ;

: compile-full {
    classify-full               // tokenize → structify → classify
    compile                     // compile all tokens → bytecode
    [] 2 PushList               // [bytecodes, empty_defs]
} ;

export compile-full
export classify-full
