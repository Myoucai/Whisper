// FizzBuzz in Whisper
// Usage: whisper run fizzbuzz.ws

import "std/str"

: fizzbuzz {
    _ 15 mod 0 = ??"FizzBuzz"
    |_ 3 mod 0 = ??"Fizz"
    |_ 5 mod 0 = ??"Buzz"
    |i64tostr ] ] ]
} ;

// Numbers 1-15
1 fizzbuzz .  2 fizzbuzz .  3 fizzbuzz .
4 fizzbuzz .  5 fizzbuzz .  6 fizzbuzz .
7 fizzbuzz .  8 fizzbuzz .  9 fizzbuzz .
10 fizzbuzz . 11 fizzbuzz . 12 fizzbuzz .
13 fizzbuzz . 14 fizzbuzz . 15 fizzbuzz .
"Done."
