# Changelog

## Unreleased (2026-10-21) — WASM/WASI M1：零 Rust 运行链路打通

### 新增/修改

- `whisperc/runtime.c`：增加 `W_WASI` 条件编译——
  - 64MB `malloc` 堆（替代 16GB `mmap`）；
  - 文件/env/时间/core 保留；`@2`/`@3`/`@5`/`@10` 在 WASI 下显式
    `unsupported`；FFI/裸 socket/`serve-concurrent` 不编译；
  - `try` 在 WASI 下显式 unsupported（wasi-libc 无 setjmp/longjmp）；
  - 修复非 Windows FFI 分支的 `dlopen(p, ...)` 未定义变量。
- `whisperc/wd.ws`：新增 `wd c <src> <out.c>`——只发射 C，不调 cc、
  不碰缓存；与 `whisper bootstrap` 的 hello C 输出逐字节一致。
- `scripts/build-wasm.sh`：`wd c`（回退 bootstrap）→
  `lift-nested.py` → Zig/wasi-sdk clang → `.wasm`；可用 `--run`。
- `scripts/lift-nested.py`：把 cgen 的 GNU 嵌套函数 `_qb` 提升为文件级
  `_qN`，使 clang/Zig 可编译 HOF 代码；这是 W2.5 的临时桥。
- `tests/security/wasm_file.ws` 等：WASI preopen/负向安全用例。
- `scripts/wd-c-gate.sh`：C-1..C-5，验证 `wd c` 不产 exe、与 bootstrap
  逐字节一致、不调用 cc、`wd build` 不回归、usage 列出 c；已进 CI。
- `scripts/wasm-security-gate.sh` + `tests/security/wasm_*.ws`：
  S-1..S-17（无 preopen 拒绝、preopen 读写/目录、env allowlist、
  `@2`/`@5`/`@6`/`@10`/`try`/`listen`/`serve-concurrent` 显式失败、
  路径逃逸拒绝、FFI 构建期失败、symlink preopen 拒绝、无 Rust
  运行时），本机 17/17。
- `scripts/wrun.sh` + `scripts/wrun-gate.sh`：宿主策略启动器；
  默认无 preopen/env，`--dir GUEST::HOST` / `--env NAME` 显式放行，
  `--http-allow`/`--exec`/`--ffi` 显式拒绝；W-1..W-7 7/7。
- `.github/workflows/wasm.yml`：Zig 0.14.1 + WAMR 2.4.5 pin，
  跑 hello、安全门禁、wrun 门禁（首次远端运行待确认）。

### 实测

- `hello.ws → wd c → zig cc → .wasm → iwasm` 输出 `Hello, World!`。
- core conformance：**26/27 通过，1 个原语义 skip**；HOF 全部通过。
- 无 preopen：`write-file` 返回 `-1`、`read-file` 为空；
  `--map-dir=/data::target/wasi-data`：`10`/`10`。
- journal 10k（538KB）WASM vs native：list 0.76s/0.24s，
  search 0.70s/0.19s；输出 SHA256 一致。
- `cargo test --workspace`：**467 passed / 0 failed**。
- 运行链路无 Rust：Zig（C 工具链）+ WAMR（C）；`crates/**` 未改。

## Unreleased (2026-10-20) — 安全模型审计 + WASM/WASI 整改计划

### 审计结论（文档，无行为变更）

- 实测确认当前 VM **不是**完整沙箱：`@0` 未绑定会拒绝，但
  `read-file`/`write-file`/`getenv` 等 ambient opcode 直接调用宿主；
  `ffi-open` 默认可用（可加载任意 native 库）。
- 实测确认 C 链 `@0`/`@5` 无任何授权检查；`-t c2` 产物可直接读文件、
  执行命令。native 产物定位改为 **trusted-only**。
- 修正 `docs/spec.md` 的“C 链能力 = no-op”旧表述为“直通、无检查”。

### 新增文档

- `docs/security-model.md`：威胁模型、信任边界、effect 矩阵、F-1..F-8
  发现、I-1..I-10 不变量、profile 与审计清单。
- `docs/wasm-wasi-plan.md`：WASM/WASI 目标架构、W0..W8 工作包、
  M0..M6 里程碑（4–6 周）、功能矩阵、风险与 DoD。
- `tests/wasm_preregistered.md`：W-1..W-9 判据与反向判据。
- `scripts/wasm-preflight.sh`：clang/wasm32 target、wasi-sysroot、WASM
  runtime 的前置检查；默认只报告，`--strict` 供 CI 用。
- 零 Rust 约束：运行时只接受 C 实现（WAMR `iwasm` / wasm3 / wasm2c），
  **不采用 wasmtime/wasmer**；编译器可用 wasi-sdk clang 或 Zig `cc`
  （Zig 自带 wasi-libc）。

### 文档同步

- README：新增“安全与执行模式”分档表；构建目标增加 wasm（规划）；
  能力沙箱状态改为“VM 部分 / native 无”。
- `docs/spec.md`：§4 差异表、§13 FFI 授权现状、§15 C 契约
  trusted-only、新增 §18 安全模型与 WASM 规范性要求。
- `docs/stdlib.md`：修正 `read-file`/`write-file` 被误标为 `@0/@1`、
  `getenv` 被误标为受 `--allow-env` 约束的说明。
- `docs/tutorial.md`：文件 IO 安全提示；修正 `http-get` 的 HTTPS 说明。
- `docs/language-backlog.md`：登记 SEC-1（能力执行缺口）与 SEC-2
  （WASM/WASI 目标）。
- `docs/independence-design.md`：新增 Step 7（WASM/WASI），Step 5
  明确“VM 不是安全方案”。

## Unreleased (2026-10-20) — 收口 ③：`wd` 缓存落地 + S-4 关闭

### `wd` 内容寻址缓存（收口 ③）

- 键 = hash(生成的 C ++ `runtime.c` 内容 ++ cc 名)，缓存落在
  `<WHISPER_ROOT>/target/whisper-cache/<key>`；命中拷贝产物直接跑，
  未命中才调 cc，成功后回填；cc 失败不写缓存。
- 新增 `wd-hash`/`wd-q`/`wd-out-target`/`wd-cache-dir`/`wd-cache-path`/
  `wd-cp`/`wd-hit?`/`wd-store`/`wd-cc-run`；`wd-build` 的 gcc 段抽成
  `wd-cc-run`，命中/未命中共用运行段。
- 门禁 `scripts/wd-cache-gate.sh`：C-1..C-6 **8 pass / 0 fail**，
  实测首次 801ms、命中 148ms。
- 修掉两处真 bug：`wd-store` 的 `wd-cp` 源/目标操作数反了；编译成功消息
  自带 `\n` 又叠加 `.` 的换行，导致 stdout 多一空行。

### S-4：`std/str` 与 `std/list` 同名冲突关闭

- `std/str`：`first`/`last` → `first-char`/`last-char`，`empty?` →
  `str-empty?`，`contains?` → `str-contains?`；`std/list` 保留原名。
  `crates/whisper-stdlib/std/str.ws` 快照同步。
- 顺带修 `std/list` 的 `contains?`/`all?`/`any?`（fold/each 体里多一个
  `swap`，运行时直接报 `expected [T]`）；`contains?` 改成显式首元素递归。
- 新增 `tests/conformance/s4_both_strlist.ws`：同时 import 两个模块，
  VM 与 c2 输出一致，已进 syntax-gate。

### Step 4：Whisper 写的 `fmt` v1

- 新增 `whisperc/fmt.ws`（纯 Whisper）与 `scripts/build-wfmt.sh`（`wd` 零 Rust
  构建）。CLI：`wfmt <src>` / `wfmt <src> <out>`。
- v1 是布局规范化：CRLF→LF、字符串外的行首/行尾空白；字符串内容、注释正文、
  token 间距不动。逐行状态机处理多行字符串、转义引号、注释里的引号。
- 门禁 `scripts/fmt-gate.sh`：F-1..F-6 **5 pass / 0 fail**（conformance 27 个
  文件幂等且 VM 输出一致；5 个代表文件 c2 输出一致）。
- 过程中修掉四个真 bug：`fmt-set` 的 `strcat` 操作数顺序、`fmt-chomp` 的
  `strnth` 后重复 `dup`、`fmt-main` 的 `write-file` 参数顺序、门禁临时文件
  缺少 `.ws` 扩展导致 c2 拒绝。

### 门禁

- `cargo test --workspace`：**467 passed / 0 failed**
- syntax-gate **27/0**、stdlib-gate **67/0**、wd-cache-gate **8/8**、
  fmt-gate **5/5**
- 待办：`scripts/build-wd.sh`（零 Rust 链）在本机单次 30 分钟+ / 6GB+，
  本轮用 `-t c2` 从同一份 `compiler.all.ws` + `wd.ws` 编出 `target/wd/wd`
  做行为验收；收口前应补跑一次零 Rust 构建。

## Unreleased (2026-09-21p) — 收口战役第二轮：删掉第二棵编译器（D-1）

第一轮把"漂移"关掉了一半——顶层 `lexer.ws`/`classify.ws`/`parser.ws`/
`main.ws` 与 `src/compile.ws` 被判定为"活的"（`whisper bootstrap` 用），于是
只是**写进文档**。写进文档不是收口，是把问题换个地方放。

**实测（这次把探针写对了）**：第一版探针里程序都没写 `.`，而 Whisper 只有
`.` 才输出，于是"VM output 为空"被误读成"完全不能用"。修正后才发现：软自举
是一棵**部分可用**的第二编译器（基础算术/单行定义/引号/跨行字符串可以；
`@map` 融合、import 展开、`print`/`exit`、CRLF 归一化都不行），硬自举基本
不可用。它与 `compiler.all.ws` 之间的漂移**没有任何机制把关**——
`test_pipeline_compare` 只比对一个表达式的结果，对手还是 Rust 前端。

**收口方式**：不是给两棵树加注释，而是让 `whisper bootstrap` 直接委托权威
流水线（把生成的 C 打到 stdout），然后删掉那棵树。**只有一份编译器源码，
漂移面归零**，而命令本身从"部分可用"变成"和 `-t c2` 逐字节一致"。

- 八个程序逐字节一致（含 CRLF、import、`@map`、`print`、引号），生成物经
  gcc 编译后运行正确；`--hard` 保留为历史别名，行为不变。
- `register_host_helpers` **保留**——它是 `c2.rs` 活路径的依赖，清理时差点
  被一起删掉，已加单测钉住。
- CI 的 `Hard self-hosting smoke` 改成**比对生成的 C**：只跑一遍的话，被删掉
  的那棵树也过（退出码同样是 0）。

**过程中差点写错的两处**：
1. 坏探针差点让我删掉一个半可用的功能（见上）。
2. 第一版单元测试是**同义反复**——把 `compile_to_c(x)` 与
   `compile_via_whisperc_vm(expand_imports(x))` 比，那是拿函数和它自己的内部
   调用比。真实的 `-t c2` 还多一步自动前置 `import "std/core"`，端到端一比
   就露馅（少 1300 余字节）。修法不是在自己这边再写一遍那六行，而是抽成
   `c2::expand_with_core` **两边共用**——自己再写一遍正是第二棵编译器的起点。

**测试总数 490 → 465**（净减 25）：删掉的 31 条覆盖的是已不存在的代码，
新增 4 条单元 + 2 条集成。门禁全绿不回归：cargo 465/0 · syntax 26 ·
stdlib 58 · typecheck FP=0 · net · app 13 路由 · 覆盖 90/0/0。

## Unreleased (2026-09-21o) — 收口战役：把登记在案的缺口逐条清掉

上一轮结束时还挂着一串"未做/残留"。本轮逐条核对，**先查再改**——侦察阶段就
推翻了三条既有描述：

| 登记的说法 | 实况 |
|---|---|
| `path_is_allowed` 下相对路径**永远被拒** | **代码早修了**（`capability.rs:129` 把相对路径 join CWD 再归一化），是文档没跟上 |
| `whisperc/compiler.ws` 等与权威文件漂移 | 属实，但其中一部分**是活的**（`whisper bootstrap` 用 `include_str!` 拼的模块化编译器），另一部分零引用 |
| `-t c` 对 `CapExec` 发射空操作 | 属实，且 `CapCall` 也一样——**两个都静默发射一行注释** |

