<p align="center">
  <img src="vscode-extension/icons/whisper.svg" width="96" alt="Whisper logo">
</p>

<h1 align="center">Whisper</h1>

<p align="center">
  <strong>AI-native, stack-based programming language. Built for data flow.</strong>
</p>

<p align="center">
  <a href="#-quick-start">Quick Start</a> ·
  <a href="#-syntax">Syntax</a> ·
  <a href="#-self-hosting">Self-Hosting</a> ·
  <a href="#-features">Features</a> ·
  <a href="#-build-targets">Build Targets</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.0.8-7cffc4" alt="version">
  <img src="https://img.shields.io/badge/tests-465%20passed%20%2B%207%20gates-brightgreen" alt="tests">
  <img src="https://img.shields.io/badge/self--hosting-hard-brightgreen" alt="self-hosting">
  <img src="https://img.shields.io/badge/native-C%20via%20gcc-blue" alt="native">
  <img src="https://img.shields.io/badge/stdlib-20%20modules-blue" alt="stdlib">
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="license">
</p>

---

Whisper 是一门**栈式、后置表示法**的编程语言，专为 **AI 生成和阅读**而设计。极简符号、零信任安全、原生置信度系统。

**核心亮点**：编译器（lexer + parser + classifier + cgen）完全用 Whisper 自身编写，生成 C 源码后经 gcc 编译为独立原生可执行文件；从冻结 C 种子（由自举链自身生成，非手写）出发已达**自举固定点**（whisper2 重新编译自身，输出与来源逐字节一致，`scripts/verify-fixedpoint.sh` 可随时复验）。

## 设计哲学

| 原则 | 说明 |
|------|------|
| **Token 极简** | 同样逻辑的 Token 消耗仅为 Python 的 30-60% |
| **栈式执行** | 后缀表示法，无需括号和优先级规则 |
| **能力沙箱** | 默认无 IO，所有副作用必须显式授权；HTTPS（rustls）+ host 白名单 + 审计 |
| **置信度原生** | 每个值携带置信度：`:c` 标注 / `conf-of` 窥视 / `unwrap` 剥壳 / `p {a} {b} ?|` 概率分支（std/conf 已定稿） |
| **双模态** | 文本 `.ws` 和二进制 `.wbin` 等价 |
| **硬自举** | 编译器自身用 Whisper 编写，输出原生代码 |

## 快速开始

### 安装（自举链，只需 gcc）

```bash
git clone https://github.com/Myoucai/Whisper.git
cd Whisper
bash scripts/bootstrap.sh
# seed(C) -> whisper1 -> whisper2，固定点自检 + stdlib 冒烟
./target/bootstrap/whisper2 -c < hello.ws > hello.c
gcc -O2 -o hello hello.c -I whisperc -lm && ./hello
```

### 安装（Rust 工具链，含 REPL/LSP/c2 一键编译）

```bash
git clone https://github.com/Myoucai/Whisper.git
cd Whisper
cargo build --release
./target/release/whisper --help          # run/build/check/repl/...
./target/release/whisper build hello.ws -t c2 -o hello   # 自举链一键到底
```

### 第一段代码

```bash
echo '"Hello, World!" .' > hello.ws
whisper run hello.ws
# → "Hello, World!"
```

仅 **2 个 Token**。对比 Python: `print("Hello, World!")` 需要 6 个。

## 语法

### 算术（后置表示法）

```
3 4 + .        # 7
10 3 - .       # 7
5 6 * .        # 30
```

### 栈操作

```
5 _ * .        # 25  (dup: 5→5,5; mul: 25)
3 4 ` - .      # 1   (swap: 3,4→4,3; sub: 1)
42 drop .      #     (drop: 移除栈顶)
```

### 条件分支

```
5 3 > ?? 100 | 0 ] .     # 100  (5>3 为真)
2 3 > ?? 100 | 0 ] .     # 0    (2>3 为假)
```

### 词定义

```
: sq { _ * } ;
: cube { _ sq * } ;
5 sq .            # 25
3 cube .          # 27
```

### 递归

```
: factorial { _ 1 > ?? _ 1 - factorial * | drop 1 ] } ;
5 factorial .     # 120

