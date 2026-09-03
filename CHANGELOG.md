# Changelog

## Unreleased (2026-09-03b) — typecheck 语义修复、HTTP TLS+chunked、置信度定稿

### typecheck
- `@nth` 元素类型传播（List(T) 索引结果 = T；此前丢失致 `@nth len` 漏报）；
  `append` 元素与列表元素类型统一（`[1 2 3] "x" append` 现在报错；结果类型保持）。
- occurs-check 核实已在 unifier 层（`t := List(t)` 拒绝）并补直测；递归调用
  的 fresh-变参宽容策略不变（防误报的既定设计）。
- 新增 `--strict-types`：类型错误致命化（默认仍为警告打印并继续）。

### HTTP TLS + chunked
- `https://` 全链路支持（rustls 0.23 ring 后端 + webpki 根 + ALPN http/1.1），
  此前 @2/@3 明拒、@6 对 443 端口发明文（静默错误）。
- `Transfer-Encoding: chunked` 自动解码（@2/@3/@6 三路径，截断容错）；
  TLS 侧显式驱动握手（write 在握手期返回 Ok(0) 的自旋陷阱）+ 对端缺
  close_notify 容忍。
- 封闭集成测试：rcgen 自签服务端 + 注入根的客户端 + chunked 响应断言。

### std/conf 定稿
- 官方表面：`<value>:<conf>` 标注、`conf-of`（窥视，不消耗）、`unwrap`（剥壳）、
  `p {a} {b} ?|`（显式概率分支，L-03 语义）+ 组合词 `conf-mul`/`labeled?`；
  文档化"浮点不进 kv，持久化用千分比"铁律。

## Unreleased (2026-09-03) — C 后端语义统一 + native/exe 分发目标

### C 运行时值带类型标签（三后端算术逐位一致）
- `wval` 从裸 uint64 改为 `{v, t}` 结构（WT_I64/WT_F64/WT_PTR），构造子
  W_I64/W_F64/W_PTR + 访问器；`+ - * / % < >` 发射为 tag 分派的
  `w_op_*()`（任一侧 f64 即按 VM binary_num_op 提升），此前
  `1.5 2.5 +` 在原生侧静默输出位模式垃圾（I64_MIN 级）而 VM 输出 4。
- whisperc/runtime.c、Rust transpiler、自举发射器（cgen.ws +
  compiler.all.ws 全部 opcode 片段）三处同步转换；字符串排序改为
  字节序（与 VM 一致，此前按指针位排序——陈旧错误）。
- 已知残留（文档化）：字符串打印无引号、`=`/dedup 值位等同而非深度
  相等、f64 打印 %g 与 Rust Display 在极端量级上有格式差。

### 种子重生为纯生成 C
- `whisperc/seed/compiler_seed.c` 此前是 1071 行手工移植的 C 前端；
  现为 `whisper build -t c2 compiler.all.ws` 自编译产物的冻结快照
  （188KB，100% 生成代码），自举固定点 gen2==gen3 保持。
  scripts/bootstrap.sh 补 MINGW 的 -lws2_32。

### 新分发目标
- **`-t native`（安装期硬件特化）**：c2 链 + 宿主 `cc -O3 -march=native
  -flto`，内容寻址缓存（键=展开源+编译器+runtime+cc+march 指纹）；
  同机重建 16ms 缓存命中，源/编译器/CPU 任一变化自动重编。
  `WHISPER_NATIVE_MARCH` 可覆盖（如 x86-64 出通用包）。
- **`-t exe`（自包含单文件）**：SBCL/Starkit 式——当前二进制副本 +
  尾部 wbin 负载 + 8 字节 magic trailer；启动时自检尾部即运行内嵌
  字节码。零手写 ELF/PE/Mach-O，Windows/Linux 实测通过
  （macOS 需重签名）。import 在构建期烘入，无需工具链。

### 测试
- 新增 `-t exe`（运行对拍 VM + trailer 布局）与 `-t native`
  （缓存命中 + 浮点对拍）集成测试；全套 whisper-cli 98 过；
  workspace 20 套全绿；bootstrap.sh 五阶段 + verify-fixedpoint.sh 绿。

## Unreleased (2026-08-21) — 修复与收敛

### 修复（P0 级）
- `whisper run` 在 HEAD 上完全不可用（`over` 关键字化与核心词表冲突）。
  `TokenKind::keyword_name` 成为关键字↔def 名往返的单一事实源；`rot`/`return`/
  `split-cond`/`list?`/`i64?`/`str?`/`bool?` 一并关键字化。
- whisperc 管线恢复：6 月版 parser.ws 的 elif 链缺 `]`、main.ws 被掏空丢失
  `compile`、PushList/字节映射缺失、op-quote 双发射——全部修复，31 个自举测试
  由 11/31 → 31/31。
- 优化器删除指令后不再产生错位跳转：每个 pass 携带新旧索引映射，
  relocate 统一修正 Cond/Jump/Loop 偏移（此前任何带循环/分支的程序经
  build 产出错位字节码）。
- VM：`@map`/`@fold` 等高阶操作不再每元素深拷贝整栈（O(元素×栈深) → O(1)）；
  引号嵌套 512 层上限防宿主栈溢出；StrSlice 拒绝非 UTF-8 边界（原为 panic）；
  负数索引/长度显式报错；`[1 2] [1 2] =` 现在为 `#t`（结构化相等）。
- 能力白名单：词法规范化路径后 `..` 穿越与前缀兄弟目录被拒绝（首个单测覆盖）。

