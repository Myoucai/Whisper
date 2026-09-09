# Whisper Standard Library v1.0

13 modules（新增 std/conf、std/encoding、std/jwt、std/html、std/csv）。

All modules are loaded via `import "std/<name>"`.

---

## std/encoding — base64 / base64url / 百分号编码

纯 Whisper 实现（零依赖，字节语义）。

```whisper
import "std/encoding"
"hello world" b64-enc .        // "aGVsbG8gd29ybGQ="
"aGVsbG8gd29ybGQ=" b64-dec .   // "hello world"
" Man" pct-enc .               // "%20Man"
"%20Man" pct-dec .             // " Man"
```

词表：`b64-enc` `b64-dec` `b64url-enc` `b64url-dec` `pct-enc` `pct-dec`
（另导出实现层词汇 `sextets/enc-ch/enc-step/enc-tail/sat/dec-ch/sget/
dec-group/hex-ch/hex-val/pct-byte/keep-set/b64-alpha/b64url-alpha/
b64-enc-with/b64-dec-with`）。

## std/jwt — JWT 解码（不验签）

依赖 std/encoding（b64url 无填充解码）。JWT 靶场（如 VulnOA）的载荷侧
消费：看 role/iat/exp/username。

```whisper
import "std/jwt"
token jwt-header .          // "{"alg":"HS256","typ":"JWT"}"
token "role" jwt-claim .    // "admin"（数字 claim 转字符串；缺 → ""）
```

## std/html — 轻量 HTML 文本提取（字节扫描，无 DOM）

```whisper
import "std/html"
"<p>Hello <b>W&amp;orld</b></p>" html-text .   // "Hello W&orld"
"<a href=\"/x\">t</a>" "a" "href" html-attr . // "/x"（仅双引号属性）
```

## std/csv — CSV 最小集（无引号转义）

```whisper
import "std/csv"
"a,b
1,2" csv-parse 0 @nth 1 @nth .          // "b"
```

## std/conf — 置信度（定稿表面）

```whisper
import "std/conf"
42 seed!                     # 定种子（复现）
#t:0.75 conf-of . unwrap .   # 0.75 / #t
0.9 0.5 conf-mul .           # 0.45（证据链合取）
0.7 { 1 } { 2 } ?| .         # P(上支)=0.7
```

铁律：置信度（浮点）不进 kv；持久化用 i64 千分比。

## std/math — Integer Mathematics

```whisper
import "std/math"
```

| Function | Stack Effect | Description | Example |
|----------|-------------|-------------|---------|
| `sq` | `n → n²` | Square | `5 sq .` → 25 |
| `cube` | `n → n³` | Cube | `3 cube .` → 27 |
| `abs` | `n → |n|` | Absolute value | `-5 abs .` → 5 |
| `factorial` | `n → n!` | Factorial (recursive) | `5 factorial .` → 120 |
| `fib` | `n → fib(n)` | Fibonacci (recursive) | `10 fib .` → 55 |
| `even` | `n → bool` | Even test | `6 even .` → #t |
| `odd` | `n → bool` | Odd test | `7 odd .` → #t |

---

## std/mathf — Float Mathematics

```whisper
import "std/mathf"
```

| Function | Stack Effect | Description | Example |
|----------|-------------|-------------|---------|
| `fsqrt` | `f64 → f64` | Square root | `16.0 fsqrt .` → 4.0 |
| `fsin` | `f64 → f64` | Sine (radians) | `0.0 fsin .` → 0.0 |
| `fcos` | `f64 → f64` | Cosine (radians) | `0.0 fcos .` → 1.0 |
| `ftan` | `f64 → f64` | Tangent (radians) | `1.0 ftan .` → 1.557... |
| `i64>f64` | `i64 → f64` | Integer to float | `42 i64>f64 .` → 42.0 |
| `f64>i64` | `f64 → i64` | Float to integer (truncate) | `3.9 f64>i64 .` → 3 |

---

## std/str — String Operations

```whisper
import "std/str"
```