: fib { _ 1 > ?? _ 1 - fib ` 2 - fib + | drop ] } ;
10 fib .          # 55
```

### 列表操作

```
[1 2 3 4 5] len .              # 5
[1 2 3 4 5] 0 { + } @fold .   # 15
[1 2 3 4 5] { _ * } @map .    # [1 4 9 16 25]
```

### 操作符速查

| 类别 | 符号 |
|------|------|
| 栈 | `_` dup, `` ` `` swap, `drop` drop, `@` rot, `$N` pick |
| 算术 | `+` `-` `*` `/` `%`(取模) |
| 比较 | `=` `<` `>` `!=` `<=` `>=` |
| 逻辑 | `&` and, `\|` or, `!` not |
| 控制 | `??..\|..]` 条件, `#` 循环, `?->` 单分支 |
| 引号 | `call`（调用栈顶引号）, `try`（捕获失败）；引号是一等值——可存进列表、条件分支、循环体、词定义体或高阶词体，再取出来 `call`（动态分发），两链一致 |
| @词 | `@map` `@each` `@fold` `@filter` `@nth` `@times` |
| 循环控制 | `leave`（退出最内层 `#` 循环） |
| IO | `.` 输出栈顶, `..` 输出全部, `,` 读输入 |
| 交互 | `read-line`（stdin 一行，EOF→空串） |
| 字符串 | `strcat strlen strslice streq strfind strreplace strsplit strjoin strmatch strfind-re str-upper str-lower str-strip fmt ctos` |
| 文件/目录 | `read-file write-file listdir mkdir file-exists` |
| 时间 | `now strftime sleep` |
| 二进制 | `pack unpack`（struct 小端打包） |
| 正则 | `strmatch strfind-re`（子集：`. * + ? [] ^$ \d\w\s | (...)`) |
| 网络 | `listen accept recv send serve-concurrent` |
| KV | `kv-set kv-get kv-del kv-keys`（进程级 str→str 共享存储） |
| 生成 | `range`（`start count → [start..start+count-1]`，预分配） |

## 自举

Whisper 实现了**硬自举**——编译器完全由自身编写。Rust 只是开发宿主（承载 VM 以运行自举编译器、提供 REPL/LSP）；引导只需 gcc，不需要 Rust。

```
冻结 C 种子 (seed/compiler_seed.c，由自举链自身生成，仅作引导件)
    │  编译 whisperc/compiler.all.ws (用 Whisper 写的完整编译器)
    ▼
gen1.c → gcc → whisper1 ──再编译自身──▶ gen2.c → gcc → whisper2
                                              │
                            whisper2 编译 compiler.all.ws → gen3.c
                            固定点: gen2.c == gen3.c (逐字节一致, 可复验)
```

```bash
# 软自举 (Rust 解析 + whisperc codegen)
whisper bootstrap hello.ws

# 硬自举 (whisperc 全管线: tokenize → structify → classify → compile)
whisper bootstrap --hard hello.ws
# whisperc: 2 main ops, 0 defs
# rust:     2 main ops, 0 defs
# Rust VM output: "Hello, World!"
# whisperc VM output: "Hello, World!"
```

**自举管线详解**：

| 阶段 | 实现 | 说明 |
|------|------|------|
| 词法分析 | `whisperc/lexer.ws` | 源码 → token 流 |
| 结构分组 | `whisperc/parser.ws` | `{}`→Quote, `[]`→List, `??..|..]`→Cond, `{}{}#`→Loop, `:..;`→Def |
| Token 分类 | `whisperc/classify.ws` | 字符串 → `[type, value]` 对，递归处理结构树 |
| 字节码编译 | `whisperc/main.ws` | 分类 token → opcode 字节码 |

