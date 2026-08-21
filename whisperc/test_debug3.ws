: debug-full {
    dup 0 @nth len over 1 @nth <= ??
        2 @nth "DONE=" . . return
    ]
    dup 0 @nth over 1 @nth @nth
    dup is-cf-op ??
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
        debug-full
    ]
    drop
    dup 0 @nth
    swap
    dup 1 @nth 1 +
    swap
    2 @nth
    swap
    rot
    mk3
    debug-full
} ;
[[[80 2] 16 97] 0 []] debug-full
