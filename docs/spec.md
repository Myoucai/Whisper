# Whisper Language Specification — v1.0 (FROZEN)

> **语法冻结声明**：本文档是 Whisper v1.0 的语法合同。自本版起，任何语法变更都
> 需要同时更新两套实现（Rust VM 与自举 C 链）、通过
> `scripts/syntax-gate.sh`（25 项双实现一致性）与 `scripts/stdlib-gate.sh`
> （24 项 stdlib 门槛），并重新微调语言模型。语义级修复（不改语法的行为修正）
> 不受此约束。

## 1. 概述

Whisper 是栈式、后缀表示法的编程语言。每个操作符消费栈顶操作数并压回结果；
程序是 token 序列，无表达式优先级、无变量声明。

两套权威实现，机器对拍保证一致：
- **Rust VM**（`whisper run`）— 参考实现与运行时
- **自举 C 链**（`scripts/bootstrap.sh` → whisper2；Rust 侧 `whisper build -t c2`）— 独立编译器

## 2. 词法

| 元素 | 语法 | 示例 |
|------|------|------|
| 整数 (i64) | 可选 `-` 前缀的数字序列 | `42` `-1` `0` |
| 浮点 (f64) | `整数.整数`（两半都必须是合法整数） | `3.14` `-0.5` |
| 字符串 | `"..."`，转义  `\n \t \r \\ \"`  | `"hello" "a
b"` |
| 布尔 | `#t` / `#f` | `#t` |
| 列表字面量 | `[ item ... ]` | `[1 2 3]` `[[1 2] [3 4]]` |
| 引用块 | `{ token ... }` | `{ _ * }` |
| 注释 | `//` 至行尾 | `3 4 + // 求和` |

**标识符**：字母开头，可含字母数字 `_ - / ? $ > <`。`$`、`>`、`<` 仅在词中
部合法（词首是操作符）：`is-$n?`、`i64>f64` 合法；`$2`（pick）、`>`（gt）
是操作符。

**约束**：`??`、`!=`、`<=`、`>=`、`..` 等多字符操作符建议与相邻 token 用空白
分隔。Rust 词法器会主动切分 `??word` 粘连，自举链词法器不切分——**规范写法是
始终留空白**。

## 3. 操作符总表（冻结）

### 3.1 栈操作

| 符号 | 词形 | 字节 | 栈效应 |
|------|------|------|--------|
| `_` | `dup` | 0x00 | `a → a a` |
| `` ` `` | `swap` | 0x01 | `a b → b a` |
| | `drop` | 0x02 | `a →` |
| `@` | `rot` | 0x03 | `a b c → b c a` |
| `$N` | | 0x04 | `.. a_N .. → .. a_N .. a_N` |
| | `over` | = `$1` | `a b → a b a` |
| | `return` | 0x61 | 从当前词提前返回 |

### 3.2 算术与比较

| 符号 | 字节 | 说明 |
|------|------|------|
| `+` `-` `*` `/` `%` | 0x10-0x14 | i64 算术；除零为运行时错误 |
| `=` `!=` `<` `>` `<=` `>=` | 0x18-0x1D | 压 `#t`/`#f` |
| `&` `\|` `!` | 0x20-0x22 | 逻辑与/或/非 |

### 3.3 控制流

两种**等价合法**的条件语法（同一规范）：

```
cond ?? THEN | ELSE ]          ;; 基本形式
cond ?? T1 | cond2 ?? T2 | E ] ]  ;; elif 链（严格嵌套，每 ?? 一 ]）
```

自举链额外接受共享闭括号形式（`?? A | B ?? C | D ]`，最后一级隐式闭合于
作用域尾）——这是历史写法，**新代码应使用严格形式**。`]` 在条件分支内与
列表字面量存在歧义（分支内写 `[...]` 列表可能被误判为闭合），已知限制：
条件分支内避免使用列表字面量。