## 构建目标

| 目标 | 命令 | 输出 | 依赖 |
|------|------|------|------|
| 字节码 | `whisper build file.ws` | `.wbin` | Rust VM |
| C 源码 | `whisper build file.ws -t c -o prog.c` | C 源码 | gcc 编译为原生可执行文件 |
| **c2 自举链** | `whisper build file.ws -t c2 -o prog` | 原生可执行文件 | gcc |
| **native 特化** | `whisper build file.ws -t native -o prog` | 按主机 CPU 特化的原生可执行文件（内容寻址缓存） | gcc |
| **exe 原生** | `whisper build file.ws -t exe -o app` | 原生可执行文件（**产物零 Rust**） | gcc（构建期） |
| bundle 打包 | `whisper build file.ws -t bundle -o app` | 单文件可执行（**内含 Rust VM**，构建期无需工具链） | 无 |
| 固定点复验 | `scripts/verify-fixedpoint.sh` | 自举链验证 | gcc |

```bash
# Rust C 转译后端（支持 @map/@fold 等高阶操作）
whisper build hello.ws -t c -o hello.c && gcc -O2 -lm -o hello hello.c && ./hello

# c2：import 自动内联 → 自举编译器（在 Rust VM 上运行）生成 C → gcc 一键到底
whisper build hello.ws -t c2 -o hello && ./hello

# native：c2 + -O3 -march=native -flto，内容寻址缓存（同机重建 = 一次拷贝）
whisper build hello.ws -t native -o hello && ./hello

# exe：原生可执行文件，产物不含任何 Rust（构建期需 gcc，运行期什么都不需要）
whisper build hello.ws -t exe -o app && ./app

# bundle：把 Rust VM 与字节码打包成单文件，构建期连 gcc 都不需要
whisper build hello.ws -t bundle -o app && ./app
```

- **`-t c`（Rust 转译器）**：覆盖字符串/列表/Map/浮点/高阶函数（`@map`/`@fold`/`@times` 等），12 个端到端 gcc 测试把关。
- **`-t c2`（自举链）**：编译器完全由 Whisper 自身编写（`whisperc/compiler.all.ws`），在 Rust VM 上运行并生成 C；支持 import 内联（含 `std/*`）、浮点字面量、循环/条件/递归、高阶函数融合（`@map`/`@each`/`@fold`/`@filter`/`@times` 编译为嵌套函数 + 循环）、v1.0.3-1.0.8 全部新词汇（regex/struct/目录/时间/leave/FFI 等）。C 运行时值带 5 种类型标签（WT_I64/WT_F64/WT_PTR/WT_SIG/WT_BOOL），多态算术（任一侧 f64 即提升）与 VM 逐位一致——`1.5 2.5 +` 原生输出 4，与 VM 相同。已知残留分歧：字符串打印不带引号（VM 带）、`=`/去重按值位等同而非深度相等（`runtime.c` 的 `w_val_eq` 仅比标签+位）。
- **`-t exe` 与 `-t bundle` 的区别**：两者都产出单文件，但分发模型相反。`exe` 走自举链出机器码——**产物零 Rust**，代价是构建期需要 C 编译器；`bundle` 把 Rust VM 与字节码尾部打包——构建期不需要任何工具链，代价是**产物内含整个 Rust VM**（含 LSP/REPL/类型检查器/包管理器）。追求"产物零 Rust"就用 `exe`。
- 固定点（whisper2 编译自身输出逐字节一致）可随时用脚本复验，已纳入 CI。

## 置信度与不确定性（std/conf，已定稿）

值的置信度是语言一等公民：`Signal(v, c)`，官方表面共四个原语 + 两个组合词：

