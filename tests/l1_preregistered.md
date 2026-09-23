# 语言侧缺口：`exit` 与 `print`（预注册）

2026-09-21 立项。仿前几份预注册：**先把验收判据写死，再改代码**。

## 为什么立项

Step 3 写 `wd`（Whisper 自己写的驱动）时撞到两个**语言本身的缺口**——它们不是
`wd` 的实现问题，换任何程序都会撞上：

1. **C 链程序没有退出码**。`cgen` 固定发射 `return 0;`，Whisper 层没有
   `exit`/`quit` 一类的词。于是 `wd` 的错误路径（源文件不存在、C 编译器失败）
   **无法让 shell 看到非零退出码**——一个不能报失败的 CLI 工具不是工具。
   实测：`wd build nope.ws out` 打印了 `cannot read source` 却仍然 `exit=0`。
2. **没有裸打印词**。打印只有 `.`（opcode 144 `OutputTop`），它自带换行且
   对字符串的引号处理在两链不同；而 `striter`（opcode 186）不是打印词，是
   字符串**迭代器**（推 `[字符码, 余串]`）。于是 `wd run` 转发子进程输出时
   只能靠 `.`，末尾多一个换行——**做不到逐字节转发**。

两者都是「原语缺失」，不是「实现 bug」。

## 设计决定（提前写明，避免事后合理化）

**走正规 opcode 路线，不借 `@n` 能力 ABI。**

- `@n` 是**能力令牌**，语义是"这个副作用需要显式授权"。而"退出进程"和
  "打印一个字符串"不是需要授权的事——把它们塞进能力 ABI 会让每个用到的
  程序都要 `--allow-...`，那是把设计错误固化下来。
- 代价是明确且机械的：新增一个 opcode 要动一串位置（清单见下）。本轮接受。

**词形**：`print`（裸打印字符串，不补换行）与 `exit`（取栈顶 i64 作为
退出码并终止；栈空或非整数则报错）。

**opcode 字节**：`0x93` = `Print`、`0x94` = `Exit`。

> **修正（实施前，2026-09-21）**：本文件初稿写的是 `0xC8`/`0xC9`，**是错的**。
> 当时只按十六进制扫了高字节区，而 `listdir`/`mkdir` 在 `opcode.rs` 里写的是
> **十进制** `200`/`201` —— 正是 `0xC8`/`0xC9`，直接撞车。
> 重新机械化清点（解析 `opcode.rs` 的 `to_byte` 全部匹配臂 × 解析
> `compiler.all.ws` 的词表与 `emit-bare`），两链**同时**空闲的字节里，
> `0x93`/`0x94` 紧接 IO 区（`0x90` `.`、`0x91` `..`、`0x92` 读输入），
> 语义上是最自然的落点，故取之。
> 教训照旧：**十六进制 grep 会漏掉十进制字面量**；字节占用表要机器枚举，不能眼看。

**两链语义**：

| | VM | C 链 |
|---|---|---|
| `print` | 取栈顶字符串，写 stdout，**不加换行** | `fputs` + `fflush` |
| `exit` | 取栈顶 i64，以该码 `std::process::exit` | `exit((int)code)` |

## 判据（全部为机器可验的断言）

### E1 —— `print` 逐字节转发

对同一程序，VM 与 `-t c2` 的 stdout **逐字节相同**（不是"逐行"），且内容
**不因 `.` 的换行/引号规则被改写**。用例：

| 用例 | 源码 | 期望 stdout（字节） |
|------|------|--------------------|
| P-1 | `"hi" print` | `hi`（**无**换行） |
| P-2 | `"a" print "b" print` | `ab` |
| P-3 | `"x\n" print` | `x\n`（保留自带换行） |
| P-4 | `"" print` | （空） |

### E2 —— `exit` 给出真实退出码

| 用例 | 源码 | 期望 shell 退出码 |
|------|------|------------------|
| X-1 | `0 exit` | 0 |
| X-2 | `3 exit` | 3 |
| X-3 | `: f { 7 exit } ; f 1 .` | 7 且**不**打印 `1`（真终止） |

VM 与 `-t c2` 产物**退出码相同**。

### E3 —— 既有语义零回归