| 形式 | 字节 | 说明 |
|------|------|------|
| `?? THEN \| ELSE ]` | 0x50 | 条件（消费 cond） |
| `{body} {cond} #` | 0x52 | while 循环（先 body 后 cond 引用块） |
| `n {body} @times` | 0x53 | 定次循环 |
| `{then} ?->` | | 单分支 |

### 3.4 高阶操作

| 符号 | 字节 | 栈效应 |
|------|------|--------|
| `list {fn} @map` | 0x43 | → 新列表（fn: item → result） |
| `list {fn} @each` | 0x44 | 副作用遍历（fn 的结果被丢弃） |
| `list init {fn} @fold` | 0x45 | → acc（fn: acc item → acc） |
| `list {pred} @filter` | 0x05 | → 满足 pred 的子列表 |
| `list n @nth` | 0x40 | → 第 n 个元素（负数为错误） |

### 3.5 列表 / 字符串 / 浮点 / 位 / Map

列表：`len`(0x42) `append`(0x41) `prepend`(0xDE) `sort` `rev` `dedup`
`take` `drop-n` `list-slice` `listfind`

字符串（字节语义）：`strlen strcat strslice streq strlt strfind strreplace
strtoi64 i64tostr strnth strchars charsstr striter strjoin strsplit ctos`

浮点（f64 以位模式流转，显式转换）：`i64tof64 f64toi64 fsqrt fsin fcos ftan
floor ceil round log exp atan2`；字面量直接支持（`3.14`）

位运算：`bitand bitor bitxor bitnot shl shr`（符号形 `<<` `>>` 等价）

Map（关联表实现）：`map-new map-get map-set map-del map-keys map-has`

类型谓词：`list? i64? str? bool?`

### 3.6 定义与模块

```
: name { body } ;          ;; 词定义（递归合法）
import "std/module"        ;; 导入（c2 递归内联；Rust 由 resolver 解析）
export name                ;; 导出（一词一行）
```

词名可以与操作符关键字同名（`TokenKind::keyword_name` 保证往返），但同名
重定义行为未定义。

## 4. 两实现的已记录差异（非语法）

| 差异 | Rust VM | 自举 C 链 |
|------|---------|-----------|
| 字符串 `.` 输出 | 带引号 `"hi"` | 裸文本 `hi`（driver 依赖） |
| 浮点 `.` 输出 | `3.14` | 位模式整数（用 `f64toi64` 转换后显示） |
| 两个 f64 的 `+ - * /` | 正确 | 位级整数运算（值无标签）——经 `i64tof64` 进、`f64toi64` 出的单操作数 f-opcode 正确 |
| `..` 格式 | `Stack: [...]` | `[...]` |
| json-parse / bytes-* | 支持 | 显式 `#error` 拒绝编译 |
| 能力 `@0-@5`、置信度 | 支持 | no-op |

## 5. 一致性强制

- `tests/conformance/*.ws`（25 项）— 每个语法构造一个程序，双实现输出必须一致
- `scripts/syntax-gate.sh` — 执行上述对拍（CI 强制）
- `scripts/stdlib-gate.sh` — stdlib 24 项门槛（CI 强制）
- `scripts/verify-fixedpoint.sh` — 自举固定点字节级验证（CI 强制）

冻结版本：**v1.0**（2026-08-22）

## 6. 词汇附录 v1.0.1（原生 IO）

语法冻结不动；本附录新增 8 个内建词（库级扩展，双实现同步）：

| 词 | 字节 | 栈效应 | 说明 |
|----|------|--------|------|
| `listen` | 236 | port → handle \| -1 | 在 0.0.0.0:port 监听 TCP |
| `accept` | 237 | handle → conn \| -1 | 阻塞接受连接 |
| `recv` | 238 | conn → str | HTTP 式读取：遇 `

`、对端关闭、2 秒超时或 64KB 停止 |
| `send` | 239 | conn str → n \| -1 | 发送，返回字节数 |
| `close` | 241 | handle → | 关闭 socket |
| `read-file` | 242 | path → str \| "" | 读文件（错误返回空串） |
| `write-file` | 243 | path data → n \| -1 | 写文件（path 先入栈） |
| `getenv` | 244 | name → str \| "" | 读环境变量 |