```bash
42 seed!                    # 定种子（不确定性行为可复现）
#t:0.75 conf-of . unwrap .  # 标注 → 窥视置信度(0.75) → 剥壳(#t)；conf-of 不消耗值
0.9 0.5 conf-mul .          # 置信度合取 → 0.45（证据链衰减）
#t:0.7 labeled? .           # 是否 Signal → #t
0.7 { 1 } { 2 } ?| .        # P(上支)=0.7 的概率分支；p 可来自栈顶 F64 或 Signal 的 c
```

铁律：置信度（浮点）不进 kv；持久化用 i64 千分比。

## HTTP 能力（--allow-http）

`http-req`（@6）与 `http-get/post`（@2/@3）支持 `https://`（rustls + webpki 根，
ALPN http/1.1）、`Transfer-Encoding: chunked` 自动解码、cookie 罐、每请求审计行、
host 白名单（零出域）、连接 10s/读写 15s 超时（挂死端点化为 status 0 无证据响应）。

> **c2 链限定**：HTTP 能力（`@2`/`@3`/`@6`-`@9`）目前**仅在 Rust VM 上可用**。
> C 链尚未实现 HTTP/TLS 客户端，用到这些令牌的程序经 `-t c2`/`exe`/`native`
> 编译会失败（gcc 报 `W__<n>` 未定义）。其余能力——`@0` 文件读、`@1` 文件写、
> `@4` 环境变量、`@5` 执行命令——**两条链均已实现并逐行对拍一致**。
> 详见 `docs/independence-design.md` 的 Step 2a / 2b。

## 性能

c2 原生输出的实测（journal 工具，10,000 条记录 538KB，输出重定向）：

| 运行形态 | list | search | 纯启动 |
|---------|------|--------|-------|
| c2 原生 exe (gcc -O2) | **6.1 ms** | 6.3 ms | 2.9 ms |
| Rust VM (release) | 26 ms | 27 ms | 4.0 ms |
| Python 对照 | 20 ms | 20 ms | 13.5 ms |

列表构建已全面预分配（`range`/`strsplit`/`@filter`），此前的 O(n²)
append 模式已消除（`range-to` 10 万元素 83s → 19ms）。

## 功能特性

| 功能 | 状态 |
|------|------|
| 栈式虚拟机 (158 opcodes) | ✅ |
| 硬自举编译器 (纯 Whisper + C 种子，固定点已验证) | ✅ |
| C 转译后端 (gcc 编译为原生可执行) | ✅ |
| 正则子集 (strmatch/strfind-re，双链手写匹配器) | ✅ |
| struct 二进制打包 (pack/unpack，小端) | ✅ |
| 共享 KV 存储 + 并发 Web 服务器 (serve-concurrent) | ✅ |
| 循环控制 (leave，编译期跳转回填) | ✅ |
| 静态类型推导（strict 默认：类型错误致命，`--no-strict-types` 降级） | ✅ |
| 能力安全沙箱 (6 caps) | ✅ |
| 置信度系统 + 概率选择 | ✅ |
| .wbin 二进制格式 | ✅ |
| 包管理器 | ✅ |
| 模块/导入系统 | ✅ |
| VS Code 语法高亮 | ✅ |
| LSP 语言服务器 | ✅ |
| 性能优化器 (常量折叠/窥孔) | ✅ |
| 错误恢复解析器 | ✅ |
| REPL (多行/历史/补全) | ✅ |
| JSON 解析/序列化 | ✅ |
| 浮点数学 (三角函数) | ✅ |
| 字符串操作 (20+ ops，含正则/大小写/格式化) | ✅ |
| 标准库 (20 模块) | ✅ |
| 真实工具 (apps/journal 命令行日记) | ✅ |

## CLI 命令

