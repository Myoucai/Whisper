# Whisper Opcode Reference v1.0

Complete reference for all 72 VM opcodes.

## Stack Operations (0x00–0x0F)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0x00` | `Dup` | `a → a a` | Duplicate top of stack |
| `0x01` | `Swap` | `a b → b a` | Swap top two elements |
| `0x02` | `Drop` | `a →` | Discard top of stack |
| `0x03` | `Rot` | `a b c → b c a` | Rotate top three |
| `0x04` | `Pick(n)` | `... a_n → ... a_n a_n` | Copy nth element (0-based from top) |

## Arithmetic (0x10–0x17)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0x10` | `Add` | `num num → num` | Addition (i64/f64 auto-coerce) |
| `0x11` | `Sub` | `num num → num` | Subtraction |
| `0x12` | `Mul` | `num num → num` | Multiplication |
| `0x13` | `Div` | `num num → num` | Division (errors on /0) |
| `0x14` | `Mod` | `i64 i64 → i64` | Modulo (errors on /0) |

## Comparison (0x18–0x1F)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0x18` | `Eq` | `a b → bool` | Equality |
| `0x19` | `Lt` | `num num → bool` | Less than |
| `0x1A` | `Gt` | `num num → bool` | Greater than |
| `0x1B` | `Neq` | `a b → bool` | Not equal |
| `0x1C` | `Le` | `num num → bool` | Less than or equal |
| `0x1D` | `Ge` | `num num → bool` | Greater than or equal |

## Logic (0x20–0x23)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0x20` | `And` | `bool bool → bool` | Logical AND |
| `0x21` | `Or` | `bool bool → bool` | Logical OR |
| `0x22` | `Not` | `bool → bool` | Logical NOT |

## Literals (0x30–0x3F)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0x30` | `PushI64(n)` | `→ i64` | Push signed 64-bit integer |
| `0x31` | `PushF64(n)` | `→ f64` | Push IEEE 754 float |
| `0x32` | `PushStr(s)` | `→ str` | Push UTF-8 string |
| `0x33` | `PushBool(b)` | `→ bool` | Push boolean |
| `0x34` | `PushList` | `e1 ... en n → [T]` | Pop n elements into a list |
| `0x35` | `PushRef(code)` | `→ ref` | Push quotation block |

## List Operations (0x40–0x45)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0x40` | `Nth` | `[T] i64 → T` | Get element at index |
| `0x41` | `Append` | `[T] T → [T]` | Append element to list |
| `0x42` | `Len` | `[T] → i64` | List length |
| `0x43` | `Map` | `[T] ref → [U]` | Transform each element |
| `0x44` | `Each` | `[T] ref →` | Iterate with side effects |
| `0x45` | `Fold` | `[T] U ref → U` | Reduce list to value |

## String Operations (0x46–0x4E)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0x46` | `StrLen` | `str → i64` | String length |
| `0x47` | `StrCat` | `str str → str` | Concatenate two strings |
| `0x48` | `StrSlice` | `str i64 i64 → str` | Substring (start, len; bounds-clamped) |
| `0x49` | `StrEq` | `str str → bool` | String equality |
| `0x4A` | `StrLt` | `str str → bool` | Lexicographic less-than |
| `0x4B` | `StrFind` | `str str → i64` | Find first occurrence (index or -1) |
| `0x4C` | `StrReplace` | `str str str → str` | Replace all occurrences |
| `0x4D` | `StrToI64` | `str → i64` | Parse string to integer |
| `0x4E` | `I64ToStr` | `i64 → str` | Format integer to string |

## Control Flow (0x50–0x53)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0x50` | `Cond(off)` | `bool →` | Conditional jump: if false, skip `off` opcodes |
| `0x51` | `Jump(off)` | `→` | Unconditional jump by offset |
| `0x52` | `Loop(off)` | `bool →` | Loop: if true, jump back by offset |
| `0x53` | `Times` | `i64 ref →` | Execute quotation n times |

## Call/Return (0x60–0x61)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0x60` | `Call(name)` | `... → ...` | Call word by name (runtime lookup) |
| `0x61` | `Return` | `→` | Return from current word/block |

## Capability (0x70–0x71)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0x70` | `CapCall(id)` | `arg → result` | Call capability by ID |
| `0x71` | `CapExec` | `cap → result` | Execute capability token on stack |

## Float Operations (0xB0–0xB5)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0xB0` | `I64ToF64` | `i64 → f64` | Convert integer to float |
| `0xB1` | `F64ToI64` | `f64 → i64` | Truncate float to integer |
| `0xB2` | `FSqrt` | `f64 → f64` | Square root |
| `0xB3` | `FSin` | `f64 → f64` | Sine (radians) |
| `0xB4` | `FCos` | `f64 → f64` | Cosine (radians) |
| `0xB5` | `FTan` | `f64 → f64` | Tangent (radians) |

## JSON (0xB6–0xB7)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0xB6` | `JsonParse` | `str → value` | Parse JSON string to Whisper value |
| `0xB7` | `JsonStringify` | `value → str` | Serialize Whisper value to JSON |

## Confidence (0x80–0x81)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0x80` | `ConfLabel(c)` | `a → a:c` | Label value with confidence |
| `0x81` | `ProbChoice` | `{alt2} {alt1} → result` | Probabilistic branch choice |

## I/O (0x90–0x92)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0x90` | `OutputTop` | `a →` | Print top of stack |
| `0x91` | `OutputAll` | `... →` | Print entire stack (debug) |
| `0x92` | `ReadInput` | `→ str` | Read line from stdin |

## Definitions (0xA0–0xA3)

| Byte | Mnemonic | Stack Effect | Description |
|------|----------|-------------|-------------|
| `0xA0` | `DefWord(name)` | `→` | Compiler: start word definition |
| `0xA1` | `EndDef` | `→` | Compiler: end word definition |
| `0xA2` | `Import` | `→` | Compiler: import module |
| `0xA3` | `Export` | `→` | Compiler: export word |

## Byte Layout Summary

```
0x00–0x04  Stack ops
0x10–0x14  Arithmetic
0x18–0x1D  Comparison
0x20–0x22  Logic
0x30–0x35  Literals
0x40–0x45  List ops
0x46–0x4E  String ops
0x50–0x53  Control flow
0x60–0x61  Call/Return
0x70–0x71  Capability
0x80–0x81  Confidence
0x90–0x92  I/O
0xA0–0xA3  Definitions
0xB0–0xB5  Float ops
0xB6–0xB7  JSON
```
