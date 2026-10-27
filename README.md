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
  <img src="https://img.shields.io/badge/version-1.3.7-7cffc4" alt="version">
  <img src="https://img.shields.io/badge/tests-467%20passed%20%2B%2019%20gates-brightgreen" alt="tests">
  <img src="https://img.shields.io/badge/self--hosting-hard-brightgreen" alt="self-hosting">
  <img src="https://img.shields.io/badge/native-C%20via%20gcc-blue" alt="native">
  <img src="https://img.shields.io/badge/stdlib-20%20modules-blue" alt="stdlib">
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="license">
</p>

---

Whisper 是一门**栈式、后置表示法**的编程语言，专为 **AI 生成和阅读**而设计。极简符号、零信任能力模型（按执行路径分档）、原生置信度系统。

**核心亮点**：编译器（lexer + parser + classifier + cgen）完全用 Whisper 自身编写，生成 C 源码后经 gcc 编译为独立原生可执行文件；从冻结 C 种子（由自举链自身生成，非手写）出发已达**自举固定点**（whisper2 重新编译自身，输出与来源逐字节一致，`scripts/verify-fixedpoint.sh` 可随时复验）。

## 设计哲学

| 原则 | 说明 |
|------|------|
| **Token 极简** | 同样逻辑的 Token 消耗仅为 Python 的 30-60% |
| **栈式执行** | 后缀表示法，无需括号和优先级规则 |
| **能力沙箱** | 目标：VM/WASM 默认无 IO、显式授权；**当前 VM 仍有 ambient/FFI 绕过，native C 无沙箱**（见 [docs/security-model.md](docs/security-model.md)） |
| **置信度原生** | 每个值携带置信度：`:c` 标注 / `conf-of` 窥视 / `unwrap` 剥壳 / `p {a} {b} ?|` 概率分支（std/conf 已定稿） |
| **双模态** | 文本 `.ws` 和二进制 `.wbin` 等价 |
| **硬自举** | 编译器自身用 Whisper 编写，输出原生代码 |

## 快速开始

### 安装（自举链，只需 gcc）

```bash
git clone https://github.com/Myoucai/Whisper.git
cd Whisper
bash scripts/bootstrap.sh
# seed(C) -> whisper1 -> whisper2 -> wd，固定点自检 + 两次冒烟
./target/bootstrap/wd run hello.ws hello    # 驱动自带 import 内联，零 Rust
./target/bootstrap/whisper2 -c < hello.ws > hello.c
gcc -O2 -o hello hello.c -I whisperc -lm && ./hello
```

`bootstrap.sh` 最后两步会用刚造出来的 `whisper2` 编出 **`wd`**（见下），所以
一条命令就能从冻结种子走到「能编能跑任何程序」，全程只用 gcc。

### 零 Rust 驱动 `wd`（缓存已落地）

`whisperc/wd.ws` 是一个**用 Whisper 自己写的驱动**，把上面那三行手工胶水
（内联 import、调 gcc、跑产物）收进一条命令。它同样只需 gcc + 冻结种子：

```bash
bash scripts/build-wd.sh              # 全程无 cargo
./target/wd/wd build hello.ws hello   # 编译，留下 hello.c
./target/wd/wd run   hello.ws hello   # 编译并跑起来
```

> CI 的 `wd-build` job 用 `bash scripts/build-wd.sh --fast`：Rust CLI 只把
> `wd` 发射成 C，运行期仍是 C；生产/零 Rust 验证仍用不带 `--fast` 的三段链。

