# Whisper Standard Library v1.0

`stdlib/` 下共 **20 个可导入模块**（`import "std/<name>"` 全部实测通过）。
本文档详列其中 15 个；`std/core`、`std/net`、`std/random`、`std/time`
未在此详列，`std/ffi` 见 `docs/spec.md` §13/§14。

All modules are loaded via `import "std/<name>"`.

> **安全提示（2026-10-20）**：本页部分 “Requires `--allow-*`” 只适用于
> `@n` 能力别名。`read-file` / `write-file` / `getenv` 是 **native opcode**，
> 当前 VM 与 C 链都会**直接调用宿主**，`--allow-file-*` / `--allow-env`
> 并不会拦它们；FFI 目前也没有授权门。完整边界见
> [docs/security-model.md](security-model.md)，不可信代码请等 WASM 目标
> （[docs/wasm-wasi-plan.md](wasm-wasi-plan.md)）。

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

## std/hash — MD5（RFC 1321，纯 Whisper）

```whisper
import "std/hash"
"" md5 .        // d41d8cd98f00b204e9800998ecf8427e
"abc" md5 .     // 900150983cd24fb0d6963f7d28e17f72
```

RFC 全向量 + 多块消息（200×a）锁进集成测试。实现：字节列表管线
（UTF-8 安全）、寄存器/常数 kv 字符串化、32 位算术以 0xFFFFFFFF 截断。

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

`@0` / `@1` 能力需要 `--allow-file-read` / `--allow-file-write` flags。

> **注意**：`read-file` / `write-file` 是 **native opcode**（v1.0.1），
> 不是 `@0` / `@1` 的别名；当前两者都直接调用宿主，`--allow-file-*` 不拦
> 它们。`@0` / `@1` 才是走 `CapabilityTable` 的显式能力令牌。

| Function | Cap | Stack Effect | Description |
|----------|-----|-------------|-------------|
| `read-file` | native opcode（非 @0） | `path → content` | Read file to string |
| `write-file` | native opcode（非 @1） | `path content →` | Write string to file |
| `println` | — | `value →` | Print value |

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
| `http-get` | @2 | `url → body` | HTTP GET（仅 `http://`） |
| `http-post` | @3 | `url body → response` | HTTP POST（仅 `http://`） |
| `http-req` | @6 | `method url headers-json body → status headers-json body` | 会话请求：cookie 罐、状态码/响应头、审计（VM-only） |
| `http-audit` | @7 | `→ audit-json` | 导出全量请求审计（VM-only） |
| `http-req-raw` | @8 | 同 @6，body 为 hex | 原始字节通道（VM-only） |
| `http-req-parallel` | @9 | `count method url headers body → results-json` | 并发会话（VM-only） |

`@2`/`@3` 只支持 `http://`；`https://` 需要 `@6`（rustls + webpki 根），
而 `@6`-`@9` **目前只有 VM 有**，C 链调用会显式报
`not implemented on the C chain`。`@6` 的响应会自动解码
`Transfer-Encoding: chunked`，连接 10s / 读写 15s 超时。

```whisper
whisper run --allow-http script.ws
```

---

## std/os — Operating System

```whisper
import "std/os"
```

Requires `--allow-env` / `--allow-exec` flags.

> **注意**：`std/os` 的 `getenv` 是 `@4` 别名，受 `--allow-env` 约束；
> 但 **bare `getenv` 是 native opcode**，当前直接读环境，不受 flag 约束。
> `exec` 是 `@5`，VM 侧受 `--allow-exec`；C 链直接 `popen`。

| Function | Cap | Stack Effect | Description |
|----------|-----|-------------|-------------|
| `getenv` | @4 | `name → value` | Read environment variable（别名） |
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
