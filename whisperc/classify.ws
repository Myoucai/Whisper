// Whisper Classifier v2 — nested ??...|...] tree
// Each ?? needs one matching ] at the end.

: is-digit { _ 48 >= $1 57 <= & ` drop } ;

: is-num-str {
    _ "" streq ?? drop #f ]
    _ 0 strnth 45 = ??
        1 9999 strslice _ "" streq ?? drop drop #f ]
        1 9999 strslice _ 0 strnth is-digit
    |   drop _ 0 strnth is-digit
    ]
} ;

: classify-one {
    _ "#t"  streq ?? drop [13] 1 append
    | _ "#f"  streq ?? drop [13] 0 append
    | _ "+"   streq ?? drop [3] 16 append
    | _ "-"   streq ?? drop [3] 17 append
    | _ "*"   streq ?? drop [3] 18 append
    | _ "/"   streq ?? drop [3] 19 append
    | _ "%"   streq ?? drop [3] 20 append
    | _ "="   streq ?? drop [3] 24 append
    | _ "<"   streq ?? drop [3] 25 append
    | _ ">"   streq ?? drop [3] 26 append
    | _ "&"   streq ?? drop [3] 32 append
    | _ "|"   streq ?? drop [3] 33 append
    | _ "!"   streq ?? drop [3] 34 append
    | _ "_"   streq ?? drop [3] 0 append
    | _ "@"    streq ?? drop [3] 3 append
    | _ "@nth" streq ?? drop [3] 64 append
    | _ "@map" streq ?? drop [3] 67 append
    | _ "@each" streq ?? drop [3] 68 append
    | _ "@fold" streq ?? drop [3] 69 append
    | _ "@times" streq ?? drop [3] 83 append
    | _ "."   streq ?? drop [3] 144 append
    | _ ".."  streq ?? drop [3] 145 append
    | _ ","   streq ?? drop [3] 146 append
    | _ ":"   streq ?? drop [3] 160 append
    | _ ";"   streq ?? drop [3] 161 append
    | _ "`"   streq ?? drop [3] 1 append
    | _ "??"  streq ?? drop [3] 80 append
    | _ "?|"  streq ?? drop [3] 129 append
    | _ "?->" streq ?? drop [3] 80 append
    | _ "#"   streq ?? drop [3] 83 append
    | _ "["   streq ?? drop [3] 0 append
    | _ "]"   streq ?? drop [3] 0 append
    | _ "$"   streq ?? drop [3] 4 append
    | _ "dup"   streq ?? drop [3] 0 append
    | _ "drop"  streq ?? drop [3] 2 append
    | _ "mod"   streq ?? drop [3] 20 append
    | _ "len"   streq ?? drop [3] 66 append
    | _ "append" streq ?? drop [3] 65 append
    | _ "times"  streq ?? drop [3] 83 append
    | _ "return" streq ?? drop [3] 97 append
    | _ "import" streq ?? drop [3] 162 append
    | _ "export" streq ?? drop [3] 163 append
    | _ "strlen"  streq ?? drop [3] 70 append
    | _ "strcat"  streq ?? drop [3] 71 append
    | _ "strslice" streq ?? drop [3] 72 append
    | _ "streq"   streq ?? drop [3] 73 append
    | _ "strlt"   streq ?? drop [3] 74 append
    | _ "strfind" streq ?? drop [3] 75 append
    | _ "strreplace" streq ?? drop [3] 76 append
    | _ "strtoi64" streq ?? drop [3] 77 append
    | _ "i64tostr" streq ?? drop [3] 78 append
    | _ "strnth"   streq ?? drop [3] 79 append
    | _ "strchars" streq ?? drop [3] 184 append
    | _ "charsstr" streq ?? drop [3] 185 append
    | _ "striter"  streq ?? drop [3] 186 append
    | _ "listfind" streq ?? drop [3] 187 append
    | _ "strjoin"  streq ?? drop [3] 188 append
    | _ "i64tof64" streq ?? drop [3] 176 append
    | _ "f64toi64" streq ?? drop [3] 177 append
    | _ "fsqrt"    streq ?? drop [3] 178 append
    | _ "fsin"     streq ?? drop [3] 179 append
    | _ "fcos"     streq ?? drop [3] 180 append
    | _ "ftan"     streq ?? drop [3] 181 append
    | _ "json-parse"     streq ?? drop [3] 182 append
    | _ "json-stringify" streq ?? drop [3] 183 append
    | _ is-num-str ?? [0] ` strtoi64 append
    | [4] ` append
    ]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]
} ;

: classify { { classify-one } @map } ;

// ── Recursive classification for structured token trees ─────────────
// Each handler takes [list] on the stack and returns [classified_list].
// "over" is used to access the original list without consuming it.

: cl-quote {
    over 1 @nth classify-nested   // [list, inner_c]
    [5] swap append               // [list, [5, inner_c]]
    swap drop
} ;
: cl-list {
    over 1 @nth { classify-nested } @map
    [6] swap append
    swap drop
} ;
: cl-cond {
    over 1 @nth classify-nested   // [list, then_c]
    over 2 @nth classify-nested   // [list, then_c, else_c]
    [7] swap append append        // [7, then_c, else_c]
    swap drop
} ;
: cl-loop {
    over 1 @nth classify-nested
    over 2 @nth classify-nested
    [8] swap append append
    swap drop
} ;

: classify-nested {
    _ list? ??
        _ len 0 = ?? ]
        dup 0 @nth _ i64? ??
            // Tagged list — dispatch on tag
            over 0 @nth _ 5 = ??
                drop cl-quote
            | over 0 @nth _ 6 = ??
                drop cl-list
            | over 0 @nth _ 7 = ??
                drop cl-cond
            | over 0 @nth _ 8 = ??
                drop cl-loop
            | over 0 @nth _ 9 = ??
                drop
                // Special handling for Def: don't classify the name
                // [9, name, body] → [9, name, body_classified]
                dup 2 @nth classify-nested   // classify body
                // Stack: [list, body_c]
                // Need to rebuild: [9, original_name, body_c]
                // original_name = list[1]
                over 1 @nth                  // [list, body_c, name]
                rot                          // [body_c, name, list]
                drop                         // [body_c, name]
                [9] swap append              // [[9, name], body_c]
                // Hmm, [9] + name = [9, name], then append body_c = [9, name, body_c]
                // But append is: list elem → new_list
                // So [9] name append → [9, name]. Good.
                // Then [9, name] body_c append → [9, name, body_c]. Good!
                append                       // [9, name, body_c]
            | drop                           // unknown tag
            ]]]]]
        |
            // Untagged list — classify each element
            drop
            { classify-nested } @map
        ]
    |
        // Not a list
        _ str? ??
            _ 0 strnth 34 = ??
                // String literal (starts with ") — wrap as [2, content]
                drop
                1 9999 strslice
                [2] swap append
            |
                // Regular string token — classify via classify-one
                classify-one
            ]
        |
            // Non-string, non-list — pass through
        ]
    ]
} ;

export classify
export classify-one
export classify-nested
