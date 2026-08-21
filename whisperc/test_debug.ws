: debug-cf2 {
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
        "new-state=" . dup . drop
    ]
    drop .
} ;
[[[80 2]] 0 []] debug-cf2
