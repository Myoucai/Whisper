# Whisper Language Specification v1.0

## 1. Overview

Whisper is a stack-based, postfix-notation, AI-native programming language.

**Design goals:**
- Token-economical (30-60% of Python's token count for equivalent logic)
- Capability-safe (zero IO by default; all side effects require explicit auth)
- Confidence-native (every value can carry a probability score 0.0–1.0)
- Self-hosting (compiler written in Whisper via `whisperc/main.ws`)
- Dual representation (`.ws` text ↔ `.wbin` binary equivalents)

## 2. Lexical Elements

### 2.1 Literals

| Type | Syntax | Examples |
|------|--------|----------|
| Integer (i64) | digits, optional `-` | `42`, `-1`, `0` |
| Float (f64) | digits `.` digits | `3.14`, `0.0`, `-1.5` |
| String | `"..."` with `\n`, `\t`, `\\`, `\"` escapes | `"hello"`, `"line\n"` |
| Boolean | `#t` (true), `#f` (false) | `#t` |
| List | `[` items `]` | `[1 2 3]`, `["a" "b"]` |
| Quotation | `{` body `}` | `{ _ * }`, `{ 2 + }` |

### 2.2 Comments

Line comments start with `//`. The `#` character is a loop operator, not a comment.

```
3 4 +   // this is a comment
```

### 2.3 Identifiers

Words (identifiers) can contain letters, digits, `_`, `-`, `/`.  
They are case-sensitive.

## 3. Syntax

### 3.1 Postfix Notation

All operations use postfix (reverse Polish) notation. No parentheses or operator precedence.

```
3 4 + .       // 7  (not 3 + 4)
5 6 * 2 + .   // 32 (not 5 * (6 + 2))
```

### 3.2 Stack Operations

| Op | Symbol | Stack Effect | Description |
|----|--------|-------------|-------------|
| dup | `_` | `a → a a` | Duplicate top |
| swap | `` ` `` | `a b → b a` | Swap top two |
| drop | `drop` | `a →` | Discard top |
| rot | `@` | `a b c → b c a` | Rotate top three |
| pick | `$n` | `... a_n → ... a_n a_n` | Copy nth element |

### 3.3 Arithmetic

| Op | Stack Effect | Description |
|----|-------------|-------------|
| `+` | `num num → num` | Addition |
| `-` | `num num → num` | Subtraction |
| `*` | `num num → num` | Multiplication |
| `/` | `num num → num` | Division |
| `mod` | `i64 i64 → i64` | Modulo |

Numeric types (i64, f64) are automatically coerced. Division by zero errors.

### 3.4 Comparison

| Op | Stack Effect |
|----|-------------|
| `=` | `a b → bool` |
| `<` | `num num → bool` |
| `>` | `num num → bool` |
| `!=` | `a b → bool` |
| `<=` | `num num → bool` |
| `>=` | `num num → bool` |

### 3.5 Logic

| Op | Stack Effect |
|----|-------------|
| `&` | `bool bool → bool` |
| `\|` | `bool bool → bool` |
| `!` | `bool → bool` |

### 3.6 Control Flow

**Conditional:** `cond ??then-expr|else-expr]`

```
5 3 > ??100|0] .     // 100 (5>3 → true)
2 3 > ??100|0] .     // 0   (2>3 → false)
```

**Single-branch:** `cond {then} ?->`

```
5 3 > { 100 } ?->    // executes {100} if 5>3
```

**Loop:** `{body} {cond} #`

```
0                   // accumulator
{ 1 + _ 1 + }       // body: increment both
{ _ 10 < }          // condition: while second < 10
#                   // loop
```

**Fixed-count loop:** `n {body} @times`

```
5 { "hello" . } @times   // prints "hello" 5 times
```

### 3.7 Word Definitions

```
: wordname { body } ;
```

Words can be recursive and mutually recursive.

