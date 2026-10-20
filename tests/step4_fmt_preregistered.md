# Step 4 第一项：Whisper 写的 `fmt`（预注册）

2026-10-20 立项。Step 4 的目标是把 `fmt`/包管理/REPL/`check`/LSP 逐步迁到
Whisper。`fmt` 是其中难度最低的一项，本轮先做**可验收的第一片**。

## 范围（v1：布局规范化，不做 AST 重排）

Rust 侧 `whisper fmt` 现在是 AST printer：注释会被 parser 丢掉，多行定义会被
压成一行。把它直接照搬过来会把"删注释"也搬过来，因此 v1 **不追求与 Rust fmt
逐字节相同**，而是先做一个安全、可逆的布局规范化器：

- `whisperc/fmt.ws`，纯 Whisper，编译成 `wfmt`
- CLI：`wfmt <src>` 打印到 stdout；`wfmt <src> <out>` 写到 `<out>`
- 规则：
  1. CRLF → LF
  2. 字符串字面量之外的**行首空白**去掉
  3. 字符串字面量之外的**行尾空白**去掉
  4. 行内 token 间距、字符串内容、注释正文不动
  5. 跨行字符串、转义引号、`//` 注释里的引号都不能把状态搞错
- v2（下一轮）：在编译器 parser 暴露可保留注释/跨度的 AST 之后，再做真正的
  pretty-print 与 AST 重排。

## 判据

- **F-1 构建**：`whisper build whisperc/fmt.ws -t c2 -o wfmt` 成功；产物能处理
  一个含定义/字符串/注释的文件。
- **F-2 幂等**：对 conformance 语料与含字符串/注释的样例，
  `wfmt(wfmt(x)) == wfmt(x)`（逐字节）。
- **F-3 语义保持**：对 `tests/conformance/*.ws`，原文件与格式化后的文件经
  `whisper run` 输出逐字节相同；至少对 5 个代表文件再经 `-t c2` 编译运行，
  输出也相同。
- **F-4 字符串保持**：含多行字符串、`\"`、`\\`、行内多个空格的样例，格式化后
  字符串内容逐字节不变（只允许 CRLF 归一化）。
- **F-5 注释保持**：`//` 后的正文不变；只有注释行/代码行前的缩进可以被去掉。
  注释里有 `"` / `//` 时不能影响后续行的字符串状态。
- **F-6 不写坏文件**：`wfmt <src> <out>` 只写 `<out>`；源文件缺失时退出 1，
  参数不足时退出 2。

## 反向判据（什么算失败）

- 删注释、改写字符串、把多行定义压成一行来"让格式好看" → **失败**（那是 v2
  的范围，v1 不做）。
- 为了处理字符串而直接跳过所有含 `"` 的行 → **失败**（F-4 覆盖不到）。
- 依赖 Rust 运行时/CLI 才能格式化 → **失败**（工具必须是 Whisper 源码）。
- 只对样例有效、conformance 语料里有任意一个语义变化 → **失败**。

## 实现要点

- 逐行状态机：`in_str` / `escaped` / `slash` / `comment` 四个标志，跨行保留
  `in_str`/`escaped`；每行重置 `slash`/`comment`。
- 行首缩进只在行首不在字符串内时去掉；行尾空白只在行尾不在字符串内时去掉。
- 用 KV 保存四个标志（字符串 "0"/"1"），避免在栈上同时搬四个布尔。
- 先做 `-t c2` 构建用于迭代；再做一次 `wd` 构建（零 Rust 链）作为收口验证。

## 执行记录

2026-10-20 第一轮落地。**F-1..F-6 全部通过**，门禁
`bash scripts/fmt-gate.sh` 为 **5 pass / 0 fail**（4 组断言 + 1 条构建检查）：

```
F-1   ✅ 构建并处理含定义/字符串的文件
F-4/5 ✅ 多行字符串/转义/注释保持且语义一致
F-2/3 ✅ conformance 27 个文件幂等且 VM 输出一致
F-3b  ✅ 5 个代表文件经 c2 编译运行输出一致
F-6   ✅ 用法错=2，源文件缺失=1
```

**实现**：`whisperc/fmt.ws`，纯 Whisper。逐行状态机用 KV 保存
`in/esc/slash/cm` 四个 "0"/"1" 标志；`fmt-char` 逐字节更新，`fmt-line`
只在行首/行尾不在字符串内时去空白，`fmt-source` 按行折叠后用
`fmt-chomp` 去掉多加的最后一个换行。

**构建**：`scripts/build-wfmt.sh` 走 `wd`（零 Rust 链）产出
`target/wfmt/wfmt`；`scripts/fmt-gate.sh` 也接受 `-t c2` 构建用于迭代。

**本轮踩到并修掉的四个真 bug**：

1. `fmt-set` 的 `strcat` 操作数顺序反了：键变成 `infmt.` 而不是 `fmt.in`，
   于是所有行都被当成字符串外，多行字符串的缩进被吃掉。修法是
   `swap "fmt." swap strcat swap kv-set`。
2. `fmt-chomp` 抄了 `wd.ws` 的旧写法，`strnth` 已经把字符串消费掉后还
   `dup`，末尾换行时会 stack underflow；补一个 `dup` 保住原串。
   （`wd.ws` 里同名的 `wd-chomp` 也有同样的潜在 bug，当前无人调用。）
3. `fmt-main` 写文件时 `path`/`data` 顺序反了，`write-file` 实际拿源文本
   当路径；补 `swap`。
4. 门禁把格式化结果命名为 `*.fmt` 而不是 `*.fmt.ws`，`-t c2` 直接拒绝
   该扩展名；格式化临时文件统一带 `.ws`。

**已知限制（留到 v2）**：v1 不做 AST 重排，所以 Rust `whisper fmt` 的
"每个顶层节点一行、定义压成一行"不会复现；这是有意的，因为 Rust 版会
丢注释。v2 需要 parser 保留注释/跨度，或单独实现 comment-attached printer。