### W1 —— 源码行尾归一化（CRLF）

同一个 CRLF 源文件里的跨行字符串 `"a\r\nb" strlen .`，三条路径给三个结果：
`whisper run` 给 **4**、`-t c2` 给 3、`wd run` 给 3。不是外观问题——那个 `\r`
直接变成了程序数据。而且 `include_str!("compiler.all.ws")` 在
`core.autocrlf` 工作树上带 CRLF，于是**同一份源码在 LF 检出与 CRLF 检出上
产出不同的 C**（3 个杂散 `\r`），这正是 `wd` 与 `-t c2` 的 3 字节残差的来源。

修法：Rust 侧在 `Lexer::new` 里折叠 CRLF（只折叠这一对，单独的 `\r` 是数据，
保留）；Whisper 侧在编译入口 `compile-full` 加一条 `normalize-eol`。两边对称。

### W2 —— `wd` 的多行定义限定名克隆（A3 收口）

`wd` 只克隆"以 `;` 收尾"的单行定义，于是 `stdlib/math.ws` 的 `clamp` 这类
多行定义没有 `math.clamp` 克隆，生成的 C 少了整个 `W_math_clamp`。
重写克隆循环（状态 `[lines, idx, out, copying]`）。**实测四个程序——自包含 /
`std/math` / `std/list` / `std/str`——`wd` 与 `-t c2` 的 C 输出逐字节相同**
（1601/1601、17875/17875、17228/17228、22569/22569）。里程碑 A 留下的 A3
残差就此清零。

过程中踩到并记录了两个真实教训：
- **`over` 会读到调用者的栈**：闭合判定一度排在 `line` 已被消耗之后，
  `over` 拿到的是 state 列表，`strfind` 拿列表当字符串 → `expected ptr`。
- **KV 只存字符串**：cgen 对键和值都做 `w_as_ptr`，把布尔存进去就是
  `expected ptr`。标志位一律走 `"1"`/`"0"`。

### W3 —— `bootstrap.sh` 在缺 `cmp` 的机器上假报固定点破裂

`if ! cmp -s a b` 在 `cmp` 不存在时得到 127，取反为**真** → 打印
`FIXED POINT BROKEN` 并退出。本机（msys2）实测 `cmp` 与 `diff` 都不存在，
**这条脚本因此从来没跑到过第 5 步**。比较逻辑抽到 `scripts/lib/same-bytes.sh`，
`bootstrap.sh` 与 `verify-fixedpoint.sh` 共用。

### W4 —— `-t c` 的能力 opcode 不再静默空操作

`CapExec` → `W_call();`，`CapCall(n)` → `W__<n>();`（runtime.c 两者本就都有，
自举链的 cgen 一直这么发）。实测 `-t c` 产物：`{ 42 } call .` → 42、
`10 { 2 * } call .` → 20、`"PATH" @4 .` 与 VM 同值。

### W5 —— 历史快照与文档漂移

机械清点全仓库的两种引用方式（`include_str!` 与模块间 `import`）后，删掉四个
**零引用**的历史快照：`whisperc/compiler.ws`（早期手工合并版）、
`src/cgen.ws`、`src/classify.ws`、`src/lexer.ws`。新增 `whisperc/README.md`
写清"谁是权威、谁是软自举链的模块"——顶层 `lexer/classify/parser/main.ws`
与 `src/compile.ws` **是活的**（`whisper bootstrap` 用），不是副本，但两者会
漂移，这一点现在明写在文档里而不是留给下一个人猜。文档里 `path_is_allowed`
的过期描述一并改正。

### W6 —— `@10` = `exec-raw`：逐字节转发，且不改 `@5` 的语义

`@5` 的 `trim` 是命令替换惯用法依赖的行为，取消它会把"修一个洞"变成"改一批
调用方"。改为新增**兄弟能力** `@10`：同一份实现、同一份授权（CLI 绑 `@5` 时
一起绑 `@10`），只是两个流原样返回。未授权时 `@10` 一样被拒。

**L-1 唯一未达标的 E4-c 就此收口**：`wd run` 的转发段与直接运行产物逐字节
相同。顺带把 `@10` 照出来的两处 C 链缺陷也修了——`( cmd )` 的外层空格被
`cmd.exe` 的 echo 吞进输出（`78 20 0A` vs VM 的 `78 0D 0A`），以及 `_popen`
读端是文本模式会把子进程 CRLF 折叠成 LF。

### 收口过程中新发现：S-1 非 ASCII 字符串两链分叉（**未修，已立项**）

做端到端实证时，演示程序里的一句中文让 `wd` 与 `-t c2` 的 C 输出差了 4 字节。
二分后确认与 import、与定义都无关，只与非 ASCII 字符串字面量有关：

| 表达式（源串 `"你好ab"`） | Rust VM | C 链 |
|---|---|---|
| `strlen .` | 8 | 4 |
| `1 strnth .` | 189 | 229 |
| `strchars .` | `[20320 22909 97 98]` | `[228 229 97 98]` |
| `.` | 正确 | **乱码** |

**后果**：`"你好" .` 经 `-t c2` 编出的 C 里字面量只有 2 字节（`E4 E5`），
不是 6 字节——**Rust CLI 这条路径会静默写坏含非 ASCII 字面量的程序**；
`wd`（原生 cgen）编同一个程序是正确的。

**未修的理由**：这不是补一个洞，而是要先定语义再统一两条运行时的
`strlen`/`strnth`/`strslice`/`strchars`/`charsstr`/`striter`/`escape-c-str`。
仓促改会把"两边都错得不一样"变成"两边都错得一样"，更难发现。
已立项为 **S-1**（`docs/language-backlog.md`），证据与一键复现见
`tests/closeout_preregistered.md`。

### 本轮明确不做（附理由，不是漏掉）

- **TLS / `@6`-`@9` 的 C 侧**：与 TLS 强耦合；在"C 只当可移植汇编、零第三方
  依赖"的前提下要独立设计。
- **C 链 host 白名单**：独立二进制没有命令行，白名单只能靠环境变量——那是
  一种看起来安全的假象。保持"要么整套做、要么不做"。
- **Step 4（工具链迁移）**：独立战役。

## Unreleased (2026-09-21n) — 战役 L-1：`print` 与 `exit` 两个原语

语言缺了两个原语，不是某个程序缺功能：C 链程序恒以 `return 0;` 结束（任何
Whisper 写的 CLI 都报不了失败），唯一的打印词 `.` 自带换行且对字符串的引号
处理两链不同（它不是字节通道）。本节补上，判据见 `tests/l1_preregistered.md`。

    "hi" print          # hi        —— 逐字节，无换行
    3 exit              # shell 看到退出码 3
    : f { 7 exit } ; f 1 .   # 退出码 7，且不打印 1（真终止）

- **opcode `0x93` = `Print`、`0x94` = `Exit`**，紧接 IO 区（`0x90` `.`、
  `0x91` `..`、`0x92` 读输入）。走正规 opcode，没有借 `@n` 能力 ABI——
  "退出进程"和"打印字符串"不是需要授权的事，塞进能力 ABI 会让每个用到的
  程序都要 `--allow-…`。
- 触点比预注册多三个（预注册写 11 个 Rust 文件，实际 14）：漏掉的是
  `lexer.rs`（关键字表）、`token.rs`（`TokenKind` + `keyword_name()`）、
  `lsp/lib.rs`（词条表）——按 opcode 视角想清单就会漏掉"词形"这一层。
- **顺带修掉一条长期被测试抹平的真实双链分歧**：C 链在 Windows 上每写一个
  `\n` 就变成 CRLF，VM 从不翻译。`print` 要求逐字节，绕不过去。修法是在
  `w_init` 里一次性把 stdout 设为二进制模式。**行为变更**：C 链程序（含 `.`）
  的换行从 CRLF 变为 LF。第一版修法（在 `print` 里惰性切换）是错的：文本
  翻译发生在 flush 时，而重定向到文件时 stdout 全缓冲，缓冲区里更早的 `\n`
  也会被按二进制写出——输出会取决于后面有没有调用 `print`。

**实测**：E1（stdout 逐字节）4/4、E2（真退出码）3/3，VM 与 `-t c2` 两链全中；
`cargo test --workspace` 482/0；syntax 26 / stdlib 58 / typecheck FP=0 / net /
app 13 路由全绿；`opcode_coverage.sh` 80 → **87** 例全过；固定点 gen2 == gen3。

**E4 只达成一半（如实登记）**：`wd` 的错误路径现在有真退出码（缺源文件 → 1，
用法错 → 2），转发也改用了 `print`（不再自己补换行）。但 `wd run` 的转发段
仍与直接运行产物不逐字节相同——差掉的字节是**子进程尾部空白**，它在 `wd`
看到输出之前就被能力 `@5` 按设计 `trim()` 掉了（两链都写明"mirror Rust 的
`String::trim()`"）。这是能力契约问题、不在 L-1 范围，登记为 W-5。

**验证用的是零 Rust 链的产物**：`bash scripts/build-wd.sh` 本轮端到端重跑通过
（种子 → whisper0 → whisper1 → wd，全程只有 gcc），E4 与四条 `wd run` 冒烟
都在 `target/wd/wd` 上复验；`-t c`/`c2`/`exe`/`native`/`bundle`/`wbin` 六个
构建目标也各自验过认得新 opcode。

## Unreleased (2026-09-21m) — `bootstrap.sh` 一条命令走到 `wd`

`scripts/bootstrap.sh` 从 5 步扩到 7 步：固定点自检之后，用刚造出来的
`whisper2` 编出 `whisperc/wd.ws`（纯 Whisper 写的驱动），再冒烟验证
`wd run 1 2 + .` → `3`。于是「干净机器 → 种子 → 能编能跑任何程序」是一条
命令，全程只用 gcc。

产物路径：`$OUT/wd`（默认 `target/bootstrap/wd`）。

    bash scripts/bootstrap.sh
    ./target/bootstrap/wd run hello.ws hello

**验证范围（如实说明）**：新增的两步与 `scripts/build-wd.sh` 的第 3 步同构，
而 `build-wd.sh` 已端到端验证通过（EXIT=0，并用其产物跑通了四条 import 用例）；
本次只对 `bootstrap.sh` 做了 `bash -n` 语法校验，**未在本轮重跑整条链**
（约 25 分钟）。

### 本轮未落地的一项（回退了）

尝试收掉 A3 的 3 字节残差（`strsplit` 尾部空段）并补多行定义的限定名克隆，
两处改动都引入了新回归且未在预算内定位，已 `git checkout` 回退到
`7fd1a74` 的验证状态——**不留半成品**。残差与多行克隆仍是已知缺口，
记录在 `tests/step3_preregistered.md`。

## Unreleased (2026-09-21l) — Step 3 里程碑 B：`wd` 的 import 展开（真凶是两处栈语义误用）

上一节留下的缺口是「`wd` 只能编自包含程序」：缺 import 展开，它的 `.c` 与
`-t c2` 差 1329 字节。本节把它补上——判据见 `tests/step3_preregistered.md`。

### 新增 —— `wd` 的 import 展开（与 `c2.rs::expand_imports` 同构）

源码未提及 `std/core` 时**先补一行 `import "std/core"`**，再统一逐行展开；
于是 std/core 的正文与限定名克隆都由同一条通路产生，**没有特例分支**。
递归在 `wd-expand-piece` 内完成，已访问集合放 KV 防环。

实测（`wd run`）：

    l1 2 + .                                   → 3
    import "std/math"  + 5 sq .                → 25
    import "std/list"  + [1 2 3] len .         → 3
    import "std/math"  + 5 math.sq .（限定名） → 25

### 定位到并修掉的两个真 bug（这才是上一轮卡住的原因）

1. **`over N @nth` 不是「取第 N 项」**，而是「复制 `$1`，再对这个副本取
   下标」。`wd-def-name` 之后 `over` 已经拿到了行文本，多写的 `1 @nth` 是
   **在对字符串取下标** → 垃圾指针 → `type error: expected ptr`。四处同样
   写法，全改。上一轮没定位到，是因为把它当成了一个原子取值操作。
