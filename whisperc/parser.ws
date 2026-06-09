// Whisper Parser v2.0 — structural grouping in Whisper
// Replaces Rust structify_chunks with Whisper-native implementation.
//
// Pipeline: flat_tokens → group-paired → recognize → structured_tokens
// Tags: 5=Quote({}), 6=List([]), 7=Cond(??..|..]), 8=Loop({}{}#), 9=Def(:nm{};)

// ══════════════════════════════════════════════════════════════════════
// Pass 1: Group paired delimiters { } and [ ] into typed markers
// ══════════════════════════════════════════════════════════════════════

: gp-loop {
    // acc tokens → finished_acc
    _ len 0 = ?? drop ]
    over 0 @nth                   // [acc, tokens, first_tok]
    _ "{" streq ??
        drop drop
        swap 1 + swap             // skip "{"
        [] swap 0 collect-brace   // [acc, inner, rest]
        group-paired-rec          // [acc, inner_grouped, rest]
        [5] swap append           // [acc, [5,inner], rest]
        swap ` swap append        // acc += [5,inner]
        gp-loop                   // continue
    |
    over 0 @nth
    _ "[" streq ??
        drop drop
        swap 1 + swap
        [] swap 0 collect-bracket
        group-paired-rec
        [6] swap append
        swap ` swap append
        gp-loop
    |
        drop drop
        over 0 @nth
        swap ` swap append
        swap 1 + swap
        gp-loop
    ]]
} ;

: group-paired-rec { [] swap gp-loop } ;

// Collect tokens between matching braces, respecting nesting depth
: collect-brace {
    // depth acc tokens → [inner_acc, tokens_after_}]
    _ len 0 = ?? drop swap drop [] swap ]
    over 0 @nth
    _ "{" streq ??
        drop drop
        swap 1 + swap
        over 0 @nth
        swap ` swap append
        swap 1 + swap
        collect-brace
    |
    over 0 @nth
    _ "}" streq ??
        drop drop
        1 -
        _ 0 = ??
            drop drop
            swap 1 + swap
            swap
        |
            over 0 @nth
            swap ` swap append
            swap 1 + swap
            collect-brace
        ]
    |
        drop drop
        over 0 @nth
        swap ` swap append
        swap 1 + swap
        collect-brace
    ]]
} ;

// Collect tokens between matching brackets
: collect-bracket {
    _ len 0 = ?? drop swap drop [] swap ]
    over 0 @nth
    _ "[" streq ??
        drop drop
        swap 1 + swap
        over 0 @nth
        swap ` swap append
        swap 1 + swap
        collect-bracket
    |
    over 0 @nth
    _ "]" streq ??
        drop drop
        1 -
        _ 0 = ??
            drop drop
            swap 1 + swap
            swap
        |
            over 0 @nth
            swap ` swap append
            swap 1 + swap
            collect-bracket
        ]
    |
        drop drop
        over 0 @nth
        swap ` swap append
        swap 1 + swap
        collect-bracket
    ]]
} ;

// ══════════════════════════════════════════════════════════════════════
// Pass 2: Recognize multi-token structural patterns
// ══════════════════════════════════════════════════════════════════════

// Helper: check if tokens match : name {body} ; pattern
: is-def-pat {
    // tokens → bool
    _ len 3 < ?? drop #f ]
    over 0 @nth _ ":" streq ??
        drop
        over 1 @nth _ str? ??
            drop
            over 2 @nth _ list? ??
                drop drop #t
            | drop drop #f ]
        | drop drop #f ]
    | drop drop #f ]
} ;

// Helper: check if tokens match {body} {cond} # pattern
: is-loop-pat {
    _ len 3 < ?? drop #f ]
    over 0 @nth _ list? ??
        drop
        over 1 @nth _ list? ??
            drop
            over 2 @nth _ "#" streq ??
                drop drop #t
            | drop drop #f ]
        | drop drop #f ]
    | drop drop #f ]
} ;

// Process the : name {body} ; pattern
: do-def {
    // acc tokens → [acc, tokens_after]
    swap 1 + swap                 // skip ":"
    over 0 @nth                   // name
    swap 1 + swap                 // skip name
    over 0 @nth                   // [5, body]
    1 @nth                        // body inner
    group-paired-rec
    [] swap rec-loop              // recurse on body
    [9] rot append                // [9, name, body_rec]
    swap ` swap append            // acc += def token
    swap 1 + swap                 // skip body
    swap 1 + swap                 // skip ";"
} ;

// Process the {body} {cond} # pattern
: do-loop {
    // acc tokens → [acc, tokens_after]
    over 0 @nth 1 @nth           // body inner from first list
    group-paired-rec
    [] swap rec-loop              // recurse on body
    swap                          // [acc, tokens, body_rec]
    over 1 @nth 1 @nth           // cond inner from second list
    group-paired-rec
    [] swap rec-loop              // recurse on cond
    [8] rot append swap           // [8, body_rec, cond_rec]
    append                        // [8, body, cond]
    swap ` swap append            // acc += loop token
    swap 3 + swap                 // skip 3 tokens
} ;

// Process the ?? then | else ] pattern
: do-cond {
    // acc tokens → [acc, tokens_after]
    swap 1 + swap                 // skip "??"
    1 [] swap                     // depth=1, then_acc=[]
    split-cond                    // [then, else, rest]
    group-paired-rec swap         // [rest, then_grouped, else]
    group-paired-rec              // [rest, then_grouped, else_grouped]
    [] swap rec-loop swap         // [rest, else_rec, then_grouped]
    [] swap rec-loop              // [rest, else_rec, then_rec]
    [7] rot append swap           // [7, then_rec, else_rec]
    append                        // [7, then, else]
    swap ` swap append            // acc += cond token
} ;

: rec-loop {
    // acc tokens → finished_acc
    _ len 0 = ?? drop ]

    // ── Try: : name {body} ;  →  [9, name, body] ──
    over is-def-pat ??
        drop do-def rec-loop
    |
    // ── Try: {body} {cond} #  →  [8, body, cond] ──
    over is-loop-pat ??
        drop do-loop rec-loop
    |
    // ── Try: ?? ... | ... ]  →  [7, then, else] ──
    over 0 @nth _ "??" streq ??
        drop drop do-cond rec-loop
    |
    // ── Fallback: emit token as-is ──
        drop drop
        over 0 @nth
        swap ` swap append
        swap 1 + swap
        rec-loop
    ]]]
} ;

// split-cond: stub — ??...|...] patterns are handled by the Rust bridge.
// The ] character serves as both list closer and conditional closer,
// making it impossible to parse nested ?? inside [...] in Whisper.
: split-cond {
    // depth then_acc tokens → [then, else, rest]
    // Return empty then/else and all tokens as rest
    drop swap drop
    [] [] rot
} ;

// ══════════════════════════════════════════════════════════════════════
// Entry point
// ══════════════════════════════════════════════════════════════════════

: structify {
    [] swap gp-loop
    [] swap rec-loop
} ;

export structify
