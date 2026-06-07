# Whisper Tutorial v1.0

## Hello World

```whisper
"Hello, World!" .
```

Only **2 tokens**.  Compare: Python `print("Hello, World!")` uses 6.

## Arithmetic (Postfix Notation)

All operations use postfix (reverse Polish) — no parentheses, no precedence rules.

```whisper
3 4 + .        // 7
10 3 - .       // 7
5 6 * .        // 30
42 6 / .       // 7
10 3 mod .     // 1

// Complex expressions
3 4 + 2 * .    // 14  (not 3 + 4 * 2)
```

Float literals: `3.14`, `0.0`, `-1.5`

```whisper
3.14 2.0 * .   // 6.28
16.0 fsqrt .   // 4.0
0.0 fsin .     // 0.0
0.0 fcos .     // 1.0
```

## Stack Operations

```whisper
5 _ * .        // 25   (dup: 5→5,5; mul: 25)
3 4 ` - .      // 1    (swap: 3,4→4,3; sub: 1)
42 drop .      //      (drop: remove top)
1 2 3 @        // stack: [1 3 2]  (rot)
```

## Conditionals

```whisper
5 3 > ??100|0] .       // 100  (5>3 is true)
2 3 > ??100|0] .       // 0    (2>3 is false)

// Single-branch
3 3 = { "equal!" . } ?->

// Complex
10 _ 0 < ??NEGATIVE|POSITIVE] .
```

## Word Definitions

```whisper
// Simple
: sq { _ * } ;
5 sq .                 // 25

// Composition
: cube { _ sq * } ;
3 cube .               // 27

// Multi-word
: double { 2 * } ;
: quad { double double } ;
5 quad .               // 20
```

## Recursion

```whisper
// Factorial
: fact { _ 1 > ??_ 1 - fact *|drop 1] } ;
5 fact .               // 120

// Fibonacci
: fib { _ 1 > ??_ 1 - fib ` 2 - fib +|] } ;
10 fib .               // 55
```

## Lists

```whisper
// Literals
[1 2 3 4 5]

// Operations
[1 2 3] len .          // 3
[10 20 30] 0 @nth .    // 10
[1 2] 3 append .       // [1 2 3]

// Higher-order
[1 2 3] { _ * } @map .    // [1 4 9]
[1 2 3 4 5] 0 { + } @fold .  // 15
```

## Strings

```whisper
"hello" strlen .             // 5
"Hello, " "World!" strcat . // "Hello, World!"
"hello" "hello" streq .     // #t
"hello world" "world" strfind .  // 6
"a-b" "-" ":" strreplace .  // "a:b"
"42" strtoi64 .             // 42
99 i64tostr .               // "99"
```

## JSON

```whisper
// Parse
"[1,2,3]" json-parse .           // [1 2 3]
"{\"key\":\"val\"}" json-parse . // [["key" "val"]]

// Stringify
[1 2 3] json-stringify .         // "[1,2,3]"

// Roundtrip
"[1,2,3]" json-parse json-stringify .  // "[1,2,3]"
```

## Modules (Import)

```whisper
// Standard library
import "std/math"
import "std/str"
import "std/list"

5 sq .                              // 25
"hello" strlen .                    // 5
[1 2 3 4 5] sum .                   // 15

// Local modules
import "./mylib"                    // loads ./mylib.ws
```

## Confidence System

```whisper
// Label values with confidence
42 :0.5          // 42 with 50% confidence

// Confidence propagates through operations
10 :0.5 2 * .    // 20:0.5

// Probabilistic choice
10 :0.7 { _ * } { 2 * } ?| .
// 70% chance: 100  (10*10)
// 30% chance: 20   (10*2)
```

## File I/O

```whisper
// Requires: whisper run --allow-file-read --allow-file-write

import "std/io"

"input.txt" read-file .          // print file contents
"output.txt" "Hello!" write-file  // write to file
42 println                        // print with newline
```

## HTTP Client

```whisper
// Requires: whisper run --allow-http

import "std/http"

"https://api.example.com/data" http-get .
```

## OS Operations

```whisper
// Requires: whisper run --allow-env --allow-exec

import "std/os"

"HOME" getenv .             // /home/user
"echo hello" exec .         // [0 "hello" ""]
```

## Testing

```whisper
import "std/test"

3 4 + 7 assert-eq     // PASS
#t assert-true        // PASS
5 3 < assert-true     // PASS
```

## Complete Example: Word Frequency Counter

```whisper
// Count word frequency in text
: split      { } ;  // TODO
: increment  { 1 + } ;
: word-count { } ;   // TODO

"hello world hello" word-count .  // [["hello" 2] ["world" 1]]
```

## CLI Quick Reference

```bash
whisper run    file.ws       # Execute source
whisper build  file.ws       # Compile to .wbin/.wasm
whisper check  file.ws       # Type-check only
whisper repl                 # Interactive REPL
whisper fmt    file.ws       # Format source
whisper install <pkg>        # Install package
whisper serve  file.ws       # HTTP server
whisper bootstrap file.ws    # Self-hosting pipeline
whisper lsp                  # LSP language server
```