**当前范围（如实说明）**：能编含 `import` 的程序（`std/core` 自动补、命名
空间限定名按 `math.sq` 形式可用），与 `-t c2` 的 C 输出**逐字节一致**（自包含、
带 import、多行定义四类程序由 `w2_wd_output_matches_c2_byte_for_byte` 常驻
钉住）。`wd` 的失败路径返回 1（构建/IO）或 2（用法），转发用 `print` + `@10`
逐字节。`wd build/run` 走**内容寻址缓存**：键 = hash(生成的 C ++ `runtime.c`
内容 ++ cc 名)，命中直接拷贝产物；改源码、改运行时、换 cc 都会失效，缓存位于
`target/whisper-cache/`。判据见 `tests/wd_cache_preregistered.md`，
门禁 `bash scripts/wd-cache-gate.sh`（C-1..C-6 八条）。

### Whisper 写的 `fmt`（Step 4 第一项）

`whisperc/fmt.ws` 是 Step 4 的第一片：纯 Whisper 写的布局规范化器，
由 `wd` 零 Rust 构建：

```bash
bash scripts/build-wfmt.sh                 # -> target/wfmt/wfmt
./target/wfmt/wfmt src.ws                  # 格式化到 stdout
./target/wfmt/wfmt src.ws src.ws           # 就地格式化
```

v1 只做安全布局：CRLF→LF、字符串外的行首缩进/行尾空白去掉；字符串内容、
注释正文、token 间距都不动。多行字符串、转义引号和注释里的引号都由逐行
状态机处理。判据与执行记录见 `tests/step4_fmt_preregistered.md`，
门禁 `bash scripts/fmt-gate.sh`（F-1..F-6）。AST 重排留到 v2。

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
| IO | `.` 输出栈顶, `..` 输出全部, `,` 读输入, `print` 裸打印（无换行，逐字节）, `exit` 以退出码终止进程 |
| 交互 | `read-line`（stdin 一行，EOF→空串） |
| 字符串 | `strcat strlen strslice streq strfind strreplace strsplit strjoin strmatch strfind-re str-upper str-lower str-strip fmt ctos` |
| 文件/目录 | `read-file write-file listdir mkdir file-exists` |
| 时间 | `now strftime sleep` |
| 二进制 | `pack unpack`（struct 小端打包） |
| 正则 | `strmatch strfind-re`（子集：`. * + ? [] ^$ \d\w\s | (...)`) |
| 网络 | `listen accept recv send serve-concurrent` |
| KV | `kv-set kv-get kv-del kv-keys`（进程级 str→str 共享存储） |
| 生成 | `range`（`start count → [start..start+count-1]`，预分配） |

> native 目标的网络默认保持历史行为开启；启动器可设 `WHISPER_NET=0` 让
> 本次进程拒绝所有网络 opcode。`WHISPER_NET_ALLOW=host1,*.example.com`
> 可把 HTTP/HTTPS 目标限制到白名单；配置白名单后默认拒绝
> loopback/私网/link-local/云元数据，可用 `WHISPER_NET_ALLOW_PRIVATE=1`
> 显式放行本地测试目标。`WHISPER_NET_METHODS=GET,POST`、
> `WHISPER_NET_PORTS=80,443`、`WHISPER_NET_MAX_RESPONSE=1048576`、
> `WHISPER_NET_TIMEOUT=30` 可进一步限制方法、端口、响应大小和连接/读写超时。
> `WHISPER_NET_REDIRECT=manual`（默认）原样返回 3xx；`=deny` 拒绝带
> `Location` 的 3xx；`=follow` 最多跟随 5 次同源重定向，跨源返回原 3xx，
> 301/302/303 改为 GET 并丢弃原 headers/body，307/308 保留方法和 body。
> `WHISPER_NET_RAW=0` 禁止 raw listen/accept/recv/send；allowlist 模式默认
> 关闭 raw，需 `WHISPER_NET_RAW=1` 显式开启，可用
> `WHISPER_NET_LISTEN_PORTS=8080` 限制监听端口。
> 未设置 allowlist 时保持全放行兼容。
> WASM v1 不走这些开关：guest 永远无网络，
> 见 [docs/wasm-v1-release.md](docs/wasm-v1-release.md)。

资源配额：