2. **`>=` 的参数顺序**：`a b >=` 算 `a >= b`（`a` 在栈下方）。循环终止条件
   写成 `len ... idx >=`，实际算的是 `n >= idx`——**idx=0 时立即判定完成**，
   展开结果恒为空串（`.c` 只剩 185 字节）。这个 bug 一开始被第一个 bug 掩盖
   （先报类型错），修掉 `over` 之后才浮出来。

另外把 `@fold` 换成该文件其余循环同款的「索引状态机」：栈纪律只剩一条——
本地状态永远一件，读它一律 `dup N @nth`。

### 顺带纠正一处测试用例错误

我一度用 `5 std.math.sq .` 当限定名用例，但它**在 VM 上也是
`Undefined word: std.math.sq`**——限定名是 `math.sq`（`std/` 不进命名空间）。
`wd` 与 `c2` 在这个名字上的行为一致 ✓，是用例错了。

### A3（与 `-t c2` 逐字节一致）现状

自包含程序从差 1329 字节缩到 **1601 vs 1604（差 3 字节）**；带 import 的
17451 vs 17878。残差主要来自**多行定义不做限定名克隆**（c2 会做）。行级比较
查不出差异行，剩 3 字节在尾部空白，未定位。

### 验证

    cargo test --workspace        480 / 0
    cargo clippy -D warnings      0
    cargo fmt --all --check       0
    syntax / stdlib               26 / 0 · 58 / 0
    typecheck-gate                FP=0 PASS
    opcode_coverage               80 / 0 / 0
    net-gate / app-gate           PASS / PASS (13 routes)

固定点未重跑：本轮仍未改 `compiler.all.ws` / `runtime.c`。

## Unreleased (2026-09-21k) — Step 3 里程碑 A：Whisper 写的驱动 `wd`（零 Rust）

判据见 `tests/step3_preregistered.md`。设计文档把 Step 3 定为"摆脱 Rust 的
分水岭"，其前置（C 链能用 `@5` 起进程）已在 Step 2a 解锁——本轮复核成立。

### 新增 —— `whisperc/wd.ws` + `scripts/build-wd.sh`

`wd` 是纯 Whisper 程序，与 `compiler.all.ws` 一起被自举链编译；`build-wd.sh`
只用 **gcc + 冻结种子**，脚本里没有 `cargo`：

    wd build <src.ws> <out>     编译到 <out>，留下 <out>.c
    wd run   <src.ws> <out>     同上，再把产物跑起来并转发其输出

`WHISPER_CC` 选 C 编译器（默认 gcc），`WHISPER_ROOT` 指仓库根（默认 `.`，
用于找 `runtime.c` 与将来的 stdlib）。

### 已达成 / 未达成（如实登记）

✅ 零 Rust 构建；✅ 编译+运行自包含程序（`1 2 + .` → `3`）；✅ 错误可定位
（源文件不存在会点名是哪个文件，而不是空文件编译后由 gcc 报错；C 编译器
失败会转出 gcc 的 stderr）。

❌ **`wd` 产的 C 与 `-t c2` 不逐字节一致**（275 vs 1604 字节）。差异全部来自
**缺失的 import 展开**：c2 会前置并内联 `std/core` 及其限定名克隆，`wd` 不做。
这是里程碑 B 的范围。**`wd` 目前只能编自包含程序**——用到 stdlib 的程序
在 c2 上要 `import`，在 `wd` 上还没有。

里程碑 B 的零件写了并逐个实测了大半（路径解析、定义行识别、注释剥离、
尾字符判定、import 行识别、KV 往返），但**限定名克隆的折叠没调通**
（`wd-clone-line` 仍抛 `type error: expected ptr`）。这块代码**没有接进
`wd-build`**，只作为注释留在文件里并标注未完成——半成品接进去会把已经能用
的部分一起带坏。

### 本轮踩到并修掉的坑（已写进代码注释）

1. `over`（= $1）在"本地值只有一件"时越界读调用者的栈 → `expected ptr`。
2. `strcat(a,b)` 是 a 在前，拼命令行时按 b 在前写导致整条命令错位。
3. `striter` 是字符串**迭代器**（推 `[字符码, 余串]`），不是打印词。
4. `file-exists` 极性写反 → 永远返回不存在的产物路径。
5. `@5` 执行不存在的路径只得到 popen 失败，报错离根因很远 → 执行前先探测
   产物路径（原名 → `.exe` → 明确报错）。

### 顺带发现的两个语言侧缺口（另立）

- **C 链程序没有退出码**：cgen 固定发射 `return 0;`，Whisper 层没有
  `exit`/`quit` 词，所以 `wd` 的错误路径无法让 shell 看到非零退出码。
- **没有裸打印词**：打印只有 `.`（自带换行），`striter` 是迭代器。`wd run`
  转发的输出末尾因此会多一个换行，不是逐字节转发。

### 验证

    cargo test --workspace        480 / 0（新增 wd 条件式集成测试；无 target/wd/wd 时跳过）
    cargo clippy -D warnings      0
    cargo fmt --all --check       0
    verify-fixedpoint             gen2 == gen3（本轮未改 compiler.all.ws）

## Unreleased (2026-09-21j) — Step 2b：`@2`/`@3` 上 C 链 + HTTP 家族改为显式失败

判据见 `tests/step2b_preregistered.md`（S1–S5）。原估为"中等工程"，本轮核实
发现该估算的前提不成立——**VM 侧的 `@2/@3` 本身就不支持 HTTPS**
（`capability.rs` 直接拒绝 `https://`，整个实现约 80 行），真正走 rustls 的是
`@6` 家族（`http_session.rs`）。因此"与 VM 对齐"的实现**不含 TLS**，量级降到
补一个 HTTP/1.1 最小客户端。

### 新增 —— C 链的 HTTP 客户端（`@2` http-get / `@3` http-post）

`whisperc/runtime.c` 新增约 190 行，语义与 `capability.rs::http_request` 逐项
对齐：URL 解析规则（端口从右切、path 恒以 `/` 开头）、请求行 / `Host` /
`Connection: close` / POST 的 `Content-Type` 与 `Content-Length`、以及"首个
`\r\n\r\n` 之后即 body"的提取规则。运行时原本只有服务端 listen/accept，
本轮补上了客户端 `connect`（`getaddrinfo` + `connect`）。

`tests/integration_tests.rs` 新增 `http_capabilities_match_across_chains`：
进程内起 `TcpListener` 当 HTTP/1.1 服务器，4 例 GET/POST 两链输出一致，
且**服务器侧记录的请求逐字节相同**。

### 安全 —— HTTPS 两链同样拒绝，且拒绝发生在建连之前

`@2`/`@3` 不支持 HTTPS（VM 也不支持）。新增测试
`https_is_refused_and_never_sent_as_plaintext`：两链都以
`HTTPS is not yet supported…` 失败，**监听套接字全程没收到任何连接**——
不存在"静默降级成明文发到 443"的路径。

同时修掉原有拒绝信息里的误导：它写着 "or enable the 'native-tls' feature"，
而这个 cargo feature **不存在**。新文案指明改用 `http://`，或改用确实支持
TLS 的 `@6`。

### 修复 —— `@6`-`@9` 从 gcc 报错改为显式失败

原来用这些 id 的程序经 c2 构建时，gcc 报
`implicit declaration of function 'W__6'`——能挡住错误二进制，但错误信息离
根因很远。现在它们在 `runtime.c` 里定义为 `w_cap_unimplemented()` →
`w_fail()`：程序**照常构建**，调用到未实现的 id 时打印
`capability @6 (http-req (session)) is not implemented on the C chain; it is
VM-only for now. See docs/independence-design.md Step 2b…` 并退出。

**没有采用编译期 `#error`**：c2 会文本内联 `std/http.ws`，编译期报错会把
"只 import 但只用 `@2`"的程序一起打死。测试同时钉住这一条。

### 修复 —— `@3` 在 VM 上根本用不了（既有 bug）

写 POST 用例时 VM 直接报 `http_post: expected URL and body`。根因：
`HttpPostCap` 没有覆写 `Capability::arity()`，默认 1，而 `call()` 要求 2 个
参数——`CapCall` 只弹 1 个，于是 `@3` **怎么写都失败**。`FileWriteCap`
（`path content @1`）早就覆写成 2，HTTP POST 这处漏了。修法与之一致，顺带
恢复双链契约（C 侧 `W__3` 本来就弹 2 个）。

### 文档 —— 修掉 README 的 HTTP 失真

README 原文把 `@2/@3` 与 `@6` 并列写成"支持 `https://`（rustls + webpki 根）"，
与代码不符。已按令牌分档重写为可用性表格（哪个 id 支持什么协议、两链各自
是否可用），并说明 `@2/@3` 的 HTTPS 缺口是**两链共同的**现状。

### 验证

    cargo test --workspace        479 / 0
    cargo clippy -D warnings      0
    cargo fmt --all --check       0
    syntax / stdlib               26 / 0 · 58 / 0
    typecheck-gate                FP=0 PASS
    opcode_coverage               80 / 0 / 0
    net-gate / app-gate           PASS / PASS (13 routes)
    verify-fixedpoint             gen2 == gen3

**仍未做（如实登记）**：`@6`/`@7`/`@8`/`@9` 的 C 侧实现（会话 cookie 罐、
审计日志、raw 通道、并行会话）；TLS（两链共同缺口）。C 链的宿主白名单**未
引入**——`@0/@1/@5` 本来也没有，不在本轮做半套沙箱。

## Unreleased (2026-09-21i) — 战役 O₂：另外四种容器里的引号（引号至此真正处处可用）

上一节（2026-09-21h）修掉了"顶层"与"列表字面量"两种容器，但同一根因在
**其余四种容器**里原封不动：条件分支、循环体、词定义体、高阶融合体。本节
把它们一次收掉——判据见 `tests/o2_preregistered.md`（P1–P5）。

### 修复 —— 容器节点内部的定义没被抬到顶层

`fuse-hof` 把「引号作值」展开成一对 token：定义 `[9, "__qN", body]` 与引用
`[16, "__qN"]`。**只有落在顶层的定义**会被 `split-defs` 收走、发射成静态函数
`W___qN`；落在任何容器节点内部的定义都会走到 `compile-one` 的兜底分支
（它只认 tag 16 的引用），变成 `#error whisper-cgen: unknown bare opcode`，
引用就此悬空。

实测矩阵（修前 VM 全对、c2 全编不过）：

| 容器 | 程序 | VM | c2（修前） | c2（现状） |
|------|------|----|----|----|
| 条件分支 | `#t ?? { 9 } \| { 8 } ] call .` | 9 | ❌ | ✅ 9 |
| 条件 else | `#f ?? { 9 } \| { 8 } ] call .` | 8 | ❌ | ✅ 8 |
| 循环体 | `2 { { 5 } drop 1 - } { dup 0 > } # .` | 0 | ❌ | ✅ 0 |
| 词定义体 | `: f { { 5 } drop 7 } ; f .` | 7 | ❌ | ✅ 7 |
| 引号套引号 | `{ { 5 } } call call .` | 5 | ❌ | ✅ 5 |
| 高阶词体 | `[1 2] { { 7 } drop 1 + } @map .` | `[2 3]` | ❌ | ✅ `[2 3]` |

**修法（一条通用通路，不为每种容器复制状态机）**：把 O′ 那版只认列表的
`lift-list` 一般化成

- `part` / `part-iter`：一条 token 流 → `[defs, rest]`（先在本层 `fuse-hof`
  展开裸引号，再拣出 `[9,…]`）；
- `hs`：容器节点 → `defs… ++ [node']`，按 tag 区间分派
  `6`(列表) / `7`,`8`(条件、循环) / `9`(词定义) / `249..254`(高阶融合体)；
  非容器原样返回，所以调用处不必先判种类；
- `hs1` / `hs2` / `hs9`：按"哪些字段是 token 流"分三种装配（`@1`；`@1`+`@2`；
  `@2`——词定义的 `name` 是字符串，自然跳过）。

