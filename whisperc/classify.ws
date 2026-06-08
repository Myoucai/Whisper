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
    | _ "@"   streq ?? drop [3] 3 append
    | _ "."   streq ?? drop [3] 144 append
    | _ ".."  streq ?? drop [3] 145 append
    | _ ","   streq ?? drop [3] 146 append
    | _ ":"   streq ?? drop [3] 160 append
    | _ ";"   streq ?? drop [3] 161 append
    | _ "`"   streq ?? drop [3] 1 append
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
    ]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]
} ;

: classify { { classify-one } @map } ;

export classify
export classify-one