```bash
WHISPER_MAX_MEMORY_MB=128       # 等价于 WHISPER_HEAP_MB
WHISPER_NET_MAX_REQUESTS=1000   # 进程级 HTTP 请求数
WHISPER_NET_MAX_BODY=1048576    # 请求 body 字节
WHISPER_NET_MAX_HEADERS=65536   # 请求/响应 header 字节
WHISPER_NET_MAX_RESPONSE=...    # 响应 body 字节
WHISPER_NET_MAX_CONNECTIONS=8   # @9 并发上限
```

未设置时不额外限制。

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
# 用自举编译器编译，把生成的 C 打到 stdout（与 build -t c2 同一条流水线）
whisper bootstrap hello.ws > hello.c
gcc -O2 -o hello hello.c -I whisperc -lm
```

**自举管线**：这条流水线只有**一份源码**——`whisperc/compiler.all.ws`，
`tokenize → structify → classify → compile-one → cgen` 全在里面。
`whisper bootstrap`、`-t c2`、`scripts/bootstrap.sh`、`scripts/build-wd.sh`
走的都是它；集成测试 `bootstrap_matches_c2_byte_for_byte` 逐字节钉住
`whisper bootstrap` 与 `build -t c2` 的输出相同。

> 2026-09-21 收口战役删掉了曾经并存的**第二棵编译器**
> （`whisperc/lexer.ws` + `classify.ws` + `parser.ws` + `main.ws` +
> `src/compile.ws`）与驱动的 `soft`/`hard` 两种模式。实测那棵树只是部分可用
> （`@map` 融合、import 展开、`print`/`exit` 都不行），而且与权威编译器之间的
> 漂移没有任何机制把关。`--hard` 现在只是历史别名，接受它但不改变行为。

## 构建目标

| 目标 | 命令 | 输出 | 依赖 |
|------|------|------|------|
| 字节码 | `whisper build file.ws` | `.wbin` | Rust VM |
| C 源码 | `whisper build file.ws -t c -o prog.c` | C 源码 | gcc 编译为原生可执行文件 |
| **c2 自举链** | `whisper build file.ws -t c2 -o prog` | 原生可执行文件 | gcc |
| **native 特化** | `whisper build file.ws -t native -o prog` | 按主机 CPU 特化的原生可执行文件（内容寻址缓存） | gcc |
| **exe 原生** | `whisper build file.ws -t exe -o app` | 原生可执行文件（**产物零 Rust**） | gcc（构建期） |
| bundle 打包 | `whisper build file.ws -t bundle -o app` | 单文件可执行（**内含 Rust VM**，构建期无需工具链） | 无 |
| **wasm（M2 部分）** | `wd wasm file.ws app.wasm` 或 `bash scripts/build-wasm.sh ...` | `.wasm` + `.wasm.c`（零 Rust runtime） | Zig cc / wasi-sdk clang + WAMR/wasm3/wasm2c |
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

## 安全与执行模式（重要）

Whisper 的安全承诺**按执行路径分档**，不是所有产物都等价：

| 模式 | 命令 | 语言级沙箱 | 说明 |
|------|------|------------|------|
| VM 解释 | `whisper run` | ⚠️ 部分 | `@n` 有 `CapabilityTable`，但 `read-file`/`write-file`/`getenv`/`ffi-*` 等 ambient opcode 目前可绕过；**暂不作为不可信代码边界** |
| bundle | `-t bundle` | ⚠️ 部分 | 内含 Rust VM，同上；体积也更大 |
| C / 原生 | `-t c`/`-t c2`/`-t exe`/`-t native`/`wd` | ❌ 无 | 直接使用 OS 用户权限；**trusted-only**，不要用来跑不可信源码 |
| WASM | `wd wasm` / `bash scripts/build-wasm.sh` | 🟡 M2 部分 | core 27/27、安全门禁 20/20、symlink/import/ro 已通；host HTTP 代理未完成，**尚不可用于生产** |

当前已确认的缺口（2026-10-20 实测）：

- VM 默认拒绝 `@0`，但 `"README.md" read-file strlen .` 直接成功；
- VM 默认配置下 `"msvcrt.dll" ffi-open .` 直接拿到 handle；
- `-t c2` 产物无授权直接读文件、执行 `@5` 命令。

**WASM v1（无网络）已在 v1.1.0 发布，不是多租户生产沙箱**；不可信
代码仍建议放在 OS 沙箱/容器里运行。支持矩阵见
[docs/wasm-v1-release.md](docs/wasm-v1-release.md)，当前版本说明见
[docs/release-notes.md](docs/release-notes.md)，后续优先级见
[docs/roadmap.md](docs/roadmap.md)；完整威胁模型见
[docs/security-model.md](docs/security-model.md)。

WASM/WASI 的详细计划与判据见 [docs/wasm-wasi-plan.md](docs/wasm-wasi-plan.md)
和 [tests/wasm_preregistered.md](tests/wasm_preregistered.md)。当前 M2 部分已打通：

```bash
# 需要 Zig（或 wasi-sdk clang）与 C 运行时 WAMR iwasm；不需要 Rust runtime
bash scripts/build-wasm.sh docs/examples/hello.ws target/hello.wasm
wd wasm docs/examples/hello.ws target/hello-wd.wasm   # 零 Rust 的 wd 入口
iwasm target/hello-wd.wasm                 # Hello, World!
bash scripts/wrun.sh target/hello.wasm     # 默认无 preopen/env