`fuse-hof` 有三处要接：容器下潜分支、**融合分支**（新造的
`[249..254, inner]` 也得过 `hs`）、**非融合分支**（新造的
`[9,"__qN",inner]` 也得过 `hs`，否则 `{ { 5 } }` 里层引号的定义仍留在定义
体内）。后两处是初版遗漏，靠探针抓出来。

### 实施中踩到的三个坑（已写进代码注释）

1. `rot` **只转栈顶三件**——四件一组时最底下那件永远上不来，据此写的重建
   序列全错。改为"读取时把源列表换回栈顶"：每次 `dup … @nth` 后 `swap`，
   取出的值沉到源下面，源始终可再取，最后 `drop` 源即得按读取顺序排好的
   序列。
2. `dup N @nth` 是**嵌套索引**：`dup 1 @nth 1 @nth` 是"取第 1 项的第 1 项"，
   不是"取两次第 1 项"。
3. `concat-bc` 拼的是**列表的项**：拼 `ref ++ hsres` 前必须
   `1 PushList` 把 `[16,"__qN"]` 包成 `[[16,"__qN"]]`，否则它被摊成两个 token
   ——正是 O′ 之前那个症状的变体。

### 验证

    cargo test --workspace        476 / 0
    cargo clippy -D warnings      0
    cargo fmt --all --check       0
    syntax / stdlib               26 / 0 · 58 / 0
    typecheck-gate                FP=0 PASS
    opcode_coverage               80 / 0 / 0
    net-gate / app-gate           PASS / PASS (13 routes)
    verify-fixedpoint             gen2 == gen3

两链对拍探针 23/23（含上表全部形态 + 各容器的"无引号"对照）。

## Unreleased (2026-09-21h) — 战役 O / O′ 收口：引号成为真正的一等值

目标："让自举链支持引号作为一等值"，使 `call` 的动态分发两链一致。
**已达成**——判据见 `tests/o_preregistered.md`（J1–J5）。上一节（2026-09-21g）
登记的三处修复只覆盖了"≥2 个引号字面量"；本节补上"引号出现在列表字面量里"
以及一条被中途掩盖的双链语义分歧，动态分发由此两链同值。

### 修复 —— `try-def-colon` / `try-loop-inner` 的裸 `streq`（VM/C 链真实分歧）

`str-eq?` 本就是为「参数可能是列表」写的带守卫版本
（`over str? ?? streq | drop drop #f ]`），`gp-loop` 一直在用；但这两处直接对
「引号体之后的那个 token」调裸 `streq`——而它可能是另一个引号节点：

- **Rust VM**：`streq` 拿到列表 → `Type mismatch: expected str, got [T]`
- **C 链**：`w_str_eq` 拿列表指针去比 → **静默返回 false**，于是侥幸通过

两个引号时看不出来（`try-loop` 的长度守卫 `len 1 <` 先返回），**三个引号
才走到那一行**。改用 `str-eq?`，两链行为一致。

> 上一节曾据此写下"门禁的 c2 腿与用户路径不等价（门禁盲点）"——那句话
> **不成立**，本节撤回（`tests/o_preregistered.md` 中保留了撤回记录）：
> NSC 那次并没有真正成功，它吐出了 C 但**链接失败**（手工脚本漏了
> `-lws2_32`）。两条路径其实都失败。门禁的 c2 腿与用户路径在"是否内联
> `std/core`"上确有差异（属实），但那不是本例的原因。

### 修复 —— 列表字面量里的引号（战役 O′）

`fuse-hof` 把"引号作值"展开成一对 token：定义 `[9, "__qN", body]` 与引用
`[16, "__qN"]`。只有**顶层**的定义会被 `split-defs` 收走、发射成真正的静态
函数 `W___qN`；落在 `[6, …]` 列表里时 `compile-one` 只认 tag 16 的引用
（它没有 tag 9 分支），定义被静默丢弃，引用便指向从未发射的函数
—— gcc 报 `'W___q0' undeclared`。

新增 `lift-list` / `lift-list-iter`（`whisperc/compiler.all.ws`）：把元素流里
tag 9 的定义抬到 `[6, rest]` 节点**之前**，列表内只留引用；`fuse-hof` 尾部
增加 `head 是 [6, …]` 的分支，递归处理元素流后 splice 回本层结果。列表可任意
嵌套——每层各自把自己的定义抬出一层，顶层自然收齐。

修后的中间态（`[{ 1 + }] len .`）：

    classify   → [[6 [[5 [[0 1] [3 16]]]]] [3 66] [3 144]]
    fuse-hof   → [[9 "__q0" [[0 1] [3 16]]] [6 [[16 "__q0"]]] [3 66] [3 144]]
    split-defs → [[[57 "__q0"] [48 1] 52 66 144] [["__q0" [[48 1] 16]]]]

**未采用另一条候选（cgen 支持 opcode 53 内联引号）**：内联引号只能借 gcc
嵌套函数，而嵌套函数的 trampoline 在外层函数返回后失效——引号一旦被存进
列表再传出去就是悬空指针。抬升成顶层静态函数没有这个生命周期问题。

### 新增 —— 两链契约测试 + 覆盖门禁 8 例

- `crates/whisper-cli/tests/integration_tests.rs`：
  `multiple_quotation_literals_compile_on_both_chains` 扩到 2/3/4 个引号；
  新增 `quotation_in_list_literal_compiles_on_both_chains`（9 例：纯列表回归
  + 列表内引号 + 取出来 `call` 的动态分发 + 嵌套列表）。
- `tools/opcode_coverage.sh`：新增 `quote-pair/triple/quad` 与
  `qlist-len/-len2/-plain/-nest/-call/-disp/-deep/-arity`，共 11 例。

### 验证

    cargo test --workspace        475 / 0
    cargo clippy -D warnings      0
    cargo fmt --all --check       0
    syntax / stdlib               26 / 0 · 58 / 0
    typecheck-gate                FP=0 PASS
    opcode_coverage               70 / 0 / 0
    net-gate / app-gate           PASS / PASS (13 routes)
    verify-fixedpoint             gen2 == gen3

**未重生种子**：种子自 2026-09-03 冻结以来 `compiler.all.ws` 已改动多次而
固定点始终成立——种子是链的起点、不是编译器的副本，改 `.ws` 不需要重生它
（`runtime.c` 由 `#include` 现场取用，同理）。本次新增代码不含"列表里放
引号"的构造，旧种子能原样编译新源码。

## Unreleased (2026-09-21g) — 战役 O（部分达成）：三处自举编译器缺陷

目标原本是"让自举链支持引号作为一等值"，使 `call` 的动态分发两链一致。
**未完全达成**——但过程里修掉三处真实的编译器缺陷，并按预注册纪律
（`tests/o_preregistered.md`）只落地**真实用户路径验证过**的部分。

### 修复：自举编译器三处缺陷

1. **`cl-list` 缺一趟 `structify`**（`classify-nested`）。`cl-quote` 是
   `dup 1 @nth structify classify-nested`，`cl-list` 却只有
   `dup 1 @nth classify-nested`。收集器把方括号内的 token 留成裸串
   （`cbr-loop` 只归组嵌套方括号、不认花括号），少了这趟 `structify`，
   列表元素里的 `{` 就被分类成**普通词**而非引号节点——实测分类树
   `[4 "{"]`。修复后为 `[6 [[5 [[0 1] [3 16]]]]]`。
2. **`next-consumer-op` 越界**：`dup 1 @nth` 未查长度，引号恰好是最后一个
   token 时（`{ 1 + }` 单独成程序即可触发）整个编译器崩在
   `Index out of bounds`。加长度守卫。
   （附带修掉第一版守卫里多余的 `drop`——`??` 已消费条件，那个 `drop`
   吃掉的是 token 列表本身；由 `--trace` 定位。）
3. **`genq-name` 残留计数器**：非首次调用时，计数器字符串既已被消费也不再
   需要，却被多读一次 `"__QN"` 留在栈上。单引号看不出来（真分支有 `drop`），
   **每多一个引号就多一份垃圾**，把真正的操作数挤出可达范围 →
   `Type mismatch: expected [T], got str`。修复：直接用栈上那个值。

另外去掉 `fuse-hof` 的 `__FH-N` **共享 KV 临时键**（递归调用会覆盖它，
名字改为全程留在栈上）——与第 3 条同属"用全局暂存传值"的隐患。

### 未达成（如实登记，未加白名单）

- **≥3 个引号**：用户路径（`whisper build -t c2`）仍报
  `Type mismatch: expected str, got [T]`。**2 个引号已两链一致（7）。**
- **列表字面量里的引号**（原判据 O-1/O-3）：根因已定位到行 —— `op-quote`
  发射 `[53, inner_bc]`，而 `53` 在整个 `compiler.all.ws` 里**只出现在
  `op-quote` 自己那两行**，cgen 没有 53 分支（`op-quote` 实为死代码）；
  `op-list6` 用含 `fuse-hof` 的 `compile` 编译元素，提升出的
  `[9, name, body]` 落在列表节点内部，而 `compile-one` **有 tag 16 分支、
  没有 tag 9 分支** → 定义被丢弃、引用留着 → `'W___q0' undeclared`。
  转入 `docs/language-backlog.md` **战役 O′**（含两条修法方向与预注册判据）。

### 顺带发现：门禁的 c2 腿与用户路径不等价（**结论已撤回，见 2026-09-21h**）

> **撤回**：本节结论错误。NSC 那次并没有真正成功——它吐出了 C 但**链接
> 失败**（手工脚本漏了 `-lws2_32`，门禁里有），两条路径其实都失败。
> 真正的原因是 `try-def-colon`/`try-loop-inner` 的裸 `streq` 在
> VM 与 C 链上行为分歧，详见 2026-09-21h。原文保留以存档。

`tools/opcode_coverage.sh` 的 c2 腿走 **NSC**（先把 `compiler.all.ws` 编成
原生二进制、再 `NSC -c < src`），而用户执行的 `whisper build -t c2` 会
**先内联 `std/core`**。三个引号的程序在 NSC 下成功、在用户路径下失败——
**NSC 通过 ≠ 用户能过**。

据此**拒绝**把 `quote-triple` 登记进门禁：它会在门禁里显示绿色，而对用户
是红色的。门禁本身改为以用户路径为准（本次只登记 2 引号一例）。
（该拒绝在 2026-09-21h 被推翻：`quote-triple` 现在是绿色的，因为缺陷已修。）

### 验证

    cargo test --workspace        474 / 0
    cargo clippy -D warnings      0
    cargo fmt --all --check       0
    syntax / stdlib               26 / 0 · 58 / 0
    typecheck-gate                FP=0 PASS
    opcode_coverage               60 / 0 / 0
    net-gate / app-gate           PASS / PASS (13 routes)
    verify-fixedpoint             gen2 == gen3

**未重生种子**：种子自 2026-09-03 冻结以来 `compiler.all.ws` 已改动多次而
固定点始终成立——种子是链的起点、不是编译器的副本，改 `.ws` 不需要重生它
（`runtime.c` 由 `#include` 现场取用，同理）。

## Unreleased (2026-09-21f) — 语言侧：`call` 补上缺失的拼写 + 两处修复

### 新增 —— `call`：动态调用引号（把一个悬空的 opcode 变得可达）

`Opcode::CapExec` 自始就在 VM 里实现（`Value::Ref(code) =>
execute_ref(code)`），但**从源码不可达**：设计原意是 `!`，而 `!` 已被
逻辑非占用（spec §3.2 冻结），词法层的 `TokenKind::Bang` 从此不再被产出，
opcode 就此悬空——文件里留着 `!` 的注释，parser 里留着
`Bang => CapExec` 的分支，但没有任何输入能走到它。

现以 `call` 词暴露（`call` 此前未被占用；`exec` 已被 std/os 的能力包装
占用故不可用）。语义：消费栈顶引号并执行，引号的输入取自调用者数据栈、
产出留在调用者数据栈——与 `@map`/`@fold` 这类提供元素槽的消费者不同。

    { 42 } call .                   # 42
    10 { 2 * } call .               # 20
    : ap { call } ; 5 { 1 + } ap .  # 6

