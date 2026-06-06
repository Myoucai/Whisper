: sq { _ * } ;
: cube { _ sq * } ;
: abs { _ 0 > ??_|0 ` -] } ;
: factorial { _ 1 > ??_ 1 - factorial *|% 1] } ;
: fib { _ 1 > ??_ 1 - fib ` 2 - fib +|%] } ;
: even { _ 2 / 2 * = } ;
: odd { even ! } ;

export sq cube abs factorial fib even odd
