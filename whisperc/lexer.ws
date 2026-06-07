// ── Whisper Lexer v1.0 ───────────────────────────────────────────────
// Tokenizer for self-hosting compiler.
// Usage:  "3 4 +" tokenize  →  [["int" "3"] ["int" "4"] ["op" "+"]]
//
// Conditional syntax reminder:
//   ??then|else]  — two-branch (both required)
//   ??then|]      — if-then (empty else)

// ── Char predicates (on i64 char codes) ───────────────────────────────

: spacep   { 32 = } ;
: tabp     { 9  = } ;
: nlp      { 10 = } ;
: crp      { 13 = } ;
: wsp      { _ spacep ` tabp | ` nlp | ` crp | } ;
: digp   { 48 >= ` 57 <= & } ;
: alphap   { _ 65 >= ` 90 <= & _ 97 >= ` 122 <= & | } ;
: op1st  {
    _ 43 = ` 45 = | ` 42 = | ` 47 = |
    ` 61 = | ` 60 = | ` 62 = |
    ` 33 = | ` 38 = | ` 124 = |
    ` 95 = | ` 96 = |
    ` 46 = | ` 44 = | ` 35 = | ` 63 = |
    ` 36 = | ` 37 = |
} ;

// ── Single char → string ──────────────────────────────────────────────

: ctos { [] swap append charsstr } ;

// ── Read one non-ws chunk ─────────────────────────────────────────────
// acc source → chunk rest

: next-chunk-acc {
    _ "" streq ??drop swap|]
    striter
    _ -1 = ??drop drop swap swap|]
    _ wsp ??drop swap|ctos swap strcat swap next-chunk-acc]
} ;

: next-chunk { "" swap next-chunk-acc } ;

// ── Skip leading whitespace ───────────────────────────────────────────

: skip-ws {
    _ "" streq ??""|]
    _ 0 strnth wsp ??_ 1 strslice skip-ws|]
} ;

// ── Keyword table ─────────────────────────────────────────────────────

: kw-table {
    [
        ["dup" "kw"]  ["swap" "kw"]  ["drop" "kw"]
        ["mod" "kw"]  ["len"  "kw"]  ["append" "kw"]
        ["import" "kw"] ["export" "kw"]
        ["strlen" "kw"] ["strcat" "kw"] ["strslice" "kw"]
        ["streq" "kw"]  ["strlt"  "kw"] ["strfind" "kw"]
        ["strreplace" "kw"] ["strtoi64" "kw"] ["i64tostr" "kw"]
        ["i64tof64" "kw"] ["f64toi64" "kw"]
        ["fsqrt" "kw"] ["fsin" "kw"] ["fcos" "kw"] ["ftan" "kw"]
        ["json-parse" "kw"] ["json-stringify" "kw"]
        ["striter" "kw"] ["listfind" "kw"] ["strjoin" "kw"]
        ["bytes-new" "kw"] ["bytes-push" "kw"]
        ["bytes-len" "kw"] ["bytes-write" "kw"]
        ["try" "kw"]
    ]
} ;

// ── Classify one chunk ────────────────────────────────────────────────

: classify {
    _ "" streq ?? drop "eof" ""|]
    _ 0 strnth digp _ 0 strnth 45 = |
    ??_ "." strfind 0 >= ??"float"|"int" ]|]
    _ 0 strnth 34 = ??"str"|]
    _ 0 strnth op1st ??"op"|]
    _ kw-table swap listfind ?? drop "word"|]
    swap [""] swap strcat
} ;

// ── Tokenize loop ─────────────────────────────────────────────────────

: tokenize-loop {
    skip-ws
    _ "" streq ??drop|]
    next-chunk
    swap swap
    classify
    swap
    append
    swap
    tokenize-loop
} ;

// ── Entry point ───────────────────────────────────────────────────────

: tokenize {
    [] tokenize-loop
} ;

export tokenize
export classify
export next-chunk
export kw-table
