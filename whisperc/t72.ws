// Whisper Lexer v1.0

: spacep   { 32 = } ;
: tabp     { 9  = } ;
: nlp      { 10 = } ;
: crp      { 13 = } ;
: wsp      { _ spacep ` tabp | ` nlp | ` crp | } ;
: digp     { 48 >= ` 57 <= & } ;
: alphap   { _ 65 >= ` 90 <= & _ 97 >= ` 122 <= & | } ;
: op1st    { _ 43 = ` 45 = | ` 42 = | ` 47 = | ` 61 = | ` 60 = | ` 62 = | ` 33 = | ` 38 = | ` 124 = | ` 95 = | ` 96 = | ` 46 = | ` 44 = | ` 35 = | ` 63 = | ` 36 = | ` 37 = | } ;

: ctos { [] ` append charsstr } ;

: next-chunk-acc {
    _ "" streq ??drop ""|]
    striter
    _ -1 = ??drop drop "" ""|]
    _ wsp ??drop `|]
    ctos rot strcat ` next-chunk-acc]
} ;

: next-chunk { "" ` next-chunk-acc } ;

: skip-ws {
    _ "" streq ??drop|]
    _ 0 strnth wsp ??_ 1 strslice skip-ws|]
} ;

: kw-table {
    [
        ["dup" "kw"]  ["swap" "kw"]  ["drop" "kw"]
        ["mod" "kw"]  ["len"  "kw"]  ["append" "kw"]
        ["import" "kw"] ["export" "kw"]
        ["try" "kw"]
    ]
} ;

: classify {
    _                               // save chunk copy
    _ 0 strnth digp                 // test first char
    ?? drop drop "int"|drop "word"] // produce type
    ` [""] ` strcat                 // pair type with chunk
} ;

: tokenize-loop {
    skip-ws
    _ "" streq ??drop|]
    next-chunk
    classify
    ` `
    append
    `
    tokenize-loop
} ;

: tokenize {
    [] tokenize-loop
} ;

export tokenize
