// ── Whisper Classifier (bootstrap) ──────────────────────────────────
// word string → [type, value]; token tree → classified tree
//   0=i64 1=f64 2=str 3=op 4=word 13=bool 15=pick(N)

: is-digit { dup 48 >= swap 57 <= & } ;

: check-rest {
    dup "" streq ??
        drop #f
    | 0 strnth is-digit
    ]
} ;

: is-num-str {
    _ "" streq ?? drop #f
    | dup 0 strnth 45 = ??
        1 9999 strslice check-rest
    | 0 strnth is-digit
    ]
    ]
} ;

: is-$n? { _ 0 strnth 36 = ?? 1 9999 strslice is-num-str | drop #f ] } ;

// ── Table-driven opcode lookup ──────────────────────────────────────
// Entry format: [opcode, name_str]  — linear scan, O(N) but N≤50 is fine.

: op-table { [
    [16 "+"] [17 "-"] [18 "*"] [19 "/"] [20 "%"]
    [24 "="] [25 "<"] [26 ">"] [27 "!="] [28 "<="] [29 ">="]
    [32 "&"] [33 "|"] [34 "!"]
    [0 "_"] [3 "@"] [1 "`"]
    [144 "."] [145 ".."] [146 ","]
    [160 ":"] [161 ";"]
    [80 "??"] [83 "#"] [4 "$"]
    [0 "dup"] [2 "drop"] [1 "swap"] [3 "rot"] [4 "over"]
    [97 "return"] [66 "len"] [65 "append"] [222 "prepend"]
    [64 "@nth"] [67 "@map"] [68 "@each"] [69 "@fold"]
    [5 "@filter"] [83 "@times"]
    [70 "strlen"] [71 "strcat"] [72 "strslice"] [73 "streq"]
    [74 "strlt"] [75 "strfind"] [76 "strreplace"]
    [77 "strtoi64"] [78 "i64tostr"] [79 "strnth"]
    [6 "strsplit"] [7 "ctos"] [8 "list-slice"]
    [184 "strchars"] [185 "charsstr"] [186 "striter"]
    [187 "listfind"] [188 "strjoin"]
    [176 "i64tof64"] [177 "f64toi64"]
    [178 "fsqrt"] [179 "fsin"] [180 "fcos"] [181 "ftan"]
    [212 "floor"] [213 "ceil"] [214 "round"]
    [215 "log"] [216 "exp"] [217 "atan2"]
    [218 "bitand"] [219 "bitor"] [220 "bitxor"] [221 "bitnot"]
    [9 "shl"] [10 "shr"]
    [223 "sort"] [224 "rev"] [225 "dedup"]
    [226 "take"] [227 "drop-n"]
    [194 "map-new"] [195 "map-get"] [196 "map-set"]
    [197 "map-del"] [198 "map-keys"] [199 "map-has"]
    [208 "now"] [209 "sleep"] [210 "random"] [211 "randint"]
    [11 "args"]
    [228 "list?"] [229 "i64?"] [230 "str?"] [231 "bool?"]
    [232 "read-all"]
    [52 "PushList"]
    [234 "buf-new"] [235 "buf-push"]
] } ;

: table-lookup-helper {
    // Stack: word table idx → opcode or -1
    $1 $1 @nth              // word table idx entry
    dup 1 @nth              // word table idx entry name
    $4 streq                // word table idx entry match
    ??
        // Found: extract opcode and return
        0 @nth              // word table idx opcode
        swap drop swap drop swap drop
        return
    |
        // Not found: advance index
        drop                // word table idx
        1 +                 // word table (idx+1)
        dup $2 len <        // word table (idx+1) in_bounds?
        ??
            table-lookup-helper
        |
            drop drop drop -1
        ]
    ]
} ;
: table-lookup {
    // table word → opcode or -1
    swap 0 table-lookup-helper
} ;

: classify-one {
    _ "#t"  streq ?? drop [13] 1 append
    | _ "#f"  streq ?? drop [13] 0 append
    | op-table $1 table-lookup
        dup -1 = ??
            // Not in table — try $N / number / word
            drop
            _ is-$n?  ?? [15] ` 1 9999 strslice strtoi64 append
            | _ is-num-str ?? [0] ` strtoi64 append
            | [4] ` append
            ]]
        | // Found in table — emit [3, opcode]
            swap drop [3] swap append
        ]
    ]]
} ;

// ── classify: map classify-one over tokens (no @map, for self-hosting) ──
: classify-loop {
    dup len 0 = ?? drop return ]
    dup 0 @nth classify-one   // [acc, tokens, classified]
    rot swap append swap       // [acc', tokens]
    tok-advance                // [acc', tokens']
    classify-loop
} ;
: classify { [] swap classify-loop } ;

// ── Recursive classification for structured token trees ─────────────

: cl-quote {
    dup 1 @nth classify-nested
    swap drop [5] swap append
} ;
: cl-list {
    dup 1 @nth classify-nested
    swap drop [6] swap append
} ;
: cl-cond {
    dup 1 @nth classify-nested
    over 2 @nth classify-nested
    rot drop [7] rot append swap append
} ;
: cl-loop {
    dup 1 @nth classify-nested
    over 2 @nth classify-nested
    rot drop [8] rot append swap append
} ;
: cl-def {
    dup 2 @nth classify-nested
    over 1 @nth
    rot drop [9] swap append swap append
} ;

// cn-map-loop: map classify-nested over list elements (no @map)
: cn-map-loop {
    dup len 0 = ?? drop return ]
    dup 0 @nth classify-nested  // [acc, tokens, classified]
    rot swap append swap         // [acc', tokens]
    tok-advance                  // [acc', tokens']
    cn-map-loop
} ;

: classify-nested {
    _ list? ??
        dup len 0 > ??
            dup 0 @nth
            _ 5 = ?? drop cl-quote
            | _ 6 = ?? drop cl-list
            | _ 7 = ?? drop cl-cond
            | _ 8 = ?? drop cl-loop
            | _ 9 = ?? drop cl-def
            | drop [] swap cn-map-loop
            ]]]]]
        |
        ]
    |
        _ str? ??
            _ 0 strnth 34 = ??
                1 9999 strslice [2] swap append
            | classify-one
            ]
        |
            ]
    ]
} ;

export classify
export classify-one
export classify-nested
