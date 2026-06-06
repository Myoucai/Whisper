# std/math — Mathematical functions
# No capabilities required (pure computation)

: sq { _ * } ;
: cube { _ sq * } ;
: pow { _ swap @times } ;               # base exp pow → base^exp
: abs { _ 0 > ??_|0 _ -]] } ;           # n → |n|
: sign { _ 0 > ??1|_ 0 < ??0 1 -|0]] } ;
: factorial { _ 1 > ??_ 1 - factorial *|1]] } ;
: fib { _ 1 > ??_ 1 - fib _ 2 - fib +|_]] } ;

export sq
export cube
export pow
export abs
export sign
export factorial
export fib
