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
// Label values with confidence (Signal wrapper, clamp [0,1])
42 :0.5          // 42 with 50% confidence

// Confidence propagates through arithmetic
10 :0.5 2 * .    // 20:0.5

// Read confidence without consuming the value (L-04)
7 :0.3 dup conf-of . .   // 0.3, then 7:0.3

// Strip the Signal wrapper (L-04)
7 :0.3 unwrap .          // 7
```

### Probabilistic choice `?|` (L-03 semantics)

```whisper
// Stack: ... p {preferred} {fallback} ?|
// P(preferred) = p; p must be f64 in [0,1] or a Signal (uses its
// confidence, unwraps back to the plain value). Missing/invalid p
// is a runtime error — no implicit defaults.

0.9 { "go" } { "wait" } ?| .     // 90% "go"

// Signal composes: confidence of x drives the choice,
// the branch sees the unwrapped value
10 :0.7 { _ * } { 2 * } ?| .     // 70%: 100, 30%: 20
```

### Determinism

```whisper
42 seed!      // pin the xorshift64 RNG (i64 seed)
rand .        // next uniform f64 in [0,1) — replayable after seed!
```

## File I/O

```whisper
"input.txt" read-file .           // print file contents
"data.txt" "Hello!" ` write-file  // write: path first (swap it on top)
// write-file OVERWRITES — append = read + strcat + write (see journal)
```

## HTTP Client

```whisper
// Requires: whisper run --allow-http

import "std/http"

"https://api.example.com/data" http-get .   // https:// 同样支持（rustls）
```

会话级请求（cookie 罐 + 状态码/响应头 + 审计行）：

```whisper
"GET" "https://api.example.com/data" "{}" "" http-req
// → status headers-json body；chunked 响应自动解码
```

## Confidence（置信度）

```whisper
import "std/conf"

42 seed!                    # 定种子（概率分支可复现）
#t:0.75 conf-of . unwrap .  # 0.75 / #t
0.9 0.5 conf-mul .          # 0.45（证据链合取）
0.7 { 1 } { 2 } ?| .        # P(上支)=0.7
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
import "std/str"

// [.., list] → [.., count]   (word parked in KV under key "w")
: wcount {
    0
    { dup "w" kv-get streq ` drop ?? 1 + | ] } @fold
} ;

// [orig, uniq] →  (prints "word : count", consumes uniq, then orig)
: report {
    dup len 0 = ?? drop drop return ]
    dup 0 @nth dup "w" ` kv-set      // park word + keep a copy
    " : " strcat "p" ` kv-set        // park "word : " prefix
    $1 wcount i64tostr               // count occurrences in orig ($1)
    "p" kv-get ` strcat .            // "word : N"
    1 drop-n
    report
} ;

"hello world hello" " " strsplit
dup sort dedup
report
// → hello : 2
//   world : 1
```

Why it is shaped this way (all learned the hard way):

- **`@fold` needs `(list, init, quote)` with the list deepest.** The body
  sees exactly `[acc, item]` — nothing else. To compare `item` against a
  *target* word, the word cannot ride the stack: it is parked in KV and
  fetched with `kv-get` inside the body.
