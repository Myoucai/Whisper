: sq { _ * } ;
: cube { _ sq * } ;
: abs { _ 0 > ??_|0 ` -] } ;
: factorial { _ 1 > ??_ 1 - factorial *|drop 1] } ;
: fib { _ 1 > ??_ 1 - fib ` 2 - fib +|drop] } ;
: even { _ 2 % 0 = } ;
: odd { even ! } ;

export sq cube abs factorial fib even odd