- `cargo test --workspace` ≥ 480 / 0；
- clippy / fmt 0；
- syntax / stdlib / typecheck / coverage / net / app 全绿不降；
- `opcode.rs` 的 `to_byte_is_injective`（自动覆盖新 opcode）通过；
- **固定点 gen2 == gen3**（本项必过：改 `compiler.all.ws`）。

### E4 —— `wd` 真正用上它们

- `wd build <不存在的文件> <out>` 的 shell 退出码 **非 0**；
- `wd run` 转发的输出与直接运行产物**逐字节相同**（不再多一个换行）。

## 反向判据（什么算失败，提前写明）

- 为省事把 `exit`/`print` 做成 `@n` 能力（要授权才能退出/打印） → **失败**：
  那是把设计错误固化。
- 为让 E2 通过而让 `exit` 只是"提前 return 到 main"（不终止） → **失败**：
  X-3 专门钉这一点。
- 改 `compiler.all.ws` 后固定点不成立且本战役内无法修复 → 回滚，如实登记。

## 触点清单（新增一个 opcode 要动的地方，实测清点）

Rust 侧（10）：
1. `crates/whisper-parser/src/ast.rs` —— `Operator` 变体
2. `crates/whisper-parser/src/parser.rs` —— 词形 → `Operator`
3. `crates/whisper-codegen/src/bytecode_gen.rs` —— `Operator` → `Opcode`
4. `crates/whisper-codegen/src/formatter.rs` —— 反查词形（`fmt` 用）
5. `crates/whisper-codegen/src/stack_effect.rs` —— (入, 出) 元数
6. `crates/whisper-codegen/src/wbin.rs` —— 字节 → `Opcode`（解码）
7. `crates/whisper-cli/src/commands/bootstrap.rs` —— `Operator` → 字节（编码）
   与 字节 → `Opcode`（解码）**两处**
8. `crates/whisper-core/src/opcode.rs` —— 变体、字节、名字、`ALL`（单射测试用）
9. `crates/whisper-core/src/vm.rs` —— 执行
10. `crates/whisper-codegen/src/cgen/transpiler.rs` —— `-t c`（Rust 转译器）后端
11. `crates/whisper-typecheck/src/checker.rs` —— 类型规则

自举链侧（2）：
12. `whisperc/compiler.all.ws` —— 词表 + `emit-bare` 分支
13. `whisperc/runtime.c` —— C 实现

## 执行记录

2026-09-21 实施完成。**结论：E1、E2、E3(a/b) 达成；E4 的 a/b 达成，c 只达成
了 `wd` 自己那一半**——根因在上游能力 `@5`，已如实登记为 W-5。

### 触点清单修正（预注册写 11+2，实际 14+2）

预注册漏了三个文件（都是「词形 → 语法」这一层，写清单时按 opcode 视角想，
把它们算进了 parser）：

| 文件 | 为什么漏不掉 |
|------|--------------|
| `crates/whisper-parser/src/lexer.rs` | 关键字表：`"print"` / `"exit"` 不登记就永远走 `Word` 分支 |
| `crates/whisper-parser/src/token.rs` | `TokenKind` 变体 + `keyword_name()` 反查（有测试钉住该不变式） |
| `crates/whisper-lsp/src/lib.rs` | 补全/悬停的词条表 |

`runtime.c` 确实需要改（见下），这一点预注册猜对了，但原因不是"要写 C 实现"
（`emit-bare` 直接内联 C），而是 **Windows 文本模式**。

### 实现摘要

- opcode `0x93` = `Print`、`0x94` = `Exit`（字节选择的修正见上）。
- VM：`Print` 取栈顶（先 `unwrap_signal`）要求 `str`，`write_all` + `flush`；
  `Exit` 要求 `i64`，`std::process::exit`。两者类型不符都报 `TypeMismatch`。
- C 链：`emit-bare` 发 `w_print_raw(_s)` / `{int64_t _c=w_as_i64(w_pop());exit((int)_c);}`；
  `-t c`（Rust 转译器）同语义。`runtime.c` 新增 `w_print_raw`。
- typecheck：`Print` 期望 `str`，`Exit` 期望 `i64`，都不回压栈。

### 实施中发现的第三个问题：C 链每个换行都变成 CRLF

`print` 在 C 链第一次跑就暴露：`"x\n" print` 在 C 链给出 `78 0D 0A`，VM 给
`78 0A`——E1 要求逐字节相同，直接不达标。