| Function | Stack Effect | Description |
|----------|-------------|-------------|
| `strlen` | `str → i64` | String length |
| `strcat` | `str str → str` | Concatenate two strings |
| `strdup` | `str → str` | Duplicate string (s → ss) |
| `streq` | `str str → bool` | String equality |
| `strlt` | `str str → bool` | Lexicographic less-than |
| `strfind` | `str str → i64` | Find substring (index or -1) |
| `strreplace` | `str str str → str` | Replace all occurrences |
| `strtoi64` | `str → i64` | Parse string to integer |
| `i64tostr` | `i64 → str` | Format integer to string |

```whisper
"hello" strlen .          // 5
"abc" "xyz" strlt .       // #t
"hello world" "world" strfind .  // 6
"a-b-c" "-" ":" strreplace .    // "a:b:c"
"42" strtoi64 2 * .       // 84
99 i64tostr "!" strcat .  // "99!"
```

---

## std/list — List Operations

```whisper
import "std/list"
```

| Function | Stack Effect | Description |
|----------|-------------|-------------|
| `length` | `[T] → i64` | List length |
| `push` | `[T] T → [T]` | Append element |
| `map` | `[T] ref → [U]` | Transform each element |
| `each` | `[T] ref →` | Iterate with side effects |
| `fold` | `[T] U ref → U` | Reduce list to value |
| `sum` | `[i64] → i64` | Sum all elements |
| `product` | `[i64] → i64` | Multiply all elements |

```whisper
[1 2 3 4 5] sum .        // 15
[1 2 3] { _ * } map .    // [1 4 9]
[1 2 3 4] 1 { * } fold . // 24
```

---

## std/io — File I/O

```whisper
import "std/io"
```

Requires `--allow-file-read` / `--allow-file-write` flags.

| Function | Cap | Stack Effect | Description |
|----------|-----|-------------|-------------|
| `read-file` | @0 | `path → content` | Read file to string |
| `write-file` | @1 | `path content →` | Write string to file |
| `println` | — | `value →` | Print value |

```whisper
whisper run --allow-file-read script.ws
```

---

## std/json — JSON

```whisper
import "std/json"
```

| Function | Stack Effect | Description |
|----------|-------------|-------------|
| `json-parse` | `str → value` | Parse JSON string |
| `json-stringify` | `value → str` | Serialize to JSON string |

**JSON ↔ Whisper mapping:**
| JSON | Whisper |
|------|---------|
| `null` | `I64(0)` |
| `true`/`false` | `Bool` |
| `42`/`3.14` | `I64`/`F64` |
| `"hello"` | `Str` |
| `[1,2,3]` | `List` |
| `{"a":1}` | `List` of `[Str, value]` pairs |

```whisper
"[1,2,3]" json-parse .              // [1 2 3]
"[1,2,3]" json-parse json-stringify . // "[1,2,3]"
```

---

## std/http — HTTP Client

```whisper
import "std/http"
```

Requires `--allow-http` flag.

| Function | Cap | Stack Effect | Description |
|----------|-----|-------------|-------------|
| `http-get` | @2 | `url → body` | HTTP GET request |
| `http-post` | @3 | `url body → response` | HTTP POST request |

```whisper
whisper run --allow-http script.ws
```

---

## std/os — Operating System

```whisper
import "std/os"
```

Requires `--allow-env` / `--allow-exec` flags.

| Function | Cap | Stack Effect | Description |
|----------|-----|-------------|-------------|
| `getenv` | @4 | `name → value` | Read environment variable |
| `exec` | @5 | `cmd → [status stdout stderr]` | Execute shell command |

```whisper
whisper run --allow-env --allow-exec script.ws
```

---

## std/test — Testing

```whisper
import "std/test"
```

| Function | Stack Effect | Description |
|----------|-------------|-------------|
| `assert-true` | `bool →` | Assert true, print PASS/FAIL |
| `assert-false` | `bool →` | Assert false |
| `assert-eq` | `a b →` | Assert equality |

```whisper
import "std/test"
3 4 + 7 assert-eq    // PASS
#t assert-true       // PASS
```