- Rust 侧：`token.rs` 把误导性的 `Bang` 正名为 `CapExec` 并补
  `keyword_name` 往返（`call` 可作 def 名）；`lexer.rs` 加关键字；
  `checker.rs` 建模栈效应（弹引号、把其声明产出压回栈）。
- **C 链**：cgen 一直把 `call` 当普通词发射 `W_call()`，但没有任何地方
  定义该符号——于是 VM 能跑、c2 编译报 gcc 错。与 `@n` 同一模式，因此在
  `runtime.c` 里实现 `W_call`，**不动冻结的 compiler.all.ws**：引号字面量
  发射为裸函数指针（`w_push(W_PTR((void*)W_qN))`），正是高阶 helper 已在
  用的 `wfn` 形状；用 `wval_is_heap` 把代码指针与 arena 里的字符串/列表
  区分开，失败走 `w_fail` 以便被 `try` 捕获（对齐 VM 的可捕获 TypeMismatch）。
- 门禁：`opcode_coverage.sh` **56 → 59**；新增集成测试
  `call_invokes_quotation_on_both_chains`（每个程序只放一个引号字面量，
  理由见下）；spec 新增 §16 词汇附录 v1.0.9。

**发现的边界（先前就存在，与 `call` 无关）**：自举链只在程序含**单个**
引号字面量时能编含 `call` 的程序。引号作为一等值——两个及以上引号字面量、
或引号出现在列表字面量里——会让 `whisperc compile-full` 报
`Type mismatch`。`[{ 1 + }] len .` 不含任何新语法，在引入 `call` 之前就
编不过（已用 stash 在 HEAD 上对照复现）。`call` 只是让这个洞变得可见：
动态分发（把引号放进列表/map 再取出来调用）正是引号作为一等值的主要
用途。已立项为**战役 O**（`docs/language-backlog.md`），含实测对照表与
预注册判据——修它要动固定点基准并重生种子，故需要独立立项。

### 修复 —— `try` 的引号合同过严，strict 默认下误拒合法代码

`checker.rs` 对 `try` 用 `expect_ref`，即要求引号是净零 `[T] → [T]`。
但 `try` 是**捕获**体的产出的（VM 跑完弹一个值、默认 0，再压
`[flag, value]`），体有净产出完全合法。于是默认 strict 下：

    { 1 0 / } try drop "caught" .   ->  try quot: expected [T7] → [T7],
                                       got [T6] → [T5 i64 | f64]

而 VM 一直跑得好好的（`--no-strict-types` 下输出 `"caught"`）。
改用 `expect_ref_shape(stack, 0, None, ...)`——"产出不限"，与 `@each`
同一个合同。引号体内的真错误仍照抓（`{ "s" 1 + } try ...` 依旧报
`arithmetic lhs: expected i64 | f64, got str`）。

### 修复 —— 文件能力的相对路径永远被拒

`path_is_allowed` 把请求路径归一化后直接与白名单根比较，而根是绝对路径
（`current_dir()`），所以相对路径**永远不可能** `starts_with` 它——
`--allow-file-write` 下 `write-file "x.txt"` 根本用不了。失败关闭，不是
安全洞，但是不可用。

现在相对路径先与 CWD 拼接再归一化比较——这正是文件系统打开文件时做的事。
`..` 仍在拼接**之后**按词法解析，所以逃逸的相对路径照样被包含性检查拦下。
用 `has_root()` 而非 `is_absolute()`：后者在 Windows 上对
`/allowed/dir/file.txt`（无盘符的盘相对路径）返回 false，会把它错误地送进
CWD 拼接分支——这个 bug 是本机（Windows）跑既有安全单测时抓到的，Linux
上不会暴露。

新增 2 个单测（相对路径可用、相对路径逃逸仍拒），原有 6 个安全单测行为
不变（它们全用绝对路径）。

### 验证

    cargo test --workspace        473 / 0
    cargo clippy -D warnings      0
    cargo fmt --all --check       0
    opcode_coverage               59 / 0 / 0

## Unreleased (2026-09-21e) — 整体回顾：CI 从未运转 + 一个被它掩盖的字节冲突

对独立性工作做整体复核时发现的问题。第一条是根因，后面几条都是它掩盖的。

### 修复（P0）—— CI 工作流文件是非法 YAML，**从未运行过**

`.github/workflows/ci.yml` 里 `name: App gate (Whisper web backend:
JSON/static/404 routes)` 的冒号未加引号，YAML 解析直接失败
（`mapping values are not allowed here`）。GitHub Actions **无法加载该
工作流**，所以 `test`/`selfhost`/`c-backend`/`lint` 全部从未执行。
CHANGELOG 里历次"四 CI 门全绿"的说法来自本地手工跑，不是 CI 的实际结论。

修复：该 name 加引号。校验后 6 个 job 全部可解析
（`test` / `c-backend` / `c-backend-clang` / `selfhost` / `lint` / `release`）。

**这条解释了本轮的另外两个回归为何能存活**：递归守卫 P0（2026-09-21b）
与下述字节冲突，都会被 `test`/`selfhost`/`lint` 任一条腿拦住。

### 修复（正确性）—— `.wbin` 指令字节冲突：`Try` 与 `BytesToHex` 共用 0xC1

`BytesToHex`（2026-09-18，L-12 新增）被分配了 `0xC1`，而 `Try` 自
2026-06-07 起就占着该字节。冲突被复制到**四处**：

| 位置 | 角色 |
|------|------|
| `opcode.rs` `to_byte` | 编码 |
| `wbin.rs` `from_byte` | `.wbin` 解码 |
| `bootstrap.rs`（编码侧） | 自举管线 |
| `bootstrap.rs`（解码侧） | 自举管线 |

三张解码表都是 `0xC1 => BytesToHex` 在前，于是**每一段 `try` 写进 `.wbin`
再读回来都变成 `bytes->hex`**，运行时炸在 "expected i64, got ref"。
实测复现：同一程序直接 `run` 正常，经 `.wbin` 往返后报错。

修法：新来者让位 —— `BytesToHex` 迁到空闲字节 `0xCE`，`Try` 保留
`0xC1`。`.wbin` 里含 `try` 的程序自此恢复正常。

**防回归**：`opcode.rs` 新增 `to_byte_is_injective` —— 158 个变体逐个取
字节，断言两两不同（`from_byte(to_byte(op)) == op` 那类往返断言**抓不到**
编码侧冲突，必须直接查重）。已反向验证：把字节改回 0xC1，测试立即报
`0xC1 shared by BytesToHex and Try`。

### 清理 —— `lint` 作业的既存债（CI 复活后会立刻变红）

- **rustfmt**：132 处差异 / 34 文件（含大量本轮未触碰的文件）→
  `cargo fmt --all`，现 `--check` 干净。
- **clippy**：39 warning → **0**，`cargo clippy --workspace -- -D warnings`
  通过。其中数条是真问题而非风格：
  - `xorshift64_next` 自由函数是死代码（VM 用的是同名方法，逻辑重复）→ 删。
  - `parser.rs` 的 `mod tests` 缺 `#[cfg(test)]`，导致测试模块被编进
    lib 目标（clippy 因此报 `use super::*` 未使用）→ 补门控。
  - `vm.rs` 一个函数上叠了两个 `#[test]`（前一个测试函数被删后留下悬空
    文档注释与属性）——**这让 libtest 多注册了一个幻影测试**，所以历史
    计数 471 里的 1 是虚的，真实基数是 **470**。
  - `builtins.rs` 的 `rot` / `@filter` 各重复一条（签名相同，无害），
    `vm.rs` 的 `"list?"` 重复一条 → 删冗余。
  - 其余为 `type_complexity`（加别名）、`is_multiple_of`、
    `eq_ignore_ascii_case`、`while let`、`Default`、十六进制分组等。

### 未修（如实记录，各自独立）

- `try` 接一个"有净产出"的引号时，strict 类型检查会拒（`expected [T]→[T],
  got [T]→[T i64|f64]`）。**HEAD 上同样如此**（已用 stash 对照验证），
  属检查器对 `try` 签名的建模问题，不在本轮范围。
- `path_is_allowed` 拿相对路径与绝对白名单比较，故 `--allow-file-write`
  下相对路径永远被拒（失败关闭，非安全洞）。在安全边界上且已有 6 个
  加固单测，应单独评估。
- HTTP 能力（`@2/@3/@6`-`@9`）仅 VM 侧可用 —— Step 2b。
- 仓库根目录三个历史调试残留（`out_fire_stage.txt` / `out_l2.txt`，
  1–3 字节，非任何脚本产出）未动。

## Unreleased (2026-09-21d) — Step 6：C 降级为可替换接口

把 C 从"依赖"变成"接口"。核心不是删掉 C，而是**让任何一个 C 编译器
都能当后端**——这恰好是当初选 C 的理由，之前却因为两个隐含假设没兑现。

- **`W_TLS` 条件宏**：原来无条件 `__thread`，等于把运行时悄悄钉在
  GCC/Clang 上。现按 MSVC → `__declspec(thread)`、C11 → `_Thread_local`、
  其余回退 `__thread`。
- **移除 `-fno-strict-aliasing`（先证明再删）**：
  - 静态审计：无类型双关（`*(double*)&x` 类 0 处、无 union），位模式
    转换全部走 `memcpy`；`w_str_new` 的 `*(uint32_t*)p` 合法（arena
    存储无声明类型，首次写入确定有效类型）。
  - 动态验证：取最重工作量（`compiler.all.ws` 65 KB → 205 KB C），
    带/不带该标志各编一个自举编译器，编译同一程序输出**逐字节一致**
    （SHA256 相同）。
  - 从 **9 个构建路径**移除（CLI 的 c2/native、bootstrap、fixedpoint、
    覆盖门禁、run_core、两个测试、README 示例），并 bump `-t native`
    内容寻址缓存键（`-O3-flto-v2`）以免复用旧标志下构建的条目。
  - **决定性理由不是性能**：MSVC 根本没有这个标志，它就是"C 可替换"
    的直接阻碍。
- **规范锁定**：`docs/spec.md` 新增 §15「C 发射契约」——C99 + POSIX/Win32、
  禁止编译器扩展、不得要求非默认标志、不得依赖 UB、编译器矩阵至少
  gcc + clang 两条腿。违反即为**规范级缺陷**。
- **CI 新增 `c-backend-clang` 腿**：`WHISPER_CC=clang` + `GCC=clang`，
  跑 C 后端测试 + stdlib 门禁 + 覆盖门禁。这是 clang 的唯一验证途径
  （本机无 clang，gcc 腿为本地实测）。
- **残留风险（如实记录）**：删标志的依据是"静态审计干净 + 最重工作量
  逐字节一致"，但**通过测试不等于证明无 UB**。若将来在某编译器/优化
  级别上出现异常，正确反应是把标志加回来并记录触发条件。
- **未纳入**：tcc / msvc 腿。MSVC 需要的不止是标志（`unistd.h`、`popen`
  命名等），属另立项。

## Unreleased (2026-09-21c) — Step 2a：C 链能力 ABI 接线（`@n` 令牌）

### 背景：`exec` 早就存在，缺的是整条能力 ABI

`exec` 不是缺词——它是能力令牌 `@5`（`stdlib/os.ws`），VM 侧一直可用。
真正的洞是 **C 链不支持 `@n` 令牌**：cgen 把 `@n` 当普通词引用，一律发射
`W__<n>()`（`compiler.all.ws:1293`），而这些函数从不生成，gcc 直接报
`implicit declaration of function 'W__4'`。后果远超"缺 exec"：
`import "std/os"` 经 c2 **根本编不过**。

之所以长期未发现：**c2 的六道门禁没有任何一个用例碰过能力令牌**——
stdlib-gate 测 math/list/str，net/app 走原生 socket 操作码。覆盖盲区，
不是偶发疏漏。

### 新增：runtime.c 实现 cgen 既成的能力 ABI

`@0`（file read）、`@1`（file write）、`@4`（env）、`@5`（exec）四个
shim，语义逐条镜像 `capability.rs`。**不动 `compiler.all.ws`**——那是
固定点基准，改它必须连带重生种子并重跑整条自举链；ABI 是 cgen 既成的
契约，实现它比修改它便宜得多。

