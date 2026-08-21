// Whisper Parser v5.0 — guarded checks, max 2-deep ?? per function
// Pipeline: flat_tokens → structify → structured_tokens
// Tags: 5=Quote({}), 6=List([]), 7=Cond, 8=Loop, 9=Def

: tok-advance { dup len 1 - 1 swap list-slice } ;

: advance-by {
    dup 0 = ?? drop return ]
    { swap tok-advance swap 1 - }
    { dup 0 > }
    #
    drop
} ;

// ══════════════════════════════════════════════════════════════════════
// Pass 1: Group paired delimiters {} and []
// ══════════════════════════════════════════════════════════════════════

: group-paired-rec { [] swap gp-loop } ;

: gp-loop {
    dup len 0 = ?? drop return ]
    dup 0 @nth
    dup "{" streq ??
        drop tok-advance
        collect-brace
        swap [5] swap append
        rot swap append swap
        gp-loop
    |
    dup "[" streq ??
        drop tok-advance
        collect-bracket
        swap [6] swap append
        rot swap append swap
        gp-loop
    |
        rot swap append swap
        tok-advance
        gp-loop
    ]
} ;

: collect-brace { [] swap cb-loop } ;

: cb-loop {
    dup len 0 = ?? drop return ]
    dup 0 @nth
    dup "{" streq ??
        drop tok-advance
        collect-brace
        swap [5] swap append
        rot swap append swap
        cb-loop
    |
    dup "}" streq ??
        drop tok-advance return
    |
        rot swap append swap
        tok-advance
        cb-loop
    ]
} ;

: collect-bracket { [] swap cbr-loop } ;

: cbr-loop {
    dup len 0 = ?? drop return ]
    dup 0 @nth
    dup "[" streq ??
        drop tok-advance
        collect-bracket
        swap [6] swap append
        rot swap append swap
        cbr-loop
    |
    dup "]" streq ??
        drop tok-advance return
    |
        rot swap append swap
        tok-advance
        cbr-loop
    ]
} ;

// ══════════════════════════════════════════════════════════════════════
// split-cond is a native opcode (0xF0) in the seed compiler.
// It takes (tokens start_idx) and returns [then_tokens, else_tokens, next_idx].
// ══════════════════════════════════════════════════════════════════════
// recognize: handle ??, loops, defs, export/import
// Each function has max 2-deep ?? nesting (the seed segfaults on 5+).
// We use guarded helper functions to avoid deep nesting.
// ══════════════════════════════════════════════════════════════════════

: recognize { [] swap rec-loop } ;

// is-tag-5?: check if value is tagged list with tag 5
: is-tag-5? { dup list? ?? 0 @nth 5 = | drop 0 ] } ;

// try-directive: handle export/import
: try-directive {
    dup len 2 < ?? 0 return ]
    dup 0 @nth dup "export" streq ??
        drop tok-advance tok-advance 1 return
    |
    "import" streq ??
        tok-advance tok-advance 1 return
    |
        0
    ]
} ;

// try-def: handle ": name {body} ;"
// Uses guarded checks — each condition checked before accessing deeper fields.
: try-def-colon {
    // Stack: [acc, tokens]. tokens[0]=":", tokens[2] is tag-5 list
    // is-tag-5? consumed tokens[2], so stack is [acc, tokens]
    dup 3 @nth ";" streq ??
        // Full match! Stack: [acc, tokens]
        dup 1 @nth                  // acc, tokens, name
        over 2 @nth 1 @nth         // acc, tokens, name, body_inner
        structify                   // acc, tokens, name, body_s
        9 swap rot swap 3 PushList  // acc, tokens, [9, name, body_s]
        rot swap append swap        // acc', tokens
        tok-advance tok-advance tok-advance tok-advance
        1 return
    |
        0
    ]
} ;

: try-def {
    dup len 4 < ?? 0 return ]
    dup 0 @nth ":" streq ?? try-def-colon | 0 ]
} ;

// try-loop: handle "{body} {cond} #"
: try-loop-inner {
    // Stack: [acc, tokens]. tokens[0] is tag-5, checked by caller
    dup 1 @nth is-tag-5? ??
        dup 2 @nth "#" streq ??
            // Full match! Stack: [acc, tokens]
            dup 0 @nth 1 @nth structify   // acc, tokens, body_s
            over 1 @nth 1 @nth structify  // acc, tokens, body_s, cond_s
            8 swap rot swap 3 PushList     // acc, tokens, [8, body_s, cond_s]
            rot swap append swap           // acc', tokens
            tok-advance tok-advance tok-advance
            1 return
        ]
        0 return
    |
        0
    ]
} ;

: try-loop {
    dup len 3 < ?? 0 return ]
    dup 0 @nth is-tag-5? ?? try-loop-inner | 0 ]
} ;

// rec-loop: main recognize loop
// We have 5 cases. To avoid 5-deep ?? in one function,
// we handle the first 3 cases (which involve function calls)
// then the last 2 inline.
: handle-regular {
    // Stack: [acc, tokens] — regular token case
    dup 0 @nth rot swap append swap
    tok-advance
    rec-loop
} ;

: handle-cond {
    // Stack: [acc, tokens] — tokens[0] is "??"
    tok-advance              // [acc, tokens']
    dup 0 split-cond         // [acc, tokens', result]
    // Build cond structure from result
    dup 0 @nth structify     // [acc, tokens', result, then_s]
    over 1 @nth structify    // [acc, tokens', result, then_s, else_s]
    7 swap rot swap 3 PushList  // [acc, tokens', result, cond]
    // Get next_idx, discard result
    over 2 @nth              // [acc, tokens', result, cond, next_idx]
    swap rot drop            // [acc, tokens', next_idx, cond]
    // Append cond to acc
    $3 swap append           // [acc, tokens', next_idx, acc']
    // Advance tokens' by next_idx
    swap $2 swap advance-by  // [acc, tokens', acc', tokens'']
    // Drop old acc and tokens', keep acc' and tokens''
    swap rot drop swap rot drop  // [acc', tokens'']
    rec-loop
} ;

: rec-loop {
    dup len 0 = ?? drop return ]

    // Try directive (export/import)
    try-directive ?? rec-loop return ]

    // Try def (: name {body} ;)
    try-def ?? rec-loop return ]

    // Try loop ({body} {cond} #)
    try-loop ?? rec-loop return ]

    // Try cond (?? ... ])
    dup 0 @nth "??" streq ?? handle-cond return ]

    // Regular token
    handle-regular
} ;

// ══════════════════════════════════════════════════════════════════════
// structify: group braces/brackets, then recognize patterns
// ══════════════════════════════════════════════════════════════════════

: structify { group-paired-rec recognize } ;

export structify
export recognize
export group-paired-rec
