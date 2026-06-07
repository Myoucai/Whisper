// Whisper Lexer v5 — uses @ (Rot) for clean stack ops
// Produces flat list of string chunks from source.

: ctos { [] ` append charsstr } ;

: is-blank { _ 32 = $1 9 = | $1 10 = | $1 13 = | ` drop } ;
: is-ws    { _ 32 = $1 9 = | $1 10 = | $1 13 = |
             $1 123 = | $1 125 = | $1 91 = | $1 93 = |
             $1 58 = | $1 59 = | ` drop } ;

// ── Chunk reader: acc src → chunk rest ──
// Stops at ws/delim, returns accumulated string + remaining source.

: read-chunk-acc {
    _ "" streq ??|]                   // src empty → return acc src
    _ 0 strnth is-ws ??|]             // ws/delim → return acc src
    _ 0 strnth ctos                   // acc src → acc src ch_str
    @ ` strcat                        // rot→src ch_str acc; swap→src acc ch_str; strcat→src new_acc
    striter drop                       // new_acc src[1:]
    read-chunk-acc
} ;

: read-chunk { .. "" ` read-chunk-acc } ;

// ── String reader: acc src → content rest ──
: read-str-acc {
    _ "" streq ?? "ERR" ""|]           // unterminated
    _ 0 strnth 34 = ??                 // closing " ?
        striter drop                   // drop the ", keep rest → acc rest
    |]
    _ 0 strnth ctos
    @ ` strcat
    striter drop
    read-str-acc
} ;

: read-str { "" ` read-str-acc } ;

// ── Tokenize loop ─────────────────────────────────────────────────────
// State: acc(tokens) src → token_list

: tokenize-loop {
    // Skip blanks
    _ "" streq ??drop|]
    _ 0 strnth _ is-blank ??` drop _ 1 9999 strslice tokenize-loop|drop]

    // Done
    _ "" streq ??drop|]

    // String?
    _ 0 strnth 34 = ??
        _ 1 9999 strslice read-str    // acc src content rest
        @ drop                         // acc content rest  (rot→content on top, drop src)
        @ @ append                     // rest [acc..., content]
        ` tokenize-loop               // new_acc rest
    |

    // Regular chunk or delimiter
    read-chunk                        // acc chunk rest
    @ @ append                         // rest [acc..., chunk]
    ` tokenize-loop                   // new_acc rest
    ]
} ;

: tokenize {
    [] ` tokenize-loop
} ;

export tokenize