- `@5` exec 用 popen + stderr 重定向到临时文件（popen 只能捕获单流），
  命令包在 `( ... )` 组里让重定向覆盖整条命令，stdin 接空设备以对齐
  Rust `Command::output()`——否则读 stdin 的命令会永久挂住 VM。
  状态码走 `WEXITSTATUS`，两流按 VM 的 `String::trim()` 修剪。
- 未覆盖 **2/3/6/7/8/9（HTTP 家族）**：需要 C 侧 HTTP+TLS 客户端，
  是独立中等工程（Step 2b）。用到它们的程序仍然编不过——这是诚实的
  结果，好过产出错误的二进制。

### 修复：VM 侧两处能力缺陷（双链对拍时暴露）

- **`Opcode::CapCall` 恒弹一个操作数**，而 `FileWriteCap`/`HttpPostCap`
  需要两个（`path content`）——所以 **`@1` 在 VM 里从来不可用**，而 C 链
  （按 shim 实际需要弹）与它分歧。新增 `Capability::arity()`（默认 1，
  `FileWriteCap` 覆写为 2），`CapCall` 按其弹取并反序成声明序。
  HTTP 能力的 arity 保持默认，行为不变（属 Step 2b）。
- **`FileWriteCap` 写完不压任何结果**：既静默丢弃失败，又与原生
  `write-file` opcode、C 链 `w_write_file`、以及类型模型三者都不一致。
  现返回字节数。`@1` 此前完全不可用，无兼容负担。

### 修复：`@n` 令牌缺类型签名（strict 默认下误拒合法代码）

`checker.rs` 把 `CapCall` 归入 `_ => {}`（既不吃操作数也不产结果），
于是 `"cmd" exec 1 @nth .` 在**默认 strict 模式**下被拒
（"@nth list: expected [T0], got str"）。现为 0/1/4/5 四个 id 建模栈效应
（`@5` 的异构结果用语言既有的 `Type::Union`）。与 J-3 修过的 `http-*`
内建签名缺失同属一类。HTTP 家族保持不建模（Step 2b）。

### 验证

- **双链对拍**：`@0/@1/@4/@5` 五组用例 VM 与 c2 输出逐行一致
  （含非零退出码 7、独立 stderr 捕获）。新增 Rust 集成测试
  `capability_abi_matches_across_chains`（覆盖文件对，因 VM 的文件能力
  白名单锚定 CWD，shell 里构造跨平台绝对路径过于脆弱）。
- `tools/opcode_coverage.sh` 新增 4 例（cap-env / cap-exec-out /
  cap-exec-st / cap-exec-err）；VM 侧运行行补上能力绑定标志。
- 回归：cargo workspace **470/0**；typecheck 棘轮 FP=0 / TP=23 不变。

## Unreleased (2026-09-21b) — P0 回归修复（递归守卫阈值）+ Step 1：`-t exe` 去 Rust 化

### 修复（P0 —— 上一提交自带的回归）

- **词调用深度守卫阈值过低，c2 链对真实程序全面不可用**。上一提交
  （eec695f）为 `: f { f } ; f` 的 30 GB 分配问题加了 2048 帧上限，
  但**自举编译器是"用递归代替循环"遍历 token 的**，所需深度 ≈ token
  数，随输入规模线性增长。实测：

  | 输入 | 体积 | 所需帧数 |
  |------|------|---------|
  | `stdlib/math.ws` | 1.9 KB | 2 049 – 4 096 |
  | `whisperc/compiler.all.ws` | 65 KB | 16 385 – 32 768 |

  后果：`whisper build -t c2` 对**任何 import stdlib 模块的程序**直接报
  `RecursionLimit`，连 `compiler.all.ws` 自己都编不过——而
  `tools/opcode_coverage.sh` 正是这样构建自举编译器的。`stdlib-gate` /
  `syntax-gate` / `app-gate` / `net-gate` / 覆盖门禁**全部受影响**。
  上一提交的回归记录只写了 cargo 与固定点（固定点走 C 种子链，绕开了
  Rust VM 的守卫），所以未被发现。
- **修法**：默认上限 2048 → **262 144**。对已知最大工作量（32 768 帧）
  留 8× 余量；按 48 字节/帧计最坏约 25 MB，而原始 bug 是 30 GB。
  另新增 `WHISPER_MAX_CALL_DEPTH` 运行时逃生口（诊断手段，非安全边界——
  内存已由数据栈守卫约束）。
- **回归测试 4 项**：`c2_chain_compiles_stdlib_imports`（std/math 经
  c2 链编译并运行，端到端复现原故障）、`deep_user_recursion_beyond_old_ceiling_runs`
  （5000 帧用户递归可运行）、`test_deep_but_legitimate_recursion_is_allowed`
  （上限不变量：>32768 且 ≤1M）、`test_word_recursion_depth_guard`
  （改为断言当前阈值而非硬编码 2048）。
- **复验**：std/math 0.8s、webapp server 2.0s、compiler.all.ws 31.2s
  全部恢复可编译；syntax 26/0、stdlib 58/0、typecheck FP=0 全绿；
  cargo workspace **469/0**。

### 变更（Step 1：`-t exe` 去 Rust 化）

- **`-t exe` 改为自举链原生产物**。此前它把 **Rust CLI 二进制本身**复制
  一份再追加 wbin 负载（`build.rs` 的 `current_exe()` + `WSHPEXE1`
  trailer），于是"自包含可执行文件"里装的其实是整个 Rust VM（还捎带
  LSP/REPL/类型检查器/包管理器）。现改走 c2 链产出普通机器码，
  **产物零 Rust**。
- **`-t bundle` 保留旧行为**（解释器打包：构建期不需要任何工具链），
  帮助文本与文档如实标注 `embeds the Rust VM`。两种分发模型不再共用
  一个名字。选择保留而非删除，是因为"没有 C 编译器也能出单文件"这条
  工作流是真实的，只是它必然带 Rust，应该按名字说清楚。
- **产物路径回显修复**：mingw gcc 会把 `-o app` 自作主张变成 `app.exe`，
  而 CLI 此前回显的是**请求路径**而非真实产物路径。现解析实际存在的
  文件再报告（影响 c2 / native / exe 三个目标）。
- **测试**：新增 `exe_target_is_native_and_carries_no_rust`——断言产物
  可运行、**不含** `WSHPEXE1` trailer、且确实经过 C 链（同级 `.c` 存在）；
  原 `bundled_exe_*` 两个测试迁移到 `-t bundle` 并更名 `bundle_target_*`。

### 环境发现

- 本机 gcc 一直存在于 `C:\msys64\mingw64\bin`（未入 PATH），因此此前
  "无 C 编译器即跳过"的 c2/C 后端测试**从未在本机真正执行过**。纳入
  PATH 后 12 项 c_backend 与全部 c2 链门禁首次在本机跑通——上述 P0
  回归正是这样暴露的。

## Unreleased (2026-09-21) — 版本号与文档口径对齐（纯文档，零行为变更）

- **版本号统一为 1.0.8**：`docs/spec.md` §13/§14 的词汇附录已在
  v1.0.8 / v1.0.8b（FFI + 字节语义补全），`ffi.rs` 与 `runtime.c` 亦以
  v1.0.8 标注；而 `Cargo.toml`、CLI `VERSION`、`package.ws` 仍停在
  1.0.0，README 徽章写 1.0.7——四处口径互不一致，本次统一到 1.0.8。
  已核：无任何测试断言版本字符串（`--version` 用例仅断言退出码）。
- **README 失真修正**（逐条实测后重写）：
  - 徽章：version 1.0.7→**1.0.8**、tests 451+5 gates→**465+7 gates**、
    stdlib 14→**20 modules**；
  - "手写 C 种子"→**冻结 C 种子**：`seed/compiler_seed.c` 头部自述
    2026-09-03 起为"100% generated C"（由自举链自身产出，旧手写种子
    在 git 历史），自举章节文字与流程图同步修正；
  - 功能表：`137 opcodes`→**154**（`opcode.rs` 枚举逐行实测，
    154 个变体全部出现在 `to_byte`/`from_byte`）；"静态类型推导
    （当前仅警告）⚠️"→**strict 默认 ✅**（N-2 已翻转，README 落后于
    代码）；"标准库 13 模块, 60+ 函数"→**20 模块**（20 个 `stdlib/*.ws`
    逐个 `import` 实测通过）；
  - c2 段：`v1.0.3-1.0.7`→**v1.0.3-1.0.8**（FFI 已在该链上跑通，
    stdlib-gate 含 p8-ffi 用例）；值标签 3 个→**5 个**
    （WT_I64/WT_F64/WT_PTR/WT_SIG/WT_BOOL，`runtime.c:32`）；
    残留分歧两条经复核**仍成立**（C 链串打印不带引号；
    `w_val_eq` 仅比标签+位，非深比较），保留并补注出处；
  - 测试章节：451→**465**、54→**58** 双链断言，补 opcode 覆盖门禁与
    typecheck 棘轮两道（门禁计数 5→7）；覆盖清单按 crate 实测重写
    （CLI 139 / core 132 / codegen 138 / parser 34 / typecheck 15 /
    LSP 5 / package 2 = 465）；
  - 项目结构：补 `apps/wheels`、`adapters`、`tests/conformance`、`tools`。
- **docs/stdlib.md**：模块计数 14→**20**（本文详列 15 个，其余
  `std/core`、`std/net`、`std/random`、`std/time` 点名，`std/ffi`
  指向 spec §13/§14）。
- **明确未动**：`registry.json`（包注册表 schema 版本与 std 包自身
  版本，语义不同）、`vscode-extension/package.json` 0.2.0（插件独立
  版本线）、`docs/spec.md` 头部 "v1.0 (FROZEN)"（语法合同版本）。
- **遗留待办（非本次范围）**：仓库内仍有与权威文件漂移的历史快照
  （`whisperc/compiler.ws` 手工合并版、顶层 `whisperc/*.ws` 与
  `whisperc/src/*.ws` 日期错位），以及 `-t c`（Rust 转译器）与
  `-t c2`（自举链）两条通往同一目的的 C 生成路径——清理需单独评估
  影响面，未纳入本轮零风险对齐。

## Unreleased (2026-09-18) — 六阶段计划 Phase 4 收尾：VM 递归防护 + 死字段清除

- **词调用深度守卫（P4.3）**：`Opcode::Call` 在 call_stack 深度
  ≥2048 时返回 `RecursionLimit`。此前解释器虽是迭代帧（无原生栈
  溢出），但 `: f { f } ; f` 会无限增长 call_stack 直至分配器
  abort 整进程（实测 30GB 分配失败）。ref 块守卫（512）此前已有，
  本次补齐词调用路径。错误文案泛化（word-call or quotation nesting）。
  回归：`: f { f } ; f` → 干净 RecursionLimit；VM 单测锁定。
- **CallFrame.base 死字段清除（P4.6）**：全仓库零读取（仅 4 处
  构造赋值），连字段带构造一并移除。
- **Phase 4 盘点确认**（计划快照滞后，实测已完成项）：Value::equals
  List/Map 深比较 ✓、StrSlice 字节语义+边界 ✓、Take/DropN/Nth 负数
  NegativeValue ✓、i64 溢出安全（实测 wrap 不 panic）✓、
  `Rc<[Opcode]>` word_dict ✓、capability 词法归一（ParentDir pop +
  sibling 前缀攻击测试，6 单测）✓、cgen transpiler W_qN 引号池 +
  runtime.c 高阶 helper 装线 + TODO=0 + gcc 端到端测试 ✓。
- **回归**：cargo workspace 465/0；固定点验证 gen2==gen3 OK
  （scripts/verify-fixedpoint.sh 全程 ~25min，当前树实测）。
- （同期 L 系列战果见 1512ca1/635c814/cda7db3：#48 keyword 冲突
  警告、L-12 bytes->hex + http-req-raw、L-11 http-req-parallel。）

## Unreleased (2026-09-10o) — N-3：WT_BOOL 第五标签（布尔全对齐）+ Type::Map 保险层

