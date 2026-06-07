: sq        { _ * } ;
: cube      { _ sq * } ;
: abs       { _ 0 < ??0 ` -|] } ;
: factorial { _ 1 > ??_ 1 - factorial *|drop 1] } ;
: fib       { _ 1 > ??_ 1 - fib ` 2 - fib +|] } ;
: even      { 2 % 0 = } ;
: odd       { 2 % 0 != } ;

export sq
export cube
export abs
export factorial
export fib
export even
export odd
