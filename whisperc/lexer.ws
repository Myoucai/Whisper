// Whisper Lexer v1.0 — minimal working version

: is-ws   { _ _ _ 32 = ` 9 = | ` 10 = | ` 13 = | } ;
: is-dig  { _ _ 48 >= ` 57 <= & } ;

: ctos { [] ` append charsstr } ;

: next-chunk-acc {
    _ "" streq ??drop ""|]
    striter
    _ -1 = ??drop drop "" ""|]
    _ is-ws ??drop `|ctos ` swap strcat ` ` next-chunk-acc]
} ;

: next-chunk { "" ` next-chunk-acc } ;

: skip-ws {
    _ "" streq ??drop|]
    _ 0 strnth is-ws ??_ 1 strslice skip-ws|]
} ;

: classify {
    _ 0 strnth is-dig ?? drop "int"|drop "word"]
} ;

: pair-up {
    // chunk type → [type, chunk]
    [] ` append ` append
} ;

: tokenize-loop {
    skip-ws
    _ "" streq ??drop|]
    next-chunk
    `                               // rest, chunk, acc
    `                               // chunk, rest, acc
    classify                        // chunk, type, acc
    pair-up                         // [type chunk], rest, acc
    `                               // rest, [type chunk], acc
    `                               // [type chunk], rest, acc
    append                          // rest, new_acc
    `                               // new_acc, rest
    tokenize-loop
} ;

: tokenize {
    [] tokenize-loop
} ;

export tokenize