根因是 Windows 文本模式：C 运行时把 stdout 上每个 `\n` 改写成 CRLF，而 Rust
的 stdout 从不翻译。**这不是 L-1 引入的**——它一直存在，Rust 集成测试用
`.replace(['\r','"'], "")` 把它抹掉了。

第一版修法是"在 `print` 里惰性 `_setmode(_O_BINARY)`"。**错的**：文本翻译发生在
**flush 时**，而 stdout 重定向到文件时是全缓冲的，于是缓冲区里**更早**的 `\n`
也被按二进制写出——实测 `"A1" . "echo hi" @5 1 @nth print "A2" .` 里，
`A1` 后面就没有 CR 了，删掉 `print` 又变回 CRLF。**输出取决于后面有没有调用
`print`**，这比原来更糟。

改成在 `w_init` 里一次性设定（`_setmode(_fileno(stdout), _O_BINARY)`），
`w_print_raw` 退回纯 `fputs + fflush`。这样确定性成立，且顺带消掉了 C 链与 VM
之间那条长期靠测试抹平的真实分歧。**行为变更**：C 链程序（含 `.`）现在只发
LF，与 VM 一致。

### 判据实测

E1（stdout 逐字节，VM vs `-t c2`）：

| 用例 | 源码 | 期望 | VM | C 链 |
|------|------|------|----|------|
| P-1 | `"hi" print` | `68 69` | ✅ | ✅ |
| P-2 | `"a" print "b" print` | `61 62` | ✅ | ✅ |
| P-3 | `"x\n" print` | `78 0A` | ✅ | ✅ |
| P-4 | `"" print` | 空 | ✅ | ✅ |

E2（shell 退出码）：X-1 `0 exit` → 0；X-2 `3 exit` → 3；X-3
`: f { 7 exit } ; f 1 .` → 7 且 stdout 为空（真终止，不是提前 return）。两链全中。

E3：`cargo test --workspace` 482/0（基线 480，新增两条 L-1 测试）；
syntax 26 / stdlib 58 / typecheck FP=0 / net / app 13 路由 —— 全绿不降；
`opcode_coverage.sh` **87/0/0**（80 → 87，新增 `print-basic`/`print-twice`/
`print-nl`/`print-empty`/`exit-zero`/`exit-code`/`exit-in-word`）；固定点
gen2 == gen3 通过。

E4（`wd` 真正用上）：

- (a) `wd build <不存在的文件> <out>` → **exit=1** ✅（修前恒为 0）
- (b) 参数不足 → **exit=2** ✅
- (c) ⚠️ **只达成一半**。`wd` 侧已改 `print`，不再自己补换行——但 `wd run` 的
  转发段与直接运行产物仍不逐字节相同：子进程输出 `33 0A`，`wd` 转发 `33`。
  差的 `0A` 在 `wd` 看到它**之前**就没了——`@5` 按设计对两个流做 `trim()`
  （`capability.rs` 与 `runtime.c` 都写明"mirror Rust 的 `String::trim()`"，
  是刻意的双链约定）。**这是能力契约问题，不是语言问题**，属 L-1 范围之外，
  登记为 W-5。

E4 的验证用的是**零 Rust 链的产物**（`bash scripts/build-wd.sh` 端到端重跑成功，
产物 `target/wd/wd`，整条链只有 gcc + 冻结种子），不是快迭代配方编出来的
临时二进制。四条 `wd run` 冒烟（自包含 / `import std/math` + `sq` /
`import std/list` + `len` / 限定名 `math.sq`）与四条退出码用例在它上面全过；
另外全构建目标（`-t c`/`c2`/`exe`/`native`/`bundle`/`wbin`）都单独验过认得
这两个 opcode（`c` 看发射文本、`wbin` 看字节含 `93`/`94`、其余实际运行）。

### 顺带确认

`bash scripts/build-wd.sh` 本轮**端到端重跑通过**（此前只有 `bash -n` 级证据）——
这条链要重新走一遍种子 → whisper0 → whisper1，是 `compiler.all.ws` 改动的
独立复核。`scripts/bootstrap.sh` 仍未端到端重跑（见 W-3）。

### 反向判据自查

- 没有把 `exit`/`print` 做成 `@n` 能力 ✅
- `exit` 是真终止（X-3 钉住，C 链用 `exit()` 而非 `return`）✅
- 改 `compiler.all.ws` 后固定点成立 ✅

