# Whisper Language Specification v0.1.0

## 1. Overview

Whisper is a stack-based, concatenative, AI-native programming language.

**Design goals:**
- Token-economical (30-60% of Python's token count)
- Capability-safe (zero IO by default)
- Confidence-native (every value carries probability)
- Self-hosting (compiler written in Whisper)
- Dual representation (.ws text, .wbin binary)

## 2. Lexical Elements

### 2.1 Literals
| Type | Syntax | Example |
|------|--------|---------|
| Integer | digit sequence | `42`, `-1` |
| Float | digit.digit | `3.14` |
| String | double-quoted | `"hello"` |
| Boolean | `#t`, `#f` | |
| List | `[ ... ]` | `[1 2 3]` |
| Quotation | `{ ... }` | `{ dup * }` |

### 2.2 Operators
See design document section 2.1.2 for the complete operator table.

## 3. Type System

| Type | Description |
|------|-------------|
| `i64` | 64-bit signed integer |
| `f64` | IEEE 754 float |
| `bool` | Boolean |
| `str` | UTF-8 string |
| `[T]` | Homogeneous list |
| `ref` | Quotation/block |
| `cap(n)` | Capability token |
| `signal(T)` | Value with confidence |

## 4. Compilation Pipeline

```
.ws source → Lexer → Parser → TypeChecker → CodeGen → .wbin / .wasm
```

## 5. Binary Format (.wbin)

- Magic: `WHSP` (4 bytes)
- Version: u32 LE
- Body: LEB128-encoded opcodes
