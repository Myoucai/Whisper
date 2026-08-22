// expect: 55|27|6
: fib { _ 1 > ?? _ 1 - fib ` 2 - fib + | ] } ; 10 fib .
: sq { _ * } ; : cube { _ sq * } ; 3 cube .
: p3 { $2 $2 + + } ; 1 2 3 p3 .
