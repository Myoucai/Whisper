# 战役 S-1：非 ASCII 字符串语义两链分叉（预注册）

2026-09-21 立项。仿前几份：**判据先写死，再动代码**。

## 上一轮登记的根因判断是错的（先纠正）

上一轮（收口战役收尾）我把它登记成"两条运行时对字节/字符没有共识，**各自
内部也不一致**，要统一 7 个原语"。**这个判断是错的**，而且错在方法上：那张
矩阵是 `| tr -d '\r'` 之后打到控制台再肉眼读的，而非 ASCII 经终端会被改写。
**和"探针里忘了写 `.`"是同一类错误**——用被污染的观测下结论。

重做测量台（一律重定向到文件、用 `od` 出字节）之后，真相简单得多。

## 实测事实（字节精确）

源串 `"你好ab"` = `E4 BD A0 E5 A5 BD 61 62`（8 字节 / 4 码点）。

**绕开字面量、在运行时用 `ctos`+`strcat` 拼出同样的字节串**（这样才能把
"编译期损坏"和"运行期语义"分开）：

| 原语 | Rust VM | C 链 | 判定 |
|------|---------|------|------|
| `strlen` | 8 | 8 | ✅ 都是字节 |
| `strnth 1` | 189 | 189 | ✅ 字节 |
| `strfind` | 3 | 3 | ✅ 字节偏移 |
| `strslice 0 2` | 存 `E4 BD`（打印时 lossy 成 U+FFFD） | `E4 BD` | ✅ **字节**，只是两链打印约定不同 |
| `ctos 228` | 1 字节 | 1 字节 | ✅ 字节 |
| **`strchars`** | `[20320 22909 97 98]`（**码点**） | `[228 189 160 …]`（**字节**） | ❌ |
| **`striter`** | 码点 + 余串（按**字符**前进） | 字节 + 余串（按**字节**前进） | ❌ |
| **`charsstr`** | `char::from_u32`（码点编码） | 按字节取低 8 位 | ❌ |

**C 链完全符合字节语义；按字符的只有 VM 的三个 opcode。**

**而这三个 opcode 就是字面量被写坏的原因**：Whisper lexer 的 `read-str-acc`
用 `striter` 前进、却用 `strnth`+`ctos` 取字节。在 C 链上 `striter` 按字节，
两者一致；在 VM 上 `striter` 按**字符**前进 → 每遇到一个多字节字符就跳过它的
后续字节 → `"你好ab"` 在 `-t c2` 编译期被削成 `E4 E5 61 62`。

## 判定依据（不是我选的，是仓库里早就写好的）

1. **spec §3.5 明写字节语义**；
2. **`stdlib/encoding.ws:3`**："词表（字节语义 §3.5：strnth/strslice/charsstr 均按字节）"；
3. **`stdlib/hash.ws:5`**："字节管线：消息 → strchars 字节列表（任意 UTF-8 安全）"；
4. C 链已如此实现；`StrNth` 的 VM 注释也写着 "byte semantics (§3.5)"。

**所以本战役不改语义、不改 spec、不改 C 链**——只是把 VM 那三个 opcode
改成它自己别处已经在用的字节语义。

## 判据

- **S1-a** 上表三行的 VM 侧与 C 链一致。
- **S1-b** `-t c2` 编出的 C 里，字面量 `"你好"` 是 **6 字节** `E4 BD A0 E5 A5 BD`，
  与 `wd`（原生 cgen）一致；`"你好ab" strlen .` 两链都给 **8**。
- **S1-c** `strchars`/`charsstr` **往返**：`"<任意字节>" strchars charsstr` 等于原串
  （含非 ASCII 与非法的中间态）。
- **S1-d** 依赖这三个词的 stdlib 两链同值：`str-rev`、`upper`、`lower`、
  `url-decode`、`hash` 里的字节管线。
- **S1-e** 既有门禁零回归（cargo / 六闸 / 固定点）。**改 `runtime.c` 或
  `compiler.all.ws` 都要重跑固定点**；本战役若只动 `vm.rs` 则不需要，但仍要跑全部。

## 反向判据（什么算失败，提前写明）

- 为了让两链一致而**改 C 链**（C 链是对的）→ **失败**。
- 改 `spec §3.5` 去迁就 VM → **失败**。
- 把 `Value::Str(Rc<String>)` 换成字节串类型"顺手做干净" → **失败**：那是
  全 VM + stdlib 的伤筋动骨改动，本战役不需要（`Ctos` 已有
  `unsafe { String::from_utf8_unchecked }` 的先例，字节串可以安全地装在
  `String` 里，只要保证"最终构成的序列是有效 UTF-8"这条既有约定）。
- 只修 lexer 的 `read-str-acc`（治标）而不动那三个 opcode → **失败**：
  `striter` 本身仍与 C 链不一致，stdlib 里直接用它的地方照样分叉。

## 执行记录

2026-09-21 实施完成。**S1-a ~ S1-e 全部达成**，改动比立项时预估的小得多：
**只动了 VM 的三个 opcode**（外加 stdlib 一处无关的旧 bug）。

### 改了什么