```
: sq { _ * } ;
: cube { _ sq * } ;
: factorial { _ 1 > ??_ 1 - factorial *|drop 1] } ;
```

### 3.8 Modules

```
import "std/math"     // load stdlib module
import "./mylib"       // load relative module (mylib.ws)
export myfunc          // mark word for export
```

The `std/` prefix maps to the `stdlib/` directory. Search order: source dir → repo root → `~/.whisper/`.

### 3.9 Confidence System

Every value can carry a confidence score (0.0–1.0). Operations propagate confidence multiplicatively.

```
42 :0.5           // value 42 with 50% confidence
10 :0.5 2 *       // → 20:0.5  (0.5 × 1.0 = 0.5)
```

**Probabilistic choice:** `value {branch1} {branch2} ?|`
- If value has confidence `c`, branch1 executes with probability `c`, branch2 with probability `1-c`.

```
10 :0.7 { _ * } { 2 * } ?| .   // 70% chance: 100, 30% chance: 20
```

### 3.10 Lists

```
[1 2 3 4 5]               // list literal
[1 2 3] len .             // 5
[1 2 3] 0 @nth .          // 1
[1 2] 3 append .           // [1 2 3]
[1 2 3] { _ * } @map .    // [1 4 9]
[1 2 3] { . } @each       // prints each element
[1 2 3] 0 { + } @fold .   // 6 (sum)
```

### 3.11 I/O

| Op | Syntax | Stack Effect |
|----|--------|-------------|
| output | `.` | `a →` |
| output-all | `..` | `... →` |
| read | `,` | `→ str` |

### 3.12 Capabilities

IO operations require capability tokens bound at launch:

```
whisper run --allow-file-read script.ws
whisper run --allow-http script.ws
whisper run --allow-env --allow-exec script.ws
```

Capability IDs:
| ID | Token | Flag | Purpose |
|----|-------|------|---------|
| 0 | `@0` | `--allow-file-read` | Read files |
| 1 | `@1` | `--allow-file-write` | Write files |
| 2 | `@2` | `--allow-http` | HTTP GET |
| 3 | `@3` | `--allow-http` | HTTP POST |
| 4 | `@4` | `--allow-env` | Environment variables |
| 5 | `@5` | `--allow-exec` | Shell commands |

## 4. Type System

| Type | Description |
|------|-------------|
| `i64` | 64-bit signed integer |
| `f64` | IEEE 754 double-precision float |
| `bool` | Boolean (`#t` / `#f`) |
| `str` | Immutable UTF-8 string (Rc) |
| `[T]` | Homogeneous list (Rc) |
| `ref` | Quotation/block: `[inputs] → [outputs]` |
| `cap(n)` | Capability token (type-safe, cannot mix with data) |
| `signal(T)` | Value with confidence score |
| `T | U` | Union type (from conditionals) |

## 5. Program Structure

A Whisper program is a flat sequence of operations. There are no statements, expressions, or block structures — everything operates on the shared data stack.

```
import "std/math"
import "std/str"

: greet { "Hello, " swap strcat . } ;
"World" greet
5 sq .
```

## 6. Binary Format (.wbin)

| Field | Size | Description |
|-------|------|-------------|
| Magic | 4 bytes | `WHSP` (0x57 0x48 0x53 0x50) |
| Version | 4 bytes | u32 LE (currently 1) |
| Body | variable | LEB128-encoded opcode stream |

## 7. Compilation Pipeline

```
.ws source → Lexer → Parser → (Import Resolver) → TypeChecker
    → BytecodeGenerator → Optimizer → .wbin
                                  → .wasm (direct)
                                  → VM execution
```

## 8. Self-Hosting

The `whisperc/main.ws` file is a Whisper compiler written in Whisper. The bootstrap pipeline runs:

```
Rust Lexer → Rust Parser → Rust Pre-compiler (AST→tokens)
    → whisperc (Whisper compiler) → bytecode
    → VM execution
```
