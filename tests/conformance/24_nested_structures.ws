// expect: 3|10|2
: m { [1 2 3] len } ; m .
: pos2 { _ 0 > ?? _ 2 * | drop 0 ] } ; 5 pos2 .
[[1] [1 2]] { len } @map 1 @nth .