错误处理惯例：哨兵值（-1 / 空串），不抛异常。stdlib：`std/net`（`http-respond`）。
一致性由 `scripts/net-gate.sh`（真实 HTTP 服务 + curl）与 conformance #26 强制。

## 7. 词汇附录 v1.0.2（共享 KV 与服务启动）

语法冻结不动；本附录新增 5 个内建词（双实现同步）：

| 词 | 字节 | 栈效应 | 说明 |
|----|------|--------|------|
| `kv-set` | 245 | key value → | 存入进程级共享 KV（跨线程可见，互斥保护） |
| `kv-get` | 246 | key → value \| "" | 取值；缺失返回空串 |
| `kv-del` | 247 | key → | 删除键 |
| `kv-keys` | 248 | → [keys] | 所有键的列表 |
| `serve-concurrent` | 255 | port handler → | C 链：每连接一线程（永不返回）；Rust VM：串行 accept 循环（同源程序） |

KV 是 **进程级字符串→字符串存储**（1024 键上限，互斥保护），与栈上的 Map（函数式、
按值复制）是两个层次：KV 用于跨请求/跨线程的共享状态。`serve-concurrent` 的 handler
以词名引用（C 链 cgen 自动生成 w_find_word 注册表），接收 conn 句柄；handler 返回后由
runtime 关闭连接——handler 内**不要** close（双重释放）。
C 链每 worker 独立 64MB 堆 + 512K 栈（`__thread`），已通过 10 路并行负载全路由验证。
一致性由 app-gate 的 13 项路由断言强制（含 KV CRUD、404/405+Allow 头、%XX 多字节 URL 解码）。

## 8. 词汇附录 v1.0.3（交互与字符串 P0）

语法冻结不动；本附录新增 5 个内建词（双实现同步，字节 249-253）：

| 词 | 字节 | 栈效应 | 说明 |
|----|------|--------|------|
| `read-line` | 249 | → str | 从 stdin 读一行（去行尾 `\n`/`\r`）；EOF 返回空串（哨兵惯例，同 read-file） |
| `str-upper` | 250 | str → str | ASCII a-z → A-Z，其余字节原样（多字节 UTF-8 不受影响，字节语义一致） |
| `str-lower` | 251 | str → str | ASCII A-Z → a-z，同上 |
| `str-strip` | 252 | str → str | 去除两端 ASCII 空白（空格/`\t`/`\r`/`\n`） |
| `fmt` | 253 | str value → str | 把**第一个** `{}` 替换为 value 的文本形式（i64→十进制、f64→十进制、str→原样、bool→`#t`/`#f`） |

`fmt` 为固定二元数、可链式组合：`"x={} y={}" 5 fmt 7 fmt` → `x=5 y=7`（每调用替换
一个占位符，从左到右）。v1.0.3 不提供 `{{` 转义（已知限制）。C 链中 bool 以 i64
1/0 表示，`fmt` 对布尔值渲染为 `1`/`0`（与 VM 的 `#t`/`#f` 存在已记录差异，
同字符串 `.` 输出的引号差异一节）。
一致性由 stdlib-gate 的 6 项 p0 断言强制（双链执行输出一致）。

## 9. 词汇附录 v1.0.4（目录/时间/循环退出 P1）

语法冻结不动；本附录新增 5 个内建词（字节 200-203、254）：

| 词 | 字节 | 栈效应 | 说明 |
|----|------|--------|------|
| `listdir` | 200 | path → [names] | 目录项名列表（按字节序排序；错误→空列表） |
| `mkdir` | 201 | path → 0 \| -1 | 单层建目录（已存在→-1） |
| `file-exists` | 202 | path → #t/\#f | 存在性检查（stat） |
| `strftime` | 203 | ts fmt → str | UTC 格式化，支持 %Y %m %d %H %M %S %%；双实现为逐字节一致的手写算法（不用平台 strftime） |
| `leave` | 254 | → | 退出最内层 `{body} {cond} #` 循环；循环外为 no-op。双实现均在编译期回填跳转（Rust: compile_loop 补丁；C 链: op-loop 的 patch-leaves 把标记改写为跳转） |