- **WT_BOOL**（runtime.c 值模型第五标签 + cgen 全量发射改造）：
  原 N-1 判定的两格"永久分歧"闭合——`#t .`→#t、`1 2 < .`→#t、
  `#t json-stringify`→true、`"[true]" json-parse 0 @nth .`→#t、
  `{ 42 } try . .`→42/#t、`#t:0.5 unwrap .`→#t。布尔字面量/比较
  （= < > != <= >=）/逻辑（& | !）/streq/map-has/谓词（list? i64?
  str? bool?）全部 W_BOOL；w_val_eq 对齐 VM equals（仅 Bool↔I64
  跨标签数值比）；bool? 精确判定（I64 1 非 bool——旧 0/1 启发式
  修正）；数值位协变（w_as_i64/w_num_f64 收 BOOL）。12 项探针
  双侧一致；spec §4 撤两行永久分歧。stdlib 门禁 3 例期望值更新
  为 VM 真值（#t）。
- **Type::Map**（typecheck 保险层，替代 kv effect 行）：借位计数
  丢失的 9 条真阳性经调查全部为 map 类型形状（map-get 收 i64）而
  非 kv 键序——A-SK effect 行以证据裁决不做；对症加 Type::Map +
  map-* 五词参数精确化：**TP 6→7、FP 保持 0**。
- **回归**：cargo 460/0；覆盖门禁 52/0/0；syntax 26/0、stdlib 58/0、
  net、app 四门全绿；typecheck-gate FP=0/TP=7；固定点
  gen2==gen3 OK。

## Unreleased (2026-09-10n) — N-2 Phase 2.5+4 收官：**strict 默认达成**（J-3 战役关闭）

J-3 归档为"不可修补"的目标经分阶段重设计达成：**type 错误默认致命，
`--no-strict-types` 降级逃逸**。三语料假阳性全零。

- **?? 臂逐槽合并**：两臂按槽位对齐合并（unify/union），不再只取
  last()——J-3 当年撤回是因被坏签名放大，Phase 2 freshen 后放大源
  已除，重试成功（t_mk4 双 FP 清零）。
- **内建签名实例化**：builtins 表的 TypeVar(99)/(0) 是固定 id，
  所有调用点共享同一变量、首点绑定污染全局（t_base 最后一条 FP
  的根）——与词签名同走 freshen。
- **借位计数**：infer_sig 模式下词体欠压操作（体首 `dup`/`drop`
  等）合成 fresh 输入并计数，签名输入数=借位数——J-3 "体首即消费
  给 0 输入"（tp-key 族）的修正。真阳性 15→6（部分真 bug 转为
  静默，如实记录）。
- **异构列表 union**：`[1 2 [3 4]]` 运行时合法——元素型升级 union
  而非报错（conformance 2→0）。
- **翻转**：main.rs 三处 flag 改 `!get_flag(--no-strict-types)`；
  帮助文本更新；覆盖门禁 VM 侧加逃逸口（行为对拍用例含刻意栈残留）。
- **验收**：cargo 460/0、syntax 26/0、stdlib 58/0、net、app、
  覆盖门禁 52/0/0、typecheck-gate 三语料 FP=0（TP 6 不罚）。
- 调试辅助保留：WHISPER_TC_TRACE=1 打印语句级类型栈。

## Unreleased (2026-09-10m) — N-2 Phase 2：多态实例化 + 假阳性普查（FP 仅剩 3）

- **调用点实例化**（checker.rs freshen）：WordRef 每次调用深拷贝
  签名并把 TypeVar 整体换新——消灭签名 var-id 被首个调用点绑定后
  全程序污染的一类。
- **决定性普查**：剩余 18 诊断 × 运行时成败交叉表——**假阳性仅 3**
  （t_base:1 + t_mk4:2，均为共享闭括号多 `??` 链的臂槽建模类，
  J-3 撤回项真身）；**真阳性 15**——检查器抓到的真 bug（t_q*
  全家 + t_ind6/s7，运行时确实错；早先手工 rc 读数有误，由门禁
  修正）。检查器的价值首次被定量。
- **门禁升级**：typecheck-gate 改为 FP 感知棘轮——运行时成功文件
  上的诊断才计 FP（CEIL_FP=3）；真阳性不惩罚。
- **回归**：cargo 460/0；覆盖门禁 52/0/0；encoding 0 /
  conformance 2 基线不动。
- **后续**：Phase 2.5（?? 臂逐槽建模）清 FP 3→0 即达 strict 默认
  前置。

## Unreleased (2026-09-10l) — N-2 Phase 1：引号栈合同——lexicon 诊断 32→18

- **消费者合同重设计**（checker.rs）：expect_ref 的 t→t 净零形状
  （J-3 战败根因之一）改为按消费者 arity——@map/@filter 至少 1
  产出、@each 产出不限（体合法地追加多输出）、@fold 2 提供槽 +
  至少 1 产出、@times 不设形状；产出检查带消费者视角换算
  （`o + (提供 − 声称)`，@fold 的 `{ + }` 正确过）。
- **引号体入口栈参数化**：从固定单元素栈改为（环境 Σ×4 + 元素
  槽×2）起查，`$N` 越过元素落到真槽；Ref 的 arity 按**低水位触底**
  构造（自产自弃不计触底），产出按入口边界切片。
- **关键语义发现**：VM Each/Fold 清栈单元素执行（父栈停靠）——
  引号根本够不到环境栈；入元数触底审计在该粒度下是噪声（t_q9
  实证），撤入检查、留给 Phase 2 按消费者入口重查。
- **战果**：typecheck-gate lexicon **32→18**（净清 14：t_ind10/
  t_ind10c/t_ind3/t_ind4/t_ind7/t_ind7a/t_ind9/t_dump/t_q9 全清，
  t_ind6/t_ind6s7 各 4→1）；conformance 2 / encoding 0 维持基线。
  剩余 18 全为槽污染类（t_q1-qc/t_mk4/t_base）——Phase 2 多态
  实例化的靶子。
- **回归**：cargo 460/0；覆盖门禁 52/0/0；stdlib 58/0；syntax 26/0；
  棘轮降档 18 并 PASS。

## Unreleased (2026-09-10k) — N-1 bool/atan2 语义统一（VM 协变）+ N-2 Phase 0 判据 harness

- **N-1（预注册判据全部兑现，tests/n1_preregistered.md）**：
  `#t 1 +`→2、`"ab" #t strnth`→98、`0 1 atan2`→0、`#t bitnot`→-2、
  `#t 0.5 +`→1.5——VM 三咽喉点 Bool 数值协变（binary_num_op 归一 /
  pop_i64 / pop_f64）+ equals 跨标签按数值比，与 C 链（bool 即
  i64）全量对齐。16 格行为矩阵实测兑现；永久分歧仅剩
  `#t .`（#t vs 1）与 `#t json-stringify`（"true" vs 1，wval 无
  bool 标签），spec §4 记录。
- **typecheck 数值协变**：infer.rs unify 加 Bool↔I64↔F64 两两可合
  六臂——静态假阳性清零优先，反向泄漏（f64 进 i64 位）由运行时
  动态检查兜底。诊断零扰动实证（N-1 前后双二进制 32/2/0 完全一致）。
- **N-2 Phase 0**：scripts/typecheck-gate.sh 判据棘轮——std/encoding
  向量链 0 / conformance 2 / **lexicon 32**（实测真基线；J-3 报告
  的 6 是当时语料规模，后续新增测试带入存量近似）。只许降不许升。
- **回归**：cargo 460/0；覆盖门禁 **52/0/0**（+bool 四用例）；
  Lexicon 双跑 0 diff/50。

## Unreleased (2026-09-10j) — M: map 顺序确定化 + ] 歧义文档化 + 后续战役登记

- **map 遍历顺序非确定（语言级修复）**：VM `Value::Map` 底层
  HashMap→BTreeMap——此前同一程序三次运行 map-keys 输出
  `["c" "a" "b"]`/`["a" "b" "c"]`/`["c" "a" "b"]`（RandomState），
  且与 C 链插入序不一致；现两实现统一为 bytewise 排序（C 链
  w_map_set 改排序插入）。覆盖门禁新增 map-order 双用例锁死。
- **`]` then-臂列表贴近闭合括号的两实现分歧（文档化已知限制）**：
  `#t ?? [1 2 3] ] ...` VM 解析器拒绝（Unexpected RBracket）、自举
  链接受——spec §2 补已知限制与规范写法（臂内列表后至少跟一词，
  或列表前置构造）。
- **后续战役登记（docs/language-backlog.md）**：N-1 bool/atan2
  语义统一（两后端行为分叉，`#t 1 +` VM 报错/C 链=2；含 16 格
  预注册判据矩阵要求）；N-2 typecheck strict（J-3 复活，前置为
  checker 重设计：多态变体推断+引号栈合同+kv effect 行）。
- **回归**：覆盖门禁 48/0/0；四 CI 门全绿；cargo 460 全绿；
  固定点 gen2==gen3 OK；Lexicon 50 测试双二进制对照输出
  **字节级一致**（BTreeMap 零行为扰动实证）。

## Unreleased (2026-09-10i) — L: c2 债务清偿——try/json/conf 全链落地 + 三笔 K 前旧债

覆盖门禁白名单清空：**46 pass / 0 fail / 0 known-gap**；CI 四门
syntax 26/0、stdlib 58/0、net、app 全绿。

- **try（L-1）**：runtime.c 引入 w_fail + setjmp/longjmp——值级错误
  （类型错、除零、栈下溢、JSON 错、listen 失败）回卷到最近 `{..} try`
  并压 [#f, 消息]；成功形状 [#t, 值] 与 VM Try 逐槽对齐（含空体默认
  0 与栈回卷深度）；cgen 发射 `w_try_call(_qb);`（嵌套 try、
  try-内-json 错误捕获实测通过）。资源级错误保持致命。
- **json（L-2）**：W_json_parse_cstr / W_json_stringify 全实现——
  递归下降（对象/数组/字符串/数/true/false/null，\uXXXX BMP）；
  紧凑序列化、对象键 bytewise 排序（serde_json BTreeMap 对齐）、
  f64 文本轮转对齐（4.0 保留 .0、1e30 无 +）；map 升级为标记前缀
  对表（magic pair 首元素）——map 与形似列表在 C 链可区分，L-08
  启发式腐坏不会复发；`.` 打印 {"k": v} 对齐 VM Debug。
- **置信度（L-3）**：`#t:0.75` 三处落地——tokenizer ':' 并入
  （累积为 "#t"/"#f" 时越过 is-ws 停止判定）、classify [14]/[12]、
  cgen 发射 w_sig 包装；WT_SIG 第四标签 + w_as_*/w_num_f64/
  wval_is_str/is_list/dump 透明解包；conf-of peek 不弹、unwrap 剥壳。
- **裸 HOP + PushRef（旧债 1，K 前即缺）**：`: map { @map } ;` 式
  包装词要求裸 @map/@each/@fold/@times/@filter/try(67/68/69/83/5/193)
  发射（引号运行时到栈）——补六个 emit-bare case 接 runtime 现成
  w_map_over 等；**引号作参数**（`{ 1 + } map`）的 PushRef(53) 发射
  从未存在——fuse-hof 的"无消费者引号"分支提升为顶层合成词 __qN
  （[16,名] 压引用 token + [9,名,体] 合成 def，split-defs 全流提取），
  新指令 [57] 发射 `w_push(W_PTR(W___qN))`。
- **K-1 克隆注释 bug（旧债 2）**：行尾 "// 注释" 让单行 def 判定
  失守，下一行 def 被原样吞进克隆体（stdlib/list 的 range-to 注释
  导致 `: filter` 双定义、gcc redefinition）——判定与体拷贝改按剥
  注释后代码。
- **杂项旧债 3**：atan2 弹栈序反了 + 改数值提升（VM 严格 f64、C 链
  提升并按门禁契约 0 1 atan2 → 0）；ffi-sym 该回 I64 地址（VM 语义，
  ffi-call 按 i64 收；一修双愈 sym-zero/crc32）；kv-keys 漏 W_PTR
  包装（server.ws 编不过）。
- **堆提交 bug（顺带修复）**：list arena 提交从基地址覆盖全跨度
  （旧 4GB 实为全量急提交，16GB 下 CI 必挂）——改字符串底部/列表
  顶部各自 64KB 切片懒提交。
- **方言纪律战报**：本轮五个自举栈序 bug 全部被"VM 先跑、原生对拍"
  抓住——kv 存值序（值在键下）、$N pick 压栈后索引漂移（$2 $2）、
  prepend 参数序（elem 在底 list 在顶）、over 在单元素栈越界（用
  `_`）、?? 包装漏闭合 ]（括号恰好重排进内层臂，语义静默错乱）。
