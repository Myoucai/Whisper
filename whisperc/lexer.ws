// Whisper Lexer v2 — returns string chunks from source
// Delimiters { } [ ] : ; are split as separate tokens.

: ctos { [] ` append charsstr } ;

// Whitespace OR delimiter (splits chunks)
: is-ws   { _ _ 32 = ` 9 = | ` 10 = | ` 13 = |
            ` 123 = | ` 125 = | ` 91 = | ` 93 = |
            ` 58 = | ` 59 = | } ;

// Skip blanks only
: is-blank { _ _ 32 = ` 9 = | ` 10 = | ` 13 = | } ;

: skip-ws {
    _ "" streq ??drop|]
    _ 0 strnth is-blank ??_ 1 strslice skip-ws|]
} ;

// Read until ws/delim; returns chunk and rest
: next-chunk-acc {
    _ "" streq ??drop ""|]
    _ 0 strnth is-ws ??drop |]
    _ 0 strnth ctos ` strcat _ 1 strslice next-chunk-acc
} ;

: next-chunk { "" ` next-chunk-acc } ;

// String reader: src (after ") → [content, rest]
: read-str {
    _ "" streq ??drop "ERR:unterminated"|]
    _ 0 strnth 34 = ??_ 1 strslice|]
    _ 0 strnth ctos ` strcat _ 1 strslice read-str
} ;

// tokenize-loop: acc src → chunk_list
// stack: [acc, src]
: tokenize-loop {
    skip-ws                             // acc src(cleaned)
    _ "" streq ??drop|]                 // done: return acc

    // String: read until closing "
    _ 0 strnth 34 = ??
        _ 1 strslice read-str           // acc src content rest_after
        `                               // acc src rest_after content
        $3                              // acc src rest_after content acc (copy acc)
        `                               // acc src rest_after acc content
        append                          // acc src rest_after [acc..., content]
        `                               // acc src new_acc rest_after
        `                               // acc new_acc src rest_after
        drop                            // acc new_acc rest_after  (drop src)
        `                               // acc rest_after new_acc
        `                               // new_acc acc rest_after
        drop                            // new_acc rest_after
        tokenize-loop                   // recurse
    |]

    // Regular chunk
    next-chunk                          // acc chunk rest
    `                                   // acc rest chunk  (chunk on top)
    $2                                  // acc rest chunk acc (copy acc)
    `                                   // acc rest acc chunk
    append                              // acc rest [acc..., chunk]
    `                                   // acc new_acc rest
    `                                   // new_acc acc rest
    drop                                // new_acc rest
    tokenize-loop                       // recurse
} ;

: tokenize {
    [] swap tokenize-loop
} ;

export tokenize