| 位置 | 改动 |
|------|------|
| `crates/whisper-core/src/vm.rs` `Opcode::StrChars` | `s.chars()` → `s.as_bytes()` |
| 同上 `Opcode::CharsStr` | `char::from_u32` → 按字节取低 8 位（与 `w_charsstr` 的 `(char)value` 一致），用 `from_utf8_unchecked` 装回 `String`（`Ctos` 已有同样的先例与注释） |
| 同上 `Opcode::StrIter` | 按**字节**推进：推 `(首字节, 其余原始字节)` |
| `stdlib/str.ws` `upper-char`/`lower-char` | 顺带修的旧 bug，见下 |

**C 链一行未改**（它本来就是对的），`spec` 未改，`Value::Str(Rc<String>)` 的
表示未改——反向判据里的三条都没踩。

### 逐条对照

| 判据 | 结果 | 证据 |
|------|------|------|
| **S1-a** 三个原语两链一致 | ✅ | 显示无关矩阵 24/25（第 25 条是 S-3，与 S-1 无关）：`strchars` 两链同为 `[228 189 160 229 165 189 97 98]`、`striter` 余串同、`charsstr` 往返同 |
| **S1-b** 字面量存活 | ✅ | `"你好ab" strlen` 两链皆 **8**；`-t c2` 的 C 里是完整 6 字节；`"a🎉b"`（4 字节 emoji）strlen 皆 6 |
| **S1-c** `strchars`/`charsstr` 往返 | ✅ | `"你好ab" strchars charsstr strchars` 等于原字节序列；中途含非法 UTF-8 的中间态也不丢字节 |
| **S1-d** stdlib 字节管线两链同值 | ✅ | `str-rev`、`upper`、`lower`、`url-decode` 全一致（集成测试 `s1_stdlib_byte_pipeline_matches_across_chains`）；`md5` 因 S-3 暂时编不出，见下 |
| **S1-e** 门禁零回归 | ✅ | cargo **467/0**、syntax 26/0、stdlib 58/0、typecheck FP=0、net PASS、app PASS(13)、覆盖 **90 → 101/0/0**、固定点 **gen2 == gen3**、fmt/clippy 干净 |

### 覆盖门禁补的非 ASCII 用例（S1-4）

新增 11 条（`cjk-*`），**全部只用核心原语**——门禁的 NSC 那侧不处理 `import`，
带 stdlib 的放集成测试。这 11 条正是这条分叉潜伏这么久的原因：原来的 `str-*`
用例全是 ASCII。

### 顺带修掉的旧 bug：`upper-char` / `lower-char`

```whisper
: upper-char { _ 96 > _ 123 < & ?? _ 32 - ` drop | ] } ;   // 旧
```
第二个 `_` 复制的是**上一步的布尔**而不是字符，条件退化成 `c > 96`，
于是**所有 >96 的字节都被减 32**——非 ASCII 一律被改坏。实测修前
`"a你b" upper` 给 `[65 196 157 128 66]`（你 的 228/189/160 全被减了 32），
修后给 `[65 228 189 160 66]` ✓。

**注意它此前是"两链一致地错"**：VM 的 `charsstr` 按码点编码，C 链按字节截断，
两边恰好都算出 196。S-1 让它们变成"一致"，这次顺手让它们变成"正确"。

### 没做到的：C 链的显示约定（不在 S-1 范围）

VM 的 `.` 打印字符串**带引号**，且对非法 UTF-8 做 lossy（`EF BF BD`），
C 链写裸字节。所以 `"你好ab" .` 两链的**输出字节**不同（`22 E4…22` vs `E4…`）。
这是既有的字符串打印约定分歧（README 已记），**与字节语义无关**，
本次矩阵一律用 `strchars` 转成数字列表来绕开它。要不要统一是另一个题目。



做 S1-d（stdlib 字节管线两链同值）时撞出来的。都与"字节 vs 字符"无关，
是**拼写与依赖**的问题：

### S-2 —— `mod` 只是 Rust 前端的别名，自举链词表里没有它

```
7 3 mod .
```
- `whisper run` → `1`（对的）
- `whisper build -t c2` → **编不出**，生成的 C 里是 `W_mod()`，gcc 报
  `implicit declaration of function 'W_mod'`

根因（已定位到行）：Rust 词法 `lexer.rs:455` 有 `"mod" => TokenKind::Mod`，
而自举链的词表 `compiler.all.ws:446` 只有 `[16 "+"] [17 "-"] [18 "*"] [19 "/"] [20 "%"]`
—— **没有 `"mod"`**。c2 / `wd` 走的是自举前端，于是把 `mod` 当成普通词引用。

**为什么一直没被发现**：覆盖门禁的算术用例用的是 `%`（`t arith '… 40 7 % .'`），
从没用过 `mod`。同一类问题（一个词只有一侧认）值得做一次全词表对拍——见下。

### S-3 —— `std/hash` 用了 `std/list` 的词却没 import 它

`stdlib/hash.ws` 只 `import "std/str"`，但第 151、164 行用了 `range-to`
（定义在 `stdlib/list.ws:25`）。`whisper run` 能跑（并给出正确的
`md5("你好") = 7eca689f0d3389d9dea66ae112e5cfd7`），C 链编不出：
`implicit declaration of function 'W_range_to'`。

**为什么一直没被发现**：`scripts/stdlib-gate.sh` 里**没有任何 hash/md5 用例**。

### 由这两条推出的建议

**做一次"词表对拍"**：把 Rust 前端的全部关键字拼写与自举链词表逐项比对，
差异即 S-2 这一类缺陷。这是机械可做的，比逐个撞出来便宜得多。


