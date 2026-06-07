// Whisper Lexer v2 — enhanced tokenizer
// Based on working v1.0.  Delimiters split into own tokens.
// Strings "..." handled as single tokens (content without quotes).

: ctos { [] ` append charsstr } ;

// Whitespace OR structural delimiter (splits chunks)
: is-ws   { _ _ 32 = ` 9 = | ` 10 = | ` 13 = |
            ` 123 = | ` 125 = | ` 91 = | ` 93 = |
            ` 58 = | ` 59 = | } ;

: is-dig  { _ _ 48 >= ` 57 <= & } ;

// Skip whitespace only (not delimiters)
: is-blank { _ _ 32 = ` 9 = | ` 10 = | ` 13 = | } ;

: skip-ws {
    _ "" streq ??drop|]
    _ 0 strnth is-blank ??_ 1 strslice skip-ws|]
} ;

// Read until whitespace/delimiter
: next-chunk-acc {
    _ "" streq ??drop ""|]
    striter
    _ -1 = ??drop drop "" ""|]
    _ is-ws ??drop | ctos ` strcat ` ` next-chunk-acc]
} ;

: next-chunk { "" ` next-chunk-acc } ;

// Read string content until closing "
: read-str-loop {
    striter
    _ -1 = ??drop drop "ERR:unterminated" ""|]
    _ 34 = ??` drop|]
    ctos ` strcat _ 1 strslice read-str-loop
} ;

// Classify chunk → type_str for non-string/delimiter chunks
: classify {
    _ 0 strnth is-dig ?? drop "int"|drop "word"]
} ;

: pair-up {
    ` [] ` append ` append
} ;

// Main loop: special-cases strings before chunk processing
: tokenize-loop {
    skip-ws
    _ "" streq ??drop|]

    // String: read "..." as single token
    _ 0 strnth 34 = ??
        1 strslice "" ` read-str-loop
        ` "str" pair-up
        ` ` append `
        tokenize-loop
    |]

    // Regular: next-chunk → classify → pair-up → append → recurse
    next-chunk
    `
    `
    classify
    pair-up
    `
    `
    append
    `
    tokenize-loop
} ;

: tokenize {
    [] ` tokenize-loop
} ;

export tokenize