# 显式给 /data 预打开目录（v1 为 rw；--env NAME 逐个放行）
bash scripts/wrun.sh --dir /data::target/data app.wasm

# 只读视图：复制到临时快照，guest 写入不落宿主
bash scripts/wrun.sh --dir-ro /data::target/data app.wasm

# 真正只读（Linux）：私有 user/mount namespace + ro bind mount
bash scripts/wrun.sh --dir-ro-strict /data::target/data app.wasm

# WASI 资源配额
WHISPER_WASM_MAX_MEMORY_MB=64 \
WHISPER_WASM_STACK_KB=256 \
WHISPER_WASM_MAX_THREADS=4 \
WHISPER_WASM_TIMEOUT=30 \
bash scripts/wrun.sh app.wasm
```

> **不要用裸 `iwasm --map-dir` 跑不可信代码**：WAMR 会跟随 preopen 里的
> symlink 逃逸到目录外。`wrun` 会拒绝含 symlink 的 preopen（S-17）；
> 裸 iwasm 没有这层检查。`wrun` 还需要 python3 做 import 白名单检查
> （只允许 `wasi_snapshot_preview1`，S-18）。

工具链状态用 `bash scripts/wasm-preflight.sh` 检查（缺失只报告，
不阻塞现有 CI）。

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

按能力令牌分两档，**两条链的可用范围不同**：

| 令牌 | 词 | 协议 | Rust VM | C 链（`-t c2`/`exe`/`native`） |
|------|----|------|---------|------------------------------|
| `@2` | `http-get` | 仅 `http://` | ✅ | ✅（2026-09-21 起，逐行对拍一致） |
| `@3` | `http-post` | 仅 `http://` | ✅ | ✅（同上） |
| `@6` | `http-req` | `http://` + **`https://`**（VM: rustls；C: 动态 OpenSSL） | ✅ | ✅ status/headers/body、cookie、用户 headers、chunked；audit 部分 |
| `@7` | `http-audit` | 导出会话审计 | ✅ | ✅ 文本行；`WHISPER_NET_AUDIT=json` 输出 host/status/bytes |
| `@8` | `http-req-raw` | 同 `@6`，body 为 hex 原始字节 | ✅ | ✅ 二进制 body（Content-Type 由 user headers 提供） |
| `@9` | `http-req-parallel` | 同 `@6`，count 份并发 | ✅ | ✅ count 1..16，按请求序聚合 |
| `@10` | `exec-raw` | 同 `@5`，但两个流**不 trim** | ✅ | ✅（2026-09-21 起） |

