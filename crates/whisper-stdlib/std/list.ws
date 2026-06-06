# std/list — List operations
# No capabilities required (pure computation)

: length { len } ;                      # list → length
: push { append } ;                     # list elem → new-list
: map { @map } ;
: each { @each } ;
: fold { @fold } ;
: sum { 0 { + } @fold } ;               # [i64] → i64 (sum of all elements)
: product { 1 { * } @fold } ;           # [i64] → i64 (product of all elements)
: reverse { [] { swap append } @fold } ; # list → reversed-list

export length
export push
export map
export each
export fold
export sum
export product
export reverse