### 移除
- ELF 直编后端（native_gen.rs ~3200 行，产物无法运行）及其前身 src/native/。
- c-whisper / c-native 衍生目标、不可编译的 compiler.c、漂移的快照文件。
  净删约 5700 行。`whisper build` 目标：`wbin` / `c` / `c2`。

### 新增
- **c2 目标**：`whisper build file.ws -t c2 -o prog` —— import 递归内联 →
  自举编译器（compiler.all.ws）在 Rust VM 上运行生成 C → gcc 一键产出原生
  可执行文件。stdlib/math（浮点/位运算）全量走通。
- Rust parser 采纳种子条件语法：平铺切分 + 子区间递归，同时接受严格嵌套与
  共享闭括号 elif 链（compiler.all.ws 首次可在 Rust parser 下完整解析）。
- 浮点字面量进入自举链（classify→C 双字面量插值）；`<<`/`>>` 符号等价 shl/shr。
- 自举链结构守卫：以 `{`/`[` 开头的程序不再令 classify 崩溃。
- C 转译器（-t c）：quote 一等化为 C 函数指针，高阶操作接线 runtime.c，
  TODO 项改为显式 `#error`；12 个端到端 gcc 测试。
- CI：whisper-cli 集成测试不再被排除；新增 selfhost（种子编译 + 22 用例双模式
  + 固定点字节级复验）与 c-backend 两个 job。
- 固定点在上述改动后复验成立（gen2 == gen3）。

### 已知限制（诚实口径）
- 自举链（c2）尚不支持高阶函数（cgen 缺 quote 发射，输出 `// unknown`）；
  Rust C 转译器（-t c）已支持。
- C 运行时值无类型标签：浮点以位模式流转，`.` 打印为整数；混合类型算术
  经显式 f-opcode（i64tof64/fsqrt/f64toi64）工作。
- 两后端 `.` 的字符串格式不同（VM 带引号，C 链裸文本——自举链 driver 依赖之）。
- 类型检查器仍为警告通道（放水项未修，见计划「明确不做」）。

# Changelog

## v1.0.0 (2026-06-06) — First Stable Release

### Language Syntax (Frozen)

| Category | Operators |
|----------|-----------|
| Stack | `_` dup, `` ` `` swap, `drop`, `@` rot, `$n` pick |
| Arithmetic | `+` `-` `*` `/` `%` (mod) |
| Comparison | `=` `<` `>` `!=` `<=` `>=` |
| Logic | `&` and, `\|` or, `!` not |
| Control | `??...\|...]` if/else, `#` loop, `?->` single-branch |
| Lists | `[ ... ]` literal, `@nth` `append` `len` `@map` `@each` `@fold` |
| Words | `: name { body } ;` define, `import` `export` |
| Capability | `@n` call, `!` exec |
| Confidence | `:0.xx` label, `?\|` probabilistic choice |
| IO | `.` output top, `..` output all, `,` read input |

### Core Features

- **VM**: 56+ opcodes, stack-based, call/return frames, recursion
- **Parser**: Recursive descent lexer + Pratt parser for full .ws syntax
- **Type Checker**: Stack-effect validation at compile time
- **Compiler**: Two-pass bytecode generation with constant folding & peephole optimization
- **.wbin Format**: LEB128-encoded binary with magic header
- **WASM Target**: Direct compilation to browser-executable .wasm (90 bytes for hello world)
- **Capability Sandbox**: File I/O, HTTP GET/POST with host/path whitelists

### Self-Hosting

- `whisperc/main.ws`: Whisper compiler written in Whisper (10 lines)
- `whisper bootstrap`: Rust Lexer → Whisper Compiler → VM Execute pipeline
- hello.ws self-compilation verified

### CLI Commands

```
whisper run       file.ws    Execute
whisper build     file.ws    Compile to .wbin/.wasm
whisper check     file.ws    Type check
whisper serve     file.ws    HTTP server
whisper repl                 Interactive REPL
whisper fmt       file.ws    Format
whisper install   pkg        Package manager
whisper bootstrap file.ws    Self-hosting pipeline
```

### Standard Library

| Module | Functions |
|--------|-----------|
| `std/math` | sq, cube, abs, factorial, fib, even, odd |
| `std/str` | strlen, strcat, strdup |
| `std/list` | length, push, map, each, fold, sum, product, reverse |
| `std/io` | read-file, write-file, println |
| `std/json` | json-parse, json-stringify |
| `std/test` | assert-true, assert-false, assert-eq |

### Tooling

- VS Code extension: syntax highlighting, bracket matching, code folding
- Web Playground: interactive editor with built-in examples
- CI/CD: GitHub Actions (test + lint + release)
- Package registry: Git-based with capability review

### Testing

- 63 unit/integration tests
- 2500+ fuzz tests (random program generation)
- 6 working examples: hello, factorial, fib, sum, map, http_get

### Performance

- Constant folding: `3 4 +` → `7` at compile time
- Peephole optimization: `dup drop` → (nothing), `0 add` → (nothing)
- Strength reduction: negative add → subtract
- Direct WASM compilation: 90 bytes vs 2800+ with embedded interpreter

## v0.2.0

- Breaking: `%` changed to Modulo, `drop` keyword added for stack discard
- Self-hosting compiler (whisperc), bootstrap pipeline
- Package manager (whisper install), VS Code extension
- WASM end-to-end verification, conditional branching fixes
- List ordering fix, recursive functions, standard library expansion

## v0.1.0

- Initial release: core VM, parser, compiler, .wbin format, basic CLI