- **`streq` consumes both operands.** With `[acc, item]`, the pattern
  `dup "w" kv-get streq` leaves `[acc, item, bool]`; the `` ` drop ``
  moves the bool above the item and drops it, so the conditional sees
  `[acc, item, cond]` and the then-branch's `1 +` computes `acc+1`.
- **Two lists, two jobs**: `dup sort dedup` forks the split into
  `(orig, uniq)`; recursion consumes `uniq` while `$1` (the value
  *under the top*, i.e. `orig`) feeds `wcount`. Counting must run over
  the **original** split — folding the deduped list yields 1 everywhere.
- **`$N` counts from the top, *excluding* the top**: on `[a, b]` (b on
  top), `$1` is `a`. On `[orig, uniq]`, `$1` = orig. `$2` needs at
  least three values on the stack.
- **A word that consumes the stack top needs no copy**: `wcount` takes
  the list directly from the top (`0 {q} @fold` — fold pops quote,
  init `0`, then the list). Earlier drafts copied with `$1` first and
  produced `(init, list)` in the wrong order for fold's pops.
- **Termination must clean everything**: the recursion ends with
  `drop drop return ]` — one drop per parked list — otherwise the
  caller's caller inherits garbage.

## Case Study: Building a Real CLI Tool (journal)

`apps/journal/journal.ws` (~60 lines) is a real command-line diary —
`j add "text"`, `j list`, `j tail [N]`, `j search word`, `j del N`.
Building it surfaced every pattern (and every trap) that matters when
writing non-trivial Whisper. This section distills the lessons.

### Pattern 1: argv adaptation — one source, two run forms

```whisper
: j-args {
    args dup 1 @nth "run" streq $1 len 1 > & ?? 3 | 1 ]
    over len $1 - list-slice
} ;
```

`args` returns the full argv. Under `whisper run file.ws a b` the user
arguments start at index 3; as a native c2 executable they start at 1.
Detect `"run"` at index 1 and slice accordingly. **Note `list-slice`
takes (list, start, COUNT)** — the third argument is a length, not an
end index.

### Pattern 2: quote sandbox vs. process-global KV

Quotes passed to `@each`/`@map`/`@filter` are **sandboxed**: they see
their own argument and nothing of the outer stack. You cannot capture
a variable. Two escapes:

1. **Stack-below (read-only, one value):** keep the value under the
   list and read it with `$1` inside the quote — works when the quote
   consumes exactly its item. The word-frequency counter above uses
   this (`dup $1 streq`).

2. **KV as global registers:** store the value with `kv-set` outside,
   `kv-get` inside. This is how `j search word` reaches the query
   string, and how `j del N` maintains a line counter:
   `"ji" kv-get strtoi64 1 + i64tostr "ji" ` kv-set` at the head of
   the quote.

### The five KV traps (all verified the hard way)

1. **Key goes on the stack first AND ends up *below* the value.**
   To store `x` under key `"k"` from pieces already on the stack,
   you usually need a swap: `[value] "k" ` kv-set`.
2. **Values are strings only.** The store is str→str (v1.0.2 spec).
   `kv-set("k", 3)` is a runtime type error — convert with
   `i64tostr`, and `strtoi64` on the way out.
3. **`kv-set` returns nothing.** A following `drop` underflows the
   stack silently ... then loudly.
4. **`kv-set` consumes exactly two values** — if the value you meant
   to park sits under something else, park first, then build the rest.
   A `dup` before the park keeps your own copy.
5. **Keys collide across helper words.** `journal` uses `"jq"`,
   `"ji"`, `"out"`; the word-frequency counter uses `"w"`, `"p"`.
   Prefix per feature (`j-*`, `w*`) — there is no scope to save you.

### Pattern 3: elif chains — dup your dispatcher

```whisper
j-args dup len 0 > ?? 0 @nth | drop "" ]
dup "add"    streq ?? drop j-add
| dup "list"  streq ?? drop j-list
| dup "search" streq ?? drop j-search
| drop "usage: ..." .
] ] ]
```

The first `streq` **consumes** the command string; every subsequent
branch must re-`dup` it. And in the strict nesting form each `??`
needs its own `]` — a dispatcher over N branches closes N brackets.
(Shared-close form `?? A | B ?? C | D ]` is accepted by the bootstrap
chain, but strict form is the documented standard.)

The same rule bit the word-frequency example's inner conditional:
`?? ... | ... ]` inside a quote still needs its own closing bracket,
and a `drop`-cascade for the operands `streq` left behind.

### Pattern 4: text-file append

`write-file` overwrites. The append idiom:

```whisper
JFILE read-file          // old content ("" if missing)
` strcat                 // swap old above the new line, concatenate
JFILE ` write-file drop
```

Note the symmetry: both file words want **path deeper than data** —
one swap before `strcat`, one swap before `write-file`.

### Pattern 5: newline without escape hell

Source-level `"
"` works in the VM, but string escapes in generated
C contexts invited subtle bugs during development. The robust form
used by journal: `10 ctos` — the byte 10 as a one-character string.
Same for `13 ctos` (CR) when you need it.

### Performance notes learned along the way

- Never build lists by appending in a loop (`w_list_append` copies
  the whole list — O(n²)). Prefer `range`, pre-sized `@map`, or the
  two-pass filter. A 10k-line `strsplit` once cost 110ms; after the
  pre-allocation fixes the whole tool runs in ~6ms.
- `@filter`+`strlen` is the idiom for dropping empty lines after
  `strsplit "
"` (the trailing split always yields one empty item
  that otherwise pollutes counts and output).

### Try it

```bash
cd apps/journal
../../target/release/whisper.exe build journal.ws -t c2 -o j.exe
./j.exe add "first entry"
./j.exe tail
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
