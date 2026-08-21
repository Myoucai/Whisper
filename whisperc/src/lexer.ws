// ── Whisper Lexer v8 (bootstrap) ────────────────────────────────────
// source string → flat token list (strings)
//   * // comments skipped to end of line
//   * "..." literals read with \" \\ \n \t \r escapes, stored with a
//     leading '"' marker so classify can tell them from words
//   * { } [ ] ; : | are single-char tokens
//   * ?? splits as two ? tokens (both → opcode 80, functionally identical)
//   * # splits unless #t/#f (not handled in this version — not needed
//     for self-hosting since #t/#f only appear in classify.ws/lexer.ws)

: ctos { [] ` append charsstr } ;

: is-blank { _ 32 = $1 9 = | $1 10 = | $1 13 = | ` drop } ;

: is-delim {
    _ 123 = $1 125 = | $1 91 = | $1 93 = |
    $1 58 = | $1 59 = | $1 124 = | ` drop
} ;

: is-ws { _ is-blank $1 is-delim | ` drop } ;

: esc-char {
    _ 110 = ?? drop 10
    | _ 116 = ?? drop 9
    | _ 114 = ?? drop 13
    | _ 48 = ?? drop 0
    | ]
    ]]]
} ;

// read-str-acc: acc s → acc' rest'
: read-str-acc {
    _ "" streq ?? "ERR" ""
    |   _ 0 strnth 34 = ??
            striter ` drop
        |   _ 0 strnth 92 = ??
                1 9999 strslice dup 0 strnth esc-char ctos
                rot strcat swap 1 9999 strslice read-str-acc
        |   _ 0 strnth ctos
            @ ` strcat
            ` striter ` drop
            read-str-acc
        ]
    ]
    ]
} ;

// read-chunk-acc: acc s → acc' rest'   (accumulate until stop char)
: read-chunk-acc {
    _ "" streq ??
    |   _ 0 strnth is-ws ??
        |   _ 0 strnth ctos
            @ ` strcat
            ` striter ` drop
            read-chunk-acc
        ]
    ]
} ;

// tokenize-loop: acc s → acc'
: tokenize-loop {
    _ "" streq ?? drop
    |   _ 0 strnth is-blank ??
            striter ` drop tokenize-loop
        |   _ 0 strnth 34 = ??
                1 9999 strslice "" ` read-str-acc
                ` "\"" ` strcat `
                @ @ append ` tokenize-loop
            |   _ 0 strnth is-ws ??
                    striter ` ctos
                    @ ` append ` tokenize-loop
                |   "" ` read-chunk-acc
                    @ @ append ` tokenize-loop
                ]
            ]
        ]
    ]
} ;

: tokenize { [] ` tokenize-loop } ;

export tokenize
export ctos
