// Whisper Math Standard Library
: sq        { _ * } ;
: cube      { _ sq * } ;
: double    { _ + } ;
: halve     { 2 / } ;
: inc       { 1 + } ;
: dec       { 1 - } ;
: neg       { 0 ` - } ;
: abs       { _ 0 < ?? 0 ` - | ] } ;
: max       { _ $2 > ?? ` drop | drop ] } ;
: min       { _ $2 < ?? ` drop | drop ] } ;
: clamp     {
    rot
    _ $2 > ?? drop ` drop return ]
    _ $3 < ?? drop drop return ]
    ` drop ` drop
} ;
: even?     { _ 2 % 0 = } ;
: odd?      { _ 2 % 1 = } ;
: zero?     { _ 0 = } ;
: positive? { _ 0 > } ;
: negative? { _ 0 < } ;
: pow       { _ 0 = ?? drop 1 | 1 ` { over * } @times ] } ;
: factorial { _ 1 > ?? _ 1 - factorial * | drop 1 ] } ;
: fib       { _ 1 > ?? _ 1 - fib ` 2 - fib + | ] } ;
: even      { even? } ;
: odd       { odd? } ;
: lshift    { << } ;
: rshift    { >> } ;
: sqrt      { fsqrt } ;
: f2i       { f64toi64 } ;
: i2f       { i64tof64 } ;
: between?  { rot drop _ over <= ` _ >= & } ;

// Constants (push as float literals)
: pi        { 3.141592653589793 } ;
: e         { 2.718281828459045 } ;
: tau       { 6.283185307179586 } ;
: inf       { 1.0 0.0 / } ;

// Float math wrappers
: floor     { floor } ;
: ceil      { ceil } ;
: round     { round } ;
: log       { log } ;
: exp       { exp } ;
: atan2     { atan2 } ;

// Bitwise operations
: bitand    { bitand } ;
: bitor     { bitor } ;
: bitxor    { bitxor } ;
: bitnot    { bitnot } ;

export sq
export cube
export double
export halve
export inc
export dec
export neg
export abs
export max
export min
export clamp
export even
export odd
export even?
export odd?
export zero?
export positive?
export negative?
export pow
export factorial
export fib
export lshift
export rshift
export sqrt
export f2i
export i2f
export between?
export pi
export e
export tau
export inf
export floor
export ceil
export round
export log
export exp
export atan2
export bitand
export bitor
export bitxor
export bitnot
