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
  <img src="https://img.shields.io/badge/version-1.0.0-7cffc4" alt="version">
  <img src="https://img.shields.io/badge/tests-277%20passed-brightgreen" alt="tests">
  <img src="https://img.shields.io/badge/self--hosting-hard-brightgreen" alt="self-hosting">
  <img src="https://img.shields.io/badge/native-elf-blue" alt="native">
  <img src="https://img.shields.io/badge/stdlib-9%20modules-blue" alt="stdlib">
  <img src="https://img.shields.io/badge/opcodes-62-orange" alt="opcodes">
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="license">
</p>

---

Whisper 是一门**栈式、后置表示法**的编程语言，专为 **AI 生成和阅读**而设计。极简符号、零信任安全、原生置信度系统。

**核心亮点**：编译器（lexer + parser + classifier + codegen）完全用 Whisper 自身编写，直接生成原生 ELF x86-64 可执行文件，**零外部依赖**。

## 设计哲学

| 原则 | 说明 |
|------|------|
| **Token 极简** | 同样逻辑的 Token 消耗仅为 Python 的 30-60% |
| **栈式执行** | 后缀表示法，无需括号和优先级规则 |
| **能力沙箱** | 默认无 IO，所有副作用必须显式授权 |
| **置信度原生** | 每个值携带置信度，原生支持概率编程 |
| **双模态** | 文本 `.ws` 和二进制 `.wbin` 等价 |
| **硬自举** | 编译器自身用 Whisper 编写，输出原生代码 |

## 快速开始

### 安装

```bash
git clone https://github.com/Myoucai/Whisper.git
cd Whisper
cargo build --release
./target/release/whisper --help
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
| @词 | `@map` `@each` `@fold` `@nth` `@times` |
| IO | `.` 输出栈顶, `..` 输出全部, `,` 读输入 |

## 自举

Whisper 实现了**硬自举**——编译器完全由自身编写，Rust 仅提供最小化 VM 执行字节码。

```
 .ws 源码 → [lexer.ws] → [classify.ws] → [parser.ws] → [main.ws] → 字节码 → 原生 ELF
              ▲               ▲               ▲              ▲
              └───────────────┴───────────────┴──────────────┘
                          全部用 Whisper 编写 (whisperc/)
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
| ELF 原生 | `whisper build file.ws -t native -o prog` | 可执行文件 | **零依赖** |

```bash
# 直接生成独立 ELF 可执行文件 (Linux x86-64)
whisper build hello.ws -t native -o hello && chmod +x hello && ./hello
# → "Hello, World!"
```

原生 ELF 后端实现了全部 **62 个操作码**，包括：算术、比较、逻辑、栈操作、列表操作（含 `@map`/`@each`/`@fold`/`@times`）、字符串操作（14 个）、浮点数学（三角函数）、JSON 解析/序列化、文件 IO、条件/循环/调用/返回。程序完全独立运行，无需任何运行时。

## 功能特性

| 功能 | 状态 |
|------|------|
| 栈式虚拟机 (62 opcodes) | ✅ |
| 硬自举编译器 (纯 Whisper) | ✅ |
| ELF 原生二进制生成 (全部 62 opcode) | ✅ |
| 静态类型推导 | ✅ |
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
| 字符串操作 (14 ops) | ✅ |
| 标准库 (9 模块, 48 函数) | ✅ |

## CLI 命令

```bash
whisper run      hello.ws         # 运行源文件
whisper build    hello.ws         # 编译为 .wbin
whisper build    hello.ws -t native  # 编译为 ELF 原生
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
│   ├── whisper-codegen/   # 字节码, .wbin, ELF
│   ├── whisper-package/   # 包管理器
│   ├── whisper-lsp/       # LSP 语言服务器
│   └── whisper-cli/       # CLI 入口
├── whisperc/              # 自举编译器 (纯 Whisper 实现)
│   ├── lexer.ws           #   词法分析器
│   ├── classify.ws        #   Token 分类器 + 递归分类
│   ├── parser.ws          #   结构分组 (替代 Rust structify_chunks)
│   └── main.ws            #   字节码编译器 + 完整管线
├── stdlib/                # 标准库 (.ws 源码)
├── vscode-extension/      # VS Code 插件
├── docs/                  # 文档和示例
└── .github/workflows/     # CI/CD
```

## 测试

```bash
cargo test --workspace
# 277 tests: 277 passed, 0 failed
```

测试覆盖：
- 31 个自举/自举验证测试（含硬自举管线对比）
- 62 个原生 ELF 后端测试
- 69 个核心 VM 测试
- 38 个解析器测试
- 43 个类型检查测试
- 54 个代码生成测试
- 模糊测试（2500+ 随机程序）

## License

MIT
