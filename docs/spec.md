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
| 限定量 | `模块名.词名`（点前后均为词字符时并入词） | `str.find`（见 docs/namespaces-design.md；Rust VM 与自举 C 链均已同步） |
| 引用块 | `{ token ... }` | `{ _ * }` |
| 注释 | `//` 至行尾 | `3 4 + // 求和` |

**标识符**：字母开头，可含字母数字 `_ - / ? $ > <`。`$`、`>`、`<` 仅在词中
部合法（词首是操作符）：`is-$n?`、`i64>f64` 合法；`$2`（pick）、`>`（gt）
是操作符。

**约束**：`??`、`!=`、`<=`、`>=`、`..` 等多字符操作符建议与相邻 token 用空白
分隔。Rust 词法器会主动切分 `??word` 粘连，自举链词法器不切分——**规范写法是
始终留空白**。

**已知限制：条件臂内的列表字面量贴近闭合 `]`（2026-09-10 实测）**
`??` 直后的 then 臂以裸列表字面量收尾并紧贴 cond 闭合括号（如
`#t ?? [1 2 3] ] len . | "x" . ]`）时，**Rust VM 解析器报
`Unexpected token: RBracket`，自举链解析器接受**——两实现语法分歧，
在语法冻结解决前属已知限制。else 臂同形（`| [1 2 3] ]`）两实现均可。
规范写法：臂内列表后至少跟一个词（`?? [1 2 3] len . | ... ]`），
或把列表在条件之前构造（`[1 2 3] #t ?? len . | drop ]`）。

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
作用域尾）——这是历史写法，**新代码应使用严格形式**。`[`/`]` 由解析器的
括号深度感知区分：列表字面量在条件分支内任意位置（含分支尾、elif 链、
嵌套列表）均正确解析（此前版本曾误判为闭合，已修复并加回归测试
`parser::list_in_cond_tests`）。

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
| `..` 格式 | `Stack: [...]` | `[...]` |
| try 捕获的错误消息文本 | Rust VmError 全文（含位置上下文） | 简短消息（"Division by zero" 等关键字一致）；形状 [#t,值]/[#f,消息] 与栈回卷完全一致 |
| JSON `\uXXXX` 代理对 | 完整解码 | BMP-only（罕见，已记录） |
| 能力 `@0-@5` | 支持 | no-op |

N-1（2026-09-10）起 **bool 数值协变两侧一致**：`#t 1 +`→2、
`"ab" #t strnth`→98、`0 1 atan2`→0、`#t bitnot`→-2、`#t 0.5 +`→1.5
（VM 三咽喉点协变 + typecheck unify 数值协变；16 格矩阵见
tests/n1_preregistered.md，覆盖门禁 4 用例锁死）。

N-3（2026-09-10）起 **布尔输出与 JSON 布尔两侧一致**（WT_BOOL 第五
值标签）：`#t .`→#t、`1 2 < .`→#t、`#t json-stringify`→true、
`"[true]" json-parse 0 @nth .`→#t、`{ 42 } try . .`→42/#t、
`#t:0.5 unwrap .`→#t——bool? 谓词精确判定（I64 1 非 bool）。相等
语义对齐 VM：仅 Bool↔I64 跨标签数值比（Bool↔F64 两侧均为假）。

L 阶段（2026-09-10）起两实现全量对齐：try（setjmp/longjmp 真错误恢复）、
json-parse/json-stringify（对象键排序对齐 serde_json BTreeMap）、
`#t:0.75` 置信度字面量 + conf-of/unwrap（WT_SIG 透明包装）在 C 链全部
落地——tools/opcode_coverage.sh 对拍 0 失败、白名单为空。

浮点算术/比较/打印（`1.5 2.5 + .`、`1.5 2.5 < .`、`3.14 .`）两实现已一致
（K-4 数值提升修复，opcode_coverage.sh 对拍锁死）。

## 5. 一致性强制

- `tests/conformance/*.ws`（25 项）— 每个语法构造一个程序，双实现输出必须一致
- `scripts/syntax-gate.sh` — 执行上述对拍（CI 强制）
- `scripts/stdlib-gate.sh` — stdlib 24 项门槛（CI 强制）
- `scripts/verify-fixedpoint.sh` — 自举固定点字节级验证（CI 强制）
- `scripts/typecheck-gate.sh` — 类型检查假阳性棘轮（CI 强制；lexicon
  语料无仓库时跳过）

**类型检查语义（N-2，2026-09-10 起）**：`whisper run` 前的类型检查错误
**默认致命**；`--no-strict-types` 降级为警告。前置条件为全套零假阳性
（三语料实测 FP=0：encoding 0 / conformance 0 / lexicon 0，真阳性
不罚——详见 docs/typecheck-strict-design.md 收官注与
scripts/typecheck-gate.sh 棘轮）。构建目标（wbin/c2/native）不做
Rust 侧类型检查，不受影响。

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

## 13. 词汇附录 v1.0.8（FFI：二进制轮子加载）

语法冻结不动；本附录新增 3 个内建词（字节 150-152）。**这是语言中
最强的能力**——可调用任意 native 函数（包括读写文件、起进程的函数），
授权语义等同 shell 访问。

| 词 | 字节 | 栈效应 | 说明 |
|----|------|--------|------|
| `ffi-open` | 150 | path → handle \| -1 | dlopen/LoadLibrary 加载 .dll/.so |
| `ffi-sym` | 151 | handle name → fnptr \| 0 | 查符号地址 |
| `ffi-call` | 152 | [args] fmt fnptr → result | 按约定调用 |

**fmt 约定**：首字符为返回类型，其余为参数类型（按序）。
类型字符：`i`=i64、`p`=字符串数据指针（Whisper 字符串直接传
`const char*`；长度用单独的 `i` + `strlen` 传）、`d`=f64（v1.0.8
**仅返回值支持**，f64 参数不支持）、`v`=void（返回寄存器位模式）。
参数上限 8 个（寄存器传参的通用垫板覆盖 win-x64 与 SysV 整数/指针
路径）。

验收基准：`crc32`——`[0 "123456789" 9] "iipi" rot ffi-call` →
`3421780262`（标准 CRC-32 校验值；三路一致：纯 C / Rust VM / c2）。

**已记录差异**：`ffi-open` 失败时 VM 返回 -1，C 链返回 0（NULL
指针直接转型）；`ffi-call` 参数数不符时 C 链静默返回 0。

包管理（v1.0.8）：manifest 新增 `native: ["lib.png"]` 字段声明随包
分发的二进制轮子；安装时与 .ws 源码同目录落盘，程序用相对路径
`ffi-open` 加载。

## 14. 词汇附录 v1.0.8b（字节语义补全）+ 轮子生态

**字节语义漏洞修补**（png-size 轮子开发中暴露的四处不一致，均对齐
§3.5"字符串字节语义"的既定合同）：

| 词 | 修补前 | 修补后 |
|----|--------|--------|
| `read-file` | UTF-8 校验失败→空串（二进制文件不可读） | 字节直读（`from_utf8_unchecked`），坏 UTF-8 存活 |
| `strnth` | `chars().nth()`——二进制上 panic | 字节下标，越界 -1 |
| `strslice` | char boundary 校验拒绝二进制切片 | 字节切片 |
| `.`/trace 打印 | 非法 UTF-8 panic | lossy 显示（数据不受影响） |

**std/ffi 胶水习语模块**：
- `ffi-fn`（libname symname → fnptr）、`ffi-arg1/2/3`（运行时参数表
  构建——列表字面量是编译期常量，`$N` 会被折叠，参数表必须运行时建）
- `ffi-go`（fnptr args fmt → result，标准收尾）
- 三坑文档化：编译期折叠/ffi-call 弹序（fnptr 在顶）/export 过滤

**轮子生态首例**（apps/wheels/png-size）：
- `be32-of`（s off → 大端 i64，字节切片反转 + unpack 实现）
- `png-size`（path → "WxH"）——纯 Whisper 零依赖解析 PNG IHDR
- 验收：自制 8×5 PNG + 系统 WLive48x48.png，与 Python 基线逐位一致，
  双链（VM + c2）一致
- package.ws 声明包元数据

## 15. C 发射契约（可替换后端）

Whisper 的 C 后端（`-t c` / `-t c2` / `-t exe` / `-t native`）把 **C 当作
可移植汇编**，而不是把某个编译器当作依赖。为此锁定以下契约——它是
**对发射产物与运行时（`whisperc/runtime.c`）的要求**，不是对用户程序的要求：

1. **语言子集：C99 + POSIX/Win32**。允许标准库头、posix 头（`unistd.h`/
   `pthread.h`/`sys/socket.h`）与 Win32 头（`winsock2.h`/`windows.h`/
   `process.h`），二者按平台 `#ifdef` 二选一。
2. **禁止编译器特有扩展**。`__attribute__`、`typeof`、`__builtin_*`、
   语句表达式 `({...})`、内联汇编、`#pragma`、`__int128` 一律不得依赖。
   线程局部存储用条件宏落到 C11 `_Thread_local`（MSVC 走
   `__declspec(thread)`），不使用裸 `__thread`。
3. **不得要求非默认编译标志**。特别地 `-fno-strict-aliasing` 已移除
   （2026-09-21）：运行时对位模式的转换全部走 `memcpy`，静态审计无类型
   双关；实测同一份 `compiler.all.ws` 产出的 C 在带/不带该标志下编译，
   编译同一程序的结果**逐字节一致**。
4. **不得依赖未定义行为来「碰巧工作」**。位模式转换一律 `memcpy`；
   新的此类需求出现时，先证明再写，不留「加个标志就行」的债。
5. **编译器矩阵**：至少 gcc 与 clang 两条腿（CI `c-backend-clang`），
   可用 `WHISPER_CC` 指定任意 C 编译器。

违反上述任一条即为规范级缺陷——因为它把"C 可替换"这一前提悄悄撤销了。

## 16. 词汇附录 v1.0.9 —— `call`（动态调用引号）

```
{ ... }  call        ;; 调用栈顶的引号，其产出留在原栈上
```

`call` 消费栈顶的引号并执行它。引号的输入取自调用者的数据栈，产出也留在
调用者的数据栈——与 `@map`/`@fold` 这类"消费者"不同，`call` 不提供任何
元素槽，只是一次普通的重入。

**这是把一个既存 opcode 变得可达，不是新语义**：`Opcode::CapExec` 自始
就在 VM 里实现（`Value::Ref(code) => execute_ref(code)`），但一直没有
可达的拼写——设计原意是 `!`，而 `!` 已被逻辑非占用（§3.2 冻结），词法
层的 `TokenKind::Bang` 从此不再被产出，opcode 就此悬空。`call` 是补上
那个缺失的拼写。

**语义**

| 栈顶 | 行为 |
|------|------|
| 引号 | 执行之；产出入调用者栈 |
| 能力令牌 `@n` | 同 `@n` 直接调用（`CapExec` 的另一分支） |
| 其它 | 运行期错误（VM 侧可被 `try` 捕获） |

**可用性（2026-09-21 起两链一致，且与所在位置无关）**：引号是**一等值**——
可以同时出现多个，可以存进列表字面量再取出来调用（动态分发），也可以出现在
条件分支、循环体、词定义体、高阶词体乃至另一个引号体里。上述形态在 VM 与
自举链（`-t c2`/`exe`/`native`）上输出一致；`tools/opcode_coverage.sh` 的
`quote-*` / `qlist-*` / `qcond-*` / `qloop-*` / `qdef-body` / `qquote-nest` /
`qhof-body` 用例即这条双链契约。

自举链的实现方式值得记一笔（它是 §15 契约的一部分）：`fuse-hof` 把"引号作
值"展开成一对 token——定义 `[9, "__qN", body]` 与引用 `[16, "__qN"]`——只有
**顶层**的定义会被 `split-defs` 收走并发射成静态函数 `W___qN`。因此凡是能
"装东西"的节点（列表 `[6,…]`、条件 `[7,…]`、循环 `[8,…]`、词定义 `[9,…]`、
高阶融合体 `[249..254,…]`）内部的定义都必须由 `hs` / `part` 抬到节点之外；
否则 `compile-one` 没有 tag 9 分支，定义会被丢弃，引用指向从未发射的函数。
用嵌套函数（gcc trampoline）内联引号是**不可行**的替代方案：trampoline 在
外层函数返回后失效，引号一旦被存进容器再传出去就是悬空指针。

**示例**

```
{ 42 } call .                 # 42
10 { 2 * } call .             # 20
: ap { call } ; 5 { 1 + } ap .  # 6
[{ 1 + }] 0 @nth 5 swap call .  # 6   （引号存进列表再取出来调用）
[{ 1 + } { 2 * } { 3 * }] 1 @nth 5 swap call .  # 10
#t ?? { 9 } | { 8 } ] call .    # 9   （条件分支里的引号）
: f { { 5 } drop 7 } ; f .      # 7   （词定义体里的引号）
```

## 17. 词汇附录 v1.0.10 —— `print` 与 `exit`（两个缺失的原语）

```
"..."  print         ;; 把栈顶字符串逐字节写 stdout，不补换行
n      exit          ;; 以 n 为退出码终止进程
```

两个词都不是"新功能"，而是**语言缺了两个原语**：在此之前，C 链程序恒以
`return 0;` 结束（任何 Whisper 写的 CLI 都无法向 shell 报告失败），而唯一的
打印词 `.` 自带换行并对字符串做与平台相关的处理（它不是字节通道）。
`striter` 不是打印词，是字符串**迭代器**（推 `[字符码, 余串]`）。

**opcode**：`0x93` = `Print`、`0x94` = `Exit`，紧接 IO 区（`0x90` `.`、
`0x91` `..`、`0x92` 读输入）。

> **选字节的教训**：初稿选了 `0xC8`/`0xC9`，看着空闲——其实那是
> `listdir`/`mkdir`，它们在 `opcode.rs` 里写成**十进制** `200`/`201`，
> 十六进制 grep 扫不到。字节占用必须机器枚举两链（`Opcode::to_byte` ×
> `compiler.all.ws` 的词表与 `emit-bare`），不能眼看。

**语义**

| | VM | C 链 |
|---|---|---|
| `print` | 取栈顶（先 `unwrap_signal`），要求 `str`，`write_all` + `flush` | `w_print_raw`：`fputs` + `fflush` |
| `exit` | 取栈顶，要求 `i64`，`std::process::exit(code)` | `exit((int)code)` |

- 两者类型不符时都报错（`print` 收非字符串、`exit` 收非整数），不静默转换。
- `exit` 是**真终止**，不是"提前 return 到 main"：`exit` 之后的代码不执行，
  调用它的词也不返回。`return`/`leave` 仍是从当前词返回。
- `print` 不消费 `.` 的格式化规则，也不受其影响：`. ` 面向人（补换行），
  `print` 面向字节（转发子进程输出、拼装片段）。

**逐字节的前提**：C 链在 Windows 上默认是文本模式 stdout，会把每个 `\n`
改写成 CRLF，而 VM 从不翻译——两链因此在**每一个换行**上都不一致（此前靠
集成测试里的 `.replace(['\r','"'], "")` 抹平）。现在 `w_init` 在程序启动时把
stdout 设为二进制模式，两链的换行都是单个 LF。**这是行为变更**：C 链程序
（含 `.`）从"CRLF"变为"LF"。

**已知残留**：`print` 的逐字节只保证"写出去的字节 = 栈上字符串的字节"。
若字符串来自 `@5`（exec），尾部空白在**到达 Whisper 之前**已被该能力的
`trim()` 吃掉——见 `docs/language-backlog.md` 的 W-5。

**示例**

```
"hi" print                      # hi        （无换行）
"a" print "b" print             # ab
"x\n" print                     # x + LF    （保留自带换行，不变成 CRLF）
3 exit                          # shell 看到退出码 3
: f { 7 exit } ; f 1 .          # 退出码 7，且不打印 1（真终止）
```

双链契约由 `tools/opcode_coverage.sh` 的 `print-*` / `exit-*` 七例，以及
`crates/whisper-cli/tests/integration_tests.rs` 的
`l1_print_is_byte_exact_on_both_chains`（逐字节比较原始 stdout）与
`l1_exit_sets_a_real_exit_code`（比较真实 `ExitStatus`）把关。