```bash
whisper run      hello.ws         # 运行源文件
whisper build    hello.ws         # 编译为 .wbin
whisper build    hello.ws -t c       # 转译为 C 源码 (gcc 编译为原生可执行)
whisper check    hello.ws         # 类型检查
whisper repl                     # 交互式 REPL
whisper fmt      hello.ws         # 格式化
whisper bootstrap hello.ws        # 自举管道 (软)
whisper bootstrap --hard hello.ws # 自举管道 (硬)
whisper install  github.com/u/r   # 安装包
whisper install  --local .        # 本地安装
whisper install  --list           # 已安装列表
```

## 项目结构

```
Whisper/
├── crates/
│   ├── whisper-core/      # VM, Value, Opcode, Capability
│   ├── whisper-parser/    # Lexer, Parser, AST, Resolver
│   ├── whisper-typecheck/ # 类型推导引擎
│   ├── whisper-codegen/   # 字节码, .wbin, C 转译
│   ├── whisper-package/   # 包管理器
│   ├── whisper-lsp/       # LSP 语言服务器
│   └── whisper-cli/       # CLI 入口
├── whisperc/              # 自举编译器 (纯 Whisper 实现)
│   ├── compiler.all.ws    #   编译器全源码（权威文件，固定点基准）
│   ├── runtime.c          #   C 运行时（seed 与 c2 共享）
│   └── seed/              #   冻结 C 种子（由自举链自身生成，非手写）
├── stdlib/                # 标准库 (20 个可导入模块 .ws)
├── apps/
│   ├── journal/           #   命令行日记（第一个真实工具）
│   ├── webapp/            #   并发 Web 后端
│   └── wheels/            #   实验场（PNG 解码/尺寸解析等，未纳入 CI）
├── adapters/              #   LoRA 微调产物（qwen30b v4/v5/v6）
├── paper/                 #   论文 + 微调实验数据/脚本
├── tests/conformance/     #   双实现一致性语料（syntax-gate 的输入）
├── tools/                 #   opcode 覆盖门禁
├── vscode-extension/      # VS Code 插件
├── docs/                  # 文档和示例
└── .github/workflows/     # CI/CD
```

## 用 Whisper 写的 Web 后端



路由分发、JSON 响应、静态文件、`WS_PORT` 环境变量配置——全部由 Whisper 编写，
自举编译器编译。10 路并发全路由验证通过（v1.0.2 每连接一线程）。

## 测试

```bash
cargo test --workspace                           # 476 tests: all passed
bash scripts/syntax-gate.sh                      # 26 语法断言（conformance 语料，VM 与 c2 双侧）
bash scripts/stdlib-gate.sh                      # 58 双链 stdlib 断言（VM+c2 输出一致）
bash scripts/net-gate.sh                         # 真实 HTTP 服务 + curl
bash scripts/app-gate.sh                         # 13 路由 Web 应用断言
bash tools/opcode_coverage.sh                    # 80 项逐特性双跑（VM vs 自举链）
bash scripts/typecheck-gate.sh                   # 类型检查假阳性棘轮（三语料 FP=0）
bash scripts/verify-fixedpoint.sh                # 自举固定点字节级复验
```

测试覆盖（476 项，`cargo test --workspace` 实测）：
- 146 项 CLI（31 单元 + 6 基准 + 109 集成：stdlib 导入冒烟、关键字作 def 名回归、软/硬自举管线、c2 链端到端、能力 ABI 与 `call`/多引号/五种容器内引号双链对拍、exe 产物零 Rust 断言、bundle 尾负载）
- 135 项核心 VM（124 单元 + 7 模糊测试 2500+ 随机程序 + 4 项 opcode 元数据）
- 139 项 codegen（62 单元 + 65 流水线 + 12 C 后端端到端 gcc）
- 34 项 parser（含「每个关键字都可作 def 名」参数化回归）
- 15 项 typecheck + 5 项 LSP + 2 项包管理器
- 优化器语义保持测试（优化前后执行结果必须一致）
- CI 含 selfhost job：种子编译 → 22 核心用例双模式 → 7 道门禁（固定点 + stdlib/覆盖/类型/语法/网络/应用）

## License

MIT
