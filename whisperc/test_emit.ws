: test {
    dup collect-labels swap
    [] swap append swap append
    0 append "" append
    dup 0 @nth over 2 @nth @nth
    over 2 @nth emit-instr
    swap
    dup dup 1 @nth swap 2 @nth
    get-label-prefix
    // Stack: instr_str, state, prefix
    // Want: acc ++ prefix ++ instr_str as new acc
    // Approach: swap prefix below state, compute acc, then strcat
    swap       // instr_str, prefix, state
    dup 3 @nth // instr_str, prefix, state, acc
    rot        // instr_str, prefix, acc, state — NO
    .s
    drop drop drop drop
} ;
[[48 42] [97]] test
