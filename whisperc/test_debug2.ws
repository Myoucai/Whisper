: debug-full {
    dup 0 @nth len over 1 @nth <= ??
        "DONE: " . 2 @nth . return
    ]
    dup 0 @nth over 1 @nth @nth
    "elem=" . dup . drop
    dup is-cf-op ??
        "cf-op" .
        1 @nth
        over 1 @nth 1 + +
        over 2 @nth swap append
        swap
        dup 0 @nth
        swap
        dup 1 @nth 1 +
        swap
        drop
        swap
        mk3
        "state1=" . dup . drop
        debug-full
    ]
    "non-cf" .
    drop
    dup 0 @nth
    swap
    dup 1 @nth 1 +
    swap
    2 @nth
    swap
    rot
    mk3
    "state2=" . dup . drop
    debug-full
} ;
[[[80 2] 16 97] 0 []] debug-full
