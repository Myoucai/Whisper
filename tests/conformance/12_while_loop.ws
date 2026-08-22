// expect: 5|120
0 { _ 1 + } { _ 5 < } # .
: fact { _ 1 > ?? _ 1 - fact * | drop 1 ] } ; 5 fact .
