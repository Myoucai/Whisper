# Whisper Standard Library v0.2.0

## std/math — 数学函数

| 函数 | 签名 | 说明 | 示例 |
|------|------|------|------|
| `sq` | `n → n²` | 平方 | `5 sq .` → 25 |
| `cube` | `n → n³` | 立方 | `3 cube .` → 27 |
| `abs` | `n → |n|` | 绝对值 | `-5 abs .` → 5 |
| `factorial` | `n → n!` | 阶乘（递归） | `5 factorial .` → 120 |
| `fib` | `n → fib(n)` | 斐波那契（递归） | `10 fib .` → 55 |
| `even` | `n → bool` | 偶数判断 | `6 even .` → #t |
| `odd` | `n → bool` | 奇数判断 | `7 odd .` → #t |

## std/str — 字符串操作

| 函数 | 签名 | 说明 | 示例 |
|------|------|------|------|
| `strlen` | `str → n` | 字符串长度 | `"hello" strlen .` → 5 |
| `strcat` | `a b → ab` | 拼接字符串 | `"a" "b" strcat .` → "ab" |
| `strdup` | `s → ss` | 重复字符串 | `"ab" strdup .` → "abab" |

## std/list — 列表操作

| 函数 | 签名 | 说明 | 示例 |
|------|------|------|------|
| `length` | `[T] → n` | 列表长度 | `[1 2 3] length .` → 3 |
| `push` | `[T] T → [T]` | 追加元素 | `[1 2] 3 push .` → [1 2 3] |
| `map` | `[T] {T→U} → [U]` | 映射变换 | `[1 2 3] { _ * } map .` → [1 4 9] |
| `each` | `[T] {T→}` → | 遍历执行 | `[1 2 3] { . } each` |
| `fold` | `[T] U {U T→U} → U` | 折叠归约 | `[1 2 3] 0 { + } fold .` → 6 |
| `sum` | `[i64] → i64` | 列表求和 | `[1 2 3] sum .` → 6 |
| `product` | `[i64] → i64` | 列表求积 | `[1 2 3] product .` → 6 |
| `reverse` | `[T] → [T]` | 反转列表 | `[1 2 3] reverse .` → [3 2 1] |

## std/io — 文件 I/O

需要能力: `@file_read` `@file_write`

| 函数 | 签名 | 说明 |
|------|------|------|
| `read-file` | `path → content` | 读取文件内容 |
| `write-file` | `path content →` | 写入文件 |
| `println` | `value →` | 输出并换行 |

```
import "std/io"
"hello.txt" read-file .    # 输出文件内容
"output.txt" "data" write-file  # 写入文件
42 println                 # 输出 "42\n"
```

## std/json — JSON 处理

| 函数 | 签名 | 说明 |
|------|------|------|
| `json-parse` | `str → json` | JSON 字符串→嵌套列表 |
| `json-stringify` | `json → str` | 嵌套列表→JSON 字符串 |

JSON 表示为嵌套列表：`{"key": "val"}` ↔ `["key" "val"]`

## std/test — 测试框架

| 函数 | 签名 | 说明 |
|------|------|------|
| `assert-true` | `bool →` | 断言为真，输出 PASS/FAIL |
| `assert-false` | `bool →` | 断言为假 |
| `assert-eq` | `a b →` | 断言相等 |

```whisper
import "std/test"
3 4 + 7 assert-eq    # PASS
#t assert-true       # PASS
```