- **教训（如实记录）**：K-6 验收清单漏跑了 stdlib/app 门禁——
  PushRef/裸 HOF/克隆注释三笔 K 前旧债因此带病入库（fa947de 的
  CI selfhost 应为红）。L 阶段补齐：本地 gates 复验入验收清单。
- **回归**：覆盖门禁 46/0/0；四 CI 门全绿；cargo test --workspace
  全绿；固定点 gen2==gen3 保持（L 变更后复验）。

## Unreleased (2026-09-09h) — K: J4-4 自举链同步收官（限定量词法全链落地）

- **自举链限定量词法（J4-4）**：`mmod.mw` 在 C 链词法器并入单词，
  独立 `.`/`..`/`3.14` 不受影响——与 Rust VM 语义一致。
  `: mmod.mw { 42 } ; mmod.mw .` 全链输出 42；`3.14 .` 双侧 3.14。
  compiler.all.ws 新增 is-wordch?/dot-joins?/chunk-cont?/chunk-or-delim
  四词（保守子集纪律：无 #t/#f 字面量、kv 只存字符串）。
- **两个根因（战报 docs/namespaces-design.md）**：
  1. seed 编译器对函数内 >2 层 ?? 嵌套产出错字节码（Rust 编译器
     容忍 → VM 侧全对、原生侧坏）——原 is-wordch? 5 层嵌套重写为
     &/| 扁平布尔链（零嵌套）；
  2. 自举词法逐字符 strslice 全剩余串 → 累计分配 O(源²)：50KB
     compiler.all.ws 自编译越过 2GB 字符串 arena（冻结种子在 60KB
     合成输入同样爆——非新词引入，链本就贴崖）。runtime.c
     HEAP_SIZE 4GB→16GB（VirtualAlloc/mmap 懒提交，实际内存不变）；
     生成 C 以 `#include "runtime.c"` 引用，gen2==gen3 文本比对不受
     影响，seed 文件保持冻结。
- **c2 管线命名空间注入（K-1）**：expand_imports 内联模块后追加
  限定克隆 `: 末段.词名 { ... } ;`——c2 目标编译含 import 源时
  限定调用可用。
- **操作码覆盖门禁（K-2）**：tools/opcode_coverage.sh——36 特性
  用例双跑对拍（VM vs 自举链 -c→gcc→run），归一化已记录打印口径
  （#t/#f→1/0、去引号）；NSC 环境变量注入预构建自举编译器；
  已知缺口（json/try/conf）白名单大声记账。现况 36 pass / 0 fail /
  3 known-gap。
- **静默兜底→硬失败（K-3）**：seed cgen 未知 token/裸 opcode 发
  `#error whisper-cgen: ...`——C 编译响亮失败，不再静默垃圾。
- **数值提升修复（K-4）**：seed cgen 的 `<=`/`>=` 用 w_num_f64
  （提升，对齐 VM binary_num_op）替代 w_as_i64（F64 硬失败）；
  浮点算术/比较/打印两实现一致（spec §4 浮点差异行删除）。
- **回归**：cargo test --workspace 460 全绿；固定点
  scripts/verify-fixedpoint.sh 通过；runtime.c 死代码 fn-trace
  调试脚手架移除。

## Unreleased (2026-09-04g) — J-4 模块命名空间（Rust 侧完整落地）

- 词法：`.` 在前后均为词字符时并入词（`mmod.mw` 单 token）；独立
  `.`/`..`/`3.14` 语义不变；5 项 lexer 单测。
- resolver 命名空间注入：import 时以路径末段注册，模块每个 Def
  克隆为 `末段.词名` 附加 Def——限定形态即普通词，永不被非限定
  冲突遮蔽；冲突语义（警告+后导出遮蔽）对非限定形态保持不变。
- 实测：std/str 与自定义模块同导出 find——str.find=1、
  mymod.find="MINE"、非限定 find=遮蔽者；双向限定同时正确。
- 向后兼容：workspace 20 套全绿；Lexicon 警告基线不变；examples 0。
- 自举链同步（J4-4）未达成：VM 侧语义全对，C 编译版崩——三个
  结构性缺口实锤（seed over 的 C-gen 静默垃圾、kv 只存字符串、
  export 过滤），修复后仍有未定位发射分歧；compiler.all.ws 回退
  保固定点。战报 docs/namespaces-design.md。spec §2 词法表补
  限定量行（标注自举链未同步）。

## Unreleased (2026-09-04f) — J-3 typecheck strict 默认：未达成，如实归档

- 保留（纯增量）：http-get/post/req/audit 内建签名入 builtins 表
  （此前未知词按 0 消费 1 产出 Any 建模，http-req 4→3 栈形全断）；
  docs/examples/api_client.ws 真 bug（`.` 在 `..` 清栈后调用）。
- 试过并回退（战报详见 docs/typecheck-strict-design.md）：
  expect_ref 放松、?? 臂逐槽合并、两遍法签名输入推断——三者能把
  Lexicon 误报压到 0，但 std/encoding 向量链在同一近似下反向失败；
  两个语料库对 checker 的压力方向相反，修补不可同时满足。
- 结论：strict 默认的前置（全套零误报）需要 checker 重设计
  （多态变体推断 + 引号栈合同 + kv 态词 effect 行），已立项存档。
- `--strict-types` opt-in 维持现状（6cffb62 起可用）。

## Unreleased (2026-09-04e) — 成熟化第一批：错误恢复栈卫生 + 堆按需提交

### try 错误恢复的两处栈卫生 bug（J-1）
- 引用块失败时数据栈回卷到进入前深度：此前半成品值残留污染恢复路径
  （`{ 1 2 3 0 / } try` 留下 [1,2]）；现只剩 [#f, 消息]；
- execute_ref_inner 错误出块时弹净本块调用帧：此前残留帧被外层主循环
  继续执行——try 捕获后引用块余下指令泄漏执行（`{ 1 0 / 5 }` 的 5
  仍会跑）；3 项回归测试（含 quote_double 空帧守卫）。

### C 后端堆 commit-on-demand（J-2 第一步）
- VirtualAlloc 从 MEM_COMMIT|MEM_RESERVE（一次性提交 4GB 提交额）改为
  MEM_RESERVE + 64KB 粒度按需提交两个 bump 方向触及的区间；Linux/macOS
  mmap 本就按需换页；soak：2M 次迭代 ~400MB 字符串 2.7s 跑完 rc=0，
  自举固定点不受影响。

### CI macOS 矩阵（J-5 可远程部分）
- test job 增加 ubuntu/macos 双矩阵（fail-fast 关闭）——三平台中唯一
  无法本地验证的平台由 CI 覆盖。

### J-3（strict 默认）与 J-4（模块命名空间）
- 体量大且 J-4 触及冻结语法（需双实现+门禁+重调模型），列入下轮专项；
- Rust 侧（whisper run）为 Rc 生命周期，内存本就回收——J-2 剩余
  （arena 回收策略/阻塞 TLS 并发化）随 daemon 化需求再议。

## Unreleased (2026-09-04d) — 说明文档对齐现状

- README：tests 徽章 392→451、stdlib 徽章 13→14 模块；能力沙箱行补
  HTTPS/host 白名单；置信度行更新为定稿四原语；
- docs/stdlib.md：std/http 表补 http-req(@6)/http-audit(@7) 与
  TLS/chunked/超时语义；
- docs/tutorial.md：HTTP 节补 https 与会话请求示例；新增 Confidence
  小节（std/conf 四原语）；
- docs/packages.md：路线勾掉 Lexicon 拉动包（五件已交付），补 sha256
  为下一项。

## Unreleased (2026-09-04c) — Lexicon 拉动包：jwt/html/csv + resolver 顺序 bug

### resolver 顺序 bug（语言级，I-2 过程中发现）
- 模块的 export 过滤此前把**传递依赖的词也滤掉**（jwt 递归装入
  encoding 后又被 jwt 自己的 export 名单丢弃），且 visited 已标记导致
  消费方再 import 时被跳过——import 顺序决定成败。重构为"传递词原样
  + 自有词按自身 export 过滤"两轨，顺序无关（双向测试锁死）。

### std/encoding 修复
- b64-dec 无填充余组（len%4 ∈ {2,3}）此前丢字节——b64url 无填充
  常态下载荷截断；补余组处理 + 无填充向量入测试。

### 新包（Lexicon 拉动）
- std/jwt：jwt-header/jwt-payload/jwt-claim（数字 claim 转字符串，
  缺键 → ""），VulnOA 真靶令牌验证；Lexicon tests/t_jwt.ws 冒烟闭环；
- std/html：html-text（剥标签 + 六个命名实体解码）/ html-attr
  （仅双引号属性，miss → ""）；
- std/csv：csv-parse（无引号最小集，如实文档化限制）。

### std/hash（md5）：入库（前一轮遗留已解）
- 纯 Whisper 全管线，RFC 向量 + 多块消息（200×a）全对。两处根因：
  I 轮函数应为 C ^ (B | **~D**)（此前漏了取反）；长度字节第 k 位
  移位丢了 8× 系数且顺序倒置（空串全零所以先"蒙对"）。

## Unreleased (2026-09-04b) — 包生态地基 + std/encoding

### 包管理端到端验收（此前从未跑通）
- import 解析补 `$WHISPER_HOME/packages/` 搜索路径（注释与实现不符，
  已装包此前不可解析）；
- `whisper install ./本地目录`：包作者开发流（不下载不删源），manifest
  从本地读，lock 记录来源路径；
- `install` 参数序修复（-y 被当包名）；本地包 → install → import →
  运行 / list / uninstall / 重装全链路冒烟通过；
- docs/packages.md：包标准（manifest/模块约定/B2 合同门/能力审查/
  native 库/路线）。

### std/encoding（样板包，纯 Whisper 零依赖）
- b64-enc/b64-dec/b64url-enc/b64url-dec（RFC 4648 含填充与 URL 安全
  字母表）、pct-enc/pct-dec（保留集 + %XX 大写十六进制）；
- RFC 向量 + 往返 + 字母表分歧用例锁进集成测试
  （encoding_stdlib_vectors）；docs/stdlib.md 补 std/encoding 与
  std/conf 条目。

### 修复过程中沉淀的栈编排铁律（新增三条，均踩实）
- 比较词会消费操作数：`dup 0 >= swap x <=` 吃掉被比较值——须
  `dup 0 >= over x <= &`；
- `$N` pick 从栈顶 0 基计数：`[a b c]` 中 a 是 `$2` 不是 `$0/$1`；
- kv 累积字符串时 `值 kv-get strcat` 是前插（值+旧），`kv-get swap
  strcat` 才是追加。

## Unreleased (2026-09-04) — `]` 歧义解禁、LSP stdlib 感知补全

### `]` 语法歧义（spec §3.3 已知限制解禁）
- 实测解析器已是括号深度感知：列表字面量在条件分支内任意位置
  （分支尾、elif 链、嵌套列表）Rust 链与自举链均正确解析。
  加 7 项回归测试锁定（parser::list_in_cond_tests），spec.md 更新。

### LSP 补全增强
- import 路径补全：光标在 `import "..."` 串内时列出 std/* 模块与
  同目录 .ws（纯函数化 completion_items，可单测）；
- 模块感知：文档 import 了 std/X 后，补全提供该模块导出词
  （解析 stdlib 源的 export 列表）；
- 内建表补 30+ 缺失词（kv-*、map-*、字符串/列表补遗、位运算、
  时间/随机、置信度定稿原语 conf-of/unwrap、http-req）；
  5 项补全测试。

### 词级热编译：立项决策文档化
- docs/hot-word-design.md：机制、四项前置条件、风险、重启触发器；
  维持不动工（当前无 VM 计算瓶颈负载，-t native 缓存已覆盖主要收益）。

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
