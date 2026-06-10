// Whisper Math Standard Library
: sq        { _ * } ;
: cube      { _ sq * } ;
: double    { _ + } ;
: halve     { 2 / } ;
: inc       { 1 + } ;
: dec       { 1 - } ;
: neg       { 0 ` - } ;
: abs       { _ 0 < ?? 0 ` - | ] } ;
: max       { _ over > ?? | drop ] } ;
: min       { _ over < ?? | drop ] } ;
: clamp     { _ over > ?? drop ` | _ over < ?? drop | ] ] } ;
: even?     { _ 2 % 0 = } ;
: odd?      { _ 2 % 1 = } ;
: zero?     { _ 0 = } ;
: positive? { _ 0 > } ;
: negative? { _ 0 < } ;
: pow       { _ 0 = ?? drop 1 | _ 1 - over ` pow * ] } ;
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
