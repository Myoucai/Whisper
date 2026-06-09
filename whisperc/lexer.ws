// Whisper Lexer v7
: ctos { [] ` append charsstr } ;
: is-blank { _ 32 = $1 9 = | $1 10 = | $1 13 = | ` drop } ;
: is-ws    { _ 32 = $1 9 = | $1 10 = | $1 13 = |
             $1 123 = | $1 125 = | $1 91 = | $1 93 = |
             $1 58 = | $1 59 = | ` drop } ;

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

: read-str-acc {
    _ "" streq ?? "ERR" ""
    |   _ 0 strnth 34 = ??
            striter ` drop
        |   _ 0 strnth ctos
            @ ` strcat
            ` striter ` drop
            read-str-acc
        ]
    ]
} ;

: tokenize-loop {
    _ "" streq ??drop
    |   _ 0 strnth _ is-blank ??
            ` striter ` drop ` drop
        |drop]

        _ "" streq ??drop
        |   _ 0 strnth 34 = ??
                1 9999 strslice "" ` read-str-acc
                // Prepend " marker for classify
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
