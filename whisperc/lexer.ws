// Whisper Lexer v2 — returns string chunks from source
// Delimiters { } [ ] : ; are split as separate tokens.

: ctos { [] ` append charsstr } ;

// Whitespace OR delimiter (splits chunks) — uses $1 not swap
: is-ws   { 32 = $1 9 = | $1 10 = | $1 13 = |
            $1 123 = | $1 125 = | $1 91 = | $1 93 = |
            $1 58 = | $1 59 = | } ;

// Skip blanks only (uses $1 not swap to avoid underflow)
: is-blank { 32 = $1 9 = | $1 10 = | $1 13 = | } ;

: skip-ws {
    _ "" streq ??|]
    _ 0 strnth _ is-blank ??striter drop skip-ws|drop]
} ;

// Read until ws/delim; returns chunk and rest
: next-chunk {
    _ 0 strnth is-ws ??
        striter ctos `              // delim_str rest
    |   "" ` next-chunk-acc         // regular
    ]
} ;

: next-chunk-acc {
    _ "" streq ??|]
    _ 0 strnth is-ws ??|]
    _ 0 strnth ctos               // outer_acc acc src ch → outer_acc acc src ch_str
    $3 ` strcat                    // outer_acc acc src ch_str acc → outer_acc acc src new_acc
    `                              // outer_acc acc new_acc src
    `                              // outer_acc new_acc acc src
    drop                           // outer_acc new_acc src
    striter drop                   // outer_acc new_acc src[1:]
    next-chunk-acc
} ;

// String reader: acc src → content rest (stops at closing ")
: read-str-acc {
    _ "" streq ??"ERR" ""|]
    _ 0 strnth 34 = ??striter drop|]
    _ 0 strnth ctos               // outer_acc acc src ch → outer_acc acc src ch_str
    $3 ` strcat                    // outer_acc acc src new_acc
    `                              // outer_acc acc new_acc src
    `                              // outer_acc new_acc acc src
    drop                           // outer_acc new_acc src
    striter drop                   // outer_acc new_acc src[1:]
    read-str-acc
} ;

: read-str { "" ` read-str-acc } ;

// tokenize-loop: acc src → chunk_list
// stack: [acc, src]
: tokenize-loop {
    skip-ws                             // acc src(cleaned)
    _ "" streq ??drop|]                 // done: return acc

    // String: read until closing "
    _ 0 strnth 34 = ??
        striter drop read-str          // acc content rest_after  (striter→first,rest; drop "; read)
    |   next-chunk                     // acc chunk rest           (regular)
    ]
    // Common: acc elem rest → append elem to acc, recurse
    `                                   // acc rest elem
    $2                                  // acc rest elem acc (copy acc)
    `                                   // acc rest acc elem
    append                              // acc rest [acc..., elem]
    `                                   // acc new_acc rest
    `                                   // new_acc acc rest
    drop                                // new_acc rest
    tokenize-loop                       // recurse
} ;

: tokenize {
    [] ` tokenize-loop
} ;

export tokenize
