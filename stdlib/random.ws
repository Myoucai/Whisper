// Whisper Random Standard Library

: random     { random } ;
: randint    { randint } ;
: coin-flip  { 0 2 randint 0 = } ;
: random-choice { _ len 0 = ?? drop drop 0 | _ len 0 randint @nth ] } ;

export random
export randint
export coin-flip
export random-choice