`@6` 家族另有 cookie 罐、每请求审计行、`Transfer-Encoding: chunked` 自动解码、
host 白名单（零出域）、连接 10s/读写 15s 超时（挂死端点化为 status 0 无证据响应）。
C 链 `@6` 已支持 HTTP/HTTPS 的 status/headers/body、cookie jar、用户
headers、chunked 解码；`@7` 默认返回 `METHOD URL STATUS` 文本行，
设置 `WHISPER_NET_AUDIT=json` 返回含 `method/url/host/status/bytes` 的
JSON 数组；`@8` 支持 hex 原始 body；`@9` 支持 1..16 份并发请求。

C 链响应 headers 与 VM 对齐：只暴露 `Location` 和 `Content-Type`；
`Set-Cookie` 只进 cookie jar，不暴露；其余响应头（Date/Server 等）隐藏。
C 链 HTTPS 通过运行时动态加载 libssl（不增加链接依赖）：
找不到 libssl 或没有受信任根时显式失败；可用
`WHISPER_TLS_CA=/path/ca.pem` 指定 CA。证书链和 hostname 校验默认开启。
macOS 使用 Homebrew `openssl@3` 的绝对路径，CI 已覆盖。

> **`@2`/`@3` 不支持 HTTPS**——VM 侧同样拒绝（`https://` 会直接报错，
> 而不是把明文发到 443）。需要 HTTPS 请用 `@6`（C/VM 均已支持）。
> `@2`/`@3` 的拒绝语义一致，并有测试断言**拒绝发生在建连之前**。