`leave` 语义与 C 的 `break` 等价（仅限 `#` 循环；quote 内的 leave 不会被外层循环回填）。
一致性由 stdlib-gate 的 6 项 p1 断言强制（双链输出一致）。

## 10. 词汇附录 v1.0.5（regex 子集）

语法冻结不动；本附录新增 2 个内建词（字节 233、204）：

| 词 | 字节 | 栈效应 | 说明 |
|----|------|--------|------|
| `strmatch` | 233 | str pattern → #t/#f | 整串锚定匹配 |
| `strfind-re` | 204 | str pattern → idx\|-1 | 首个匹配起点（无锚点搜索） |

子集：字面量、`.`、`*` `+` `?`（贪婪+回溯）、`[...]`（范围/`^`取反）、
`^` `$` 锚点、`\d \D \w \W \s \S` 及转义字面量、顶层 `|`（首选分支优先）、
`(...)` 分组（不捕获，可加量词）。不支持：反向引用、前瞻、惰性量词、
`{n,m}`、Unicode 类。模式长度上限 4096 字节。
双实现为逐字节一致的手写回溯匹配器（Rust `rematch.rs` / C `w_re_*`），
零正则库依赖。`strmatch` 的交替取首个成功分支（PCRE 语义）。

**附带的编译器修复**：bootstrap lexer 的未知转义此前丢弃反斜杠
（`"\d"` 变 `"d"`），与 Rust lexer 不一致；现统一为保留 `\`+字符，
并显式处理 `\"` 与 `\`（固定点已复验）。这使得 regex 模式能在
自举链中原样存活。
一致性由 stdlib-gate 的 10 项 p2 断言强制（双链输出一致）。

## 11. 词汇附录 v1.0.6（struct 二进制打包）

语法冻结不动；本附录新增 2 个内建词（字节 12、13）：

| 词 | 字节 | 栈效应 | 说明 |
|----|------|--------|------|
| `pack` | 12 | [values] fmt → bytes | 小端打包（错误→空串） |
| `unpack` | 13 | bytes fmt → [values] | 解包（截断/坏格式→空列表） |

格式串字符：`i` i32、`I` u32、`h` i16、`H` u16、`b` i8、`B` u8、`q` i64、
`Q` u64、`d` f64（位模式 8 字节）、`s` NUL 结尾字符串。前导 `<` 可选
（小端为唯一字节序；大端 `>` 不支持，文档化边界）。

语义约定：超宽整数值按 C 风格截断回绕（`[511] "<b" pack` → 字节 0xFF），
有符号字段解包时符号扩展（→ -1）；`unpack` 遇截断或缺失 NUL 返回空列表
（哨兵惯例）；pack 的值数与格式不符返回空串。
双实现打包字节逐位一致（Rust `structpack.rs` / C `w_pack`/`w_unpack`）。
**已记录差异**：VM 的 `unpack 'd'` 返回 F64 值；C 链返回位模式整数
（该链 f64 的固有表示，见 §4 差异表）。
一致性由 stdlib-gate 的 6 项 p2 断言强制（双链输出一致，f64 按位断言）。

## 12. 词汇附录 v1.0.7（range 内建）+ strjoin 语义修复

**`range`**（字节 205，v1.0.7）：`start count → [start..start+count-1]`。
预分配构造（替代递归 append 的 O(n²)；10 万元素从 83s → 19ms）。
`range-to` 改为薄封装：`n → [1..n]` 即 `1 ` range`。

**`strjoin` 语义修复**（v1.0.7，破坏性）：原实现无视栈上的分隔符
（实为纯拼接）。现固化为 `sep list strjoin → str`（sep 先入栈），
双链一致。seed 的两处调用点做了机械签名同步（seed include
runtime.c，链接适配不涉编译器逻辑）。