> c2 链的 HTTP 会话能力已覆盖 `@6`-`@9`；其余能力——`@0` 文件读、
> `@1` 文件写、`@4` 环境变量、`@5` 执行命令、`@10` 执行命令（原样流）
> ——两条链均已实现并逐行对拍一致。详见
> `docs/independence-design.md` Step 2a/2b 与 `tests/step2b_preregistered.md`。
> **这里的“实现”指语义/输出对拍，不是授权**：C 链没有 `CapabilityTable`，
> 也没有 policy；`@0`/`@5` 在 native 里直接调用宿主。
>
> **`@10` 与 `@5` 是同一份授权的两种结果形状**：CLI/VM 侧绑 `@5` 时一起绑 `@10`，
> 未授权时 `@10` 一样被拒（`Capability not bound: @10`）。`@5` 的 `trim` 是
> 命令替换惯用法依赖的行为，故保留；逐字节转发（`wd run`）用 `@10`。
> **注意**：C 原生产物没有 CLI，也没有这层绑定；`@5`/`@10` 在 native 里是
> 直接 `popen`，见 [docs/security-model.md](docs/security-model.md)。

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
| 零 Rust 驱动 `wd` + 内容寻址缓存 | ✅ |
| Whisper 写的 `fmt`（布局规范化 v1，`wd` 构建） | ✅ |
| C 转译后端 (gcc 编译为原生可执行) | ✅ |
| 正则子集 (strmatch/strfind-re，双链手写匹配器) | ✅ |
| struct 二进制打包 (pack/unpack，小端) | ✅ |
| 共享 KV 存储 + 并发 Web 服务器 (serve-concurrent) | ✅ |
| 循环控制 (leave，编译期跳转回填) | ✅ |
| 静态类型推导（strict 默认：类型错误致命，`--no-strict-types` 降级） | ✅ |
| 能力沙箱 | ⚠️ VM 的 `@n` 部分；ambient/FFI 待收口；C 链无 |
| WASM/WASI 安全执行目标 | 🟡 M2 部分（core 27/27；`wd wasm`；安全 20/20） |
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
whisper bootstrap hello.ws        # 自举编译器 → C 源码（stdout）
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
├── docs/                  # 规范/教程/安全模型/WASM-WASI 计划/示例
└── .github/workflows/     # CI/CD
```

## 用 Whisper 写的 Web 后端



路由分发、JSON 响应、静态文件、`WS_PORT` 环境变量配置——全部由 Whisper 编写，
自举编译器编译。10 路并发全路由验证通过（v1.0.2 每连接一线程）。

## 测试

```bash
cargo test --workspace                           # 467 tests: all passed
bash scripts/syntax-gate.sh                      # 27 语法断言（conformance 语料，VM 与 c2 双侧）
bash scripts/stdlib-gate.sh                      # 67 双链 stdlib 断言（VM+c2 输出一致）
bash scripts/net-gate.sh                         # 真实 HTTP 服务 + curl
bash scripts/app-gate.sh                         # 13 路由 Web 应用断言
bash tools/opcode_coverage.sh                    # 101 项逐特性双跑（VM vs 自举链）
bash scripts/typecheck-gate.sh                   # 类型检查假阳性棘轮（三语料 FP=0）
bash scripts/wd-cache-gate.sh                    # wd 内容寻址缓存 C-1..C-6（8 条断言）
bash scripts/wd-c-gate.sh                        # wd c 零 Rust C 发射（C-1..C-5）
bash scripts/wd-wasm-gate.sh                     # wd wasm 零 Rust C→wasm（W-1..W-4）
bash scripts/wasm-security-gate.sh               # WASI 安全 S-1..S-20（需 Zig/WAMR）
bash scripts/wasm-conformance.sh                 # WASM core conformance 27/27
bash scripts/https-gate.sh                       # native @6 HTTPS/CA/hostname（需 OpenSSL）
bash scripts/http6-gate.sh                       # native @6 cookie/用户 header/audit
bash scripts/net-allow-gate.sh                   # native WHISPER_NET_ALLOW 白名单
bash scripts/http-parity-gate.sh                 # @6/@7/@8 + @9/302 VM/C 对拍
bash scripts/http9-gate.sh                       # native @9 并发扇出
bash scripts/wrun-gate.sh                        # wrun 策略 W-1..W-7（需 WAMR）
bash scripts/build-wfmt.sh && bash scripts/fmt-gate.sh  # Whisper 写的 fmt（F-1..F-6）
bash scripts/verify-fixedpoint.sh                # 自举固定点字节级复验
```

测试覆盖（467 项，`cargo test --workspace` 实测）：
- 133 项 CLI（4 单元 + 6 基准 + 123 集成：stdlib 导入冒烟、关键字作 def 名回归、软/硬自举管线、c2 链端到端、能力 ABI 与 `call`/多引号/五种容器内引号双链对拍、`print` 逐字节与 `exit` 真退出码、CRLF 归一化、**非 ASCII 字节语义与 stdlib 字节管线**、`-t c` 的能力 opcode、`@10` exec-raw、`wd` 与 `-t c2` 的 C 输出逐字节相同、`whisper bootstrap` 与 `-t c2` 逐字节相同、exe 产物零 Rust 断言、bundle 尾负载）
- 135 项核心 VM（124 单元 + 7 模糊测试 2500+ 随机程序 + 4 项 opcode 元数据）
- 139 项 codegen（62 单元 + 65 流水线 + 12 C 后端端到端 gcc）
- 38 项 parser（含「每个关键字都可作 def 名」参数化回归、CRLF 归一化四条）
- 15 项 typecheck + 5 项 LSP + 2 项包管理器
- 优化器语义保持测试（优化前后执行结果必须一致）
- CI 含 selfhost job：种子编译 → 22 核心用例双模式 → 9 道门禁（固定点 + stdlib/缓存/fmt/覆盖/类型/语法/网络/应用）

## License

MIT
