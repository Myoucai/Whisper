# 摆脱 Rust / 摆脱 C —— 依赖清算与分层设计

2026-09-21 立项文档。本文先给出**实测的依赖账**（哪个路径依赖什么），
再据此设计分层目标与分步动作。所有结论都有代码出处，不写"大致""应该"。

---

## 一、依赖清算（实测）

### 1.1 两条轴，必须分开看

"依赖 Rust/C" 是两件不同的事，混在一起谈必然失焦：

- **构建期依赖**：装配工具链需要什么（要装 Rust 编译器吗？要装 gcc 吗？）
- **运行期依赖**：产物/执行需要什么（产物里有没有 Rust VM？跑程序要不要解释器？）

### 1.2 逐路径实测

| 路径 | 构建期需要 | 产物内含 | 结论 |
|------|-----------|---------|------|
| `scripts/bootstrap.sh` | **仅 gcc** | 纯原生 | ✅ 零 Rust（已达成） |
| `whisper build -t c2` | Rust(宿主) + gcc | 纯原生 | ⚠️ 构建期有 Rust |
| `whisper build -t native` | Rust(宿主) + gcc | 纯原生 | ⚠️ 构建期有 Rust |
| `whisper build -t exe` | Rust | **Rust VM 本体** | ❌ 产物内含 Rust |
| `whisper run` / `.wbin` | Rust | 需 Rust VM 执行 | ❌ 运行期依赖 Rust |
| `repl` / `lsp` / `fmt` / `check` / 包管理 | Rust | — | ❌ 全部 Rust 实现 |

出处：

- `bootstrap.sh:2-3` 自述 `with gcc as the only dependency`、`zero Rust involvement`（5 步全 gcc）
- `crates/whisper-cli/src/commands/c2.rs:159` `let mut vm = Vm::new();` —— c2 用 **Rust VM 当宿主**
  执行 `compile-full`(:169) 与 `cgen`(:176)
- `crates/whisper-cli/src/commands/build.rs:85-92`：
  ```rust
  let self_exe = std::env::current_exe()...   // ← 复制 whisper.exe 自己
  let mut bundled = self_exe;
  bundled.extend_from_slice(&payload);
  bundled.extend_from_slice(&(payload.len() as u64).to_le_bytes());
  bundled.extend_from_slice(b"WSHPEXE1");
  ```
  **`-t exe` 产出的"自包含可执行文件"就是 Rust CLI 二进制 + 尾部 wbin 负载。**

### 1.3 结论一：Rust 的泄漏点只有三个

1. **`-t exe` 把 Rust VM 复制一份当产物**（最严重：所谓"自包含"其实是"自带 Rust"）
2. **`run` / `.wbin` 必须 Rust VM 才能执行**
3. **`-t c2` 用 Rust VM 当宿主**跑 `compiler.all.ws`

其余（`fmt`/`check`/`lsp`/包管理）是"工具缺失"，不是"架构依赖"。

---

## 二、结论二：C 那层几乎已经没有"C 特有"的东西了

这是本次清算最重要的发现。对 `whisperc/runtime.c`（2027 行）逐项扫描：

| 非标准构造 | 出现次数 |
|-----------|---------|
| `__attribute__` | **0** |
| `typeof` | **0** |
| `__builtin_*` | **0** |
| 语句表达式 `({ ... })` | **0** |
| 内联汇编 `asm(...)` | **0** |
| `__int128` / `__extension__` / `#pragma` | **0** |
| `__thread` | **1** |

`#include` 全部是标准头 + POSIX/Win32（`winsock2.h`/`windows.h`/`dlfcn.h`/
`pthread.h`/`sys/socket.h`，均带 `#ifdef` 分支，`runtime.c:18` 还有
"winsock2.h must precede windows.h" 这类跨平台处理）。

**唯一实质性的非标准依赖是编译标志 `-fno-strict-aliasing`**（规避严格别名
UB），这是可以修的技术债，不是语言绑定。

而且 `WHISPER_CC` 已经存在并生效：

- `commands/c2.rs:217` 与 `commands/native.rs:58` 均 `std::env::var("WHISPER_CC").unwrap_or("gcc")`
- `native.rs:111` 报错文案 `install gcc or set WHISPER_CC`
- `crates/whisper-codegen/tests/c_backend_tests.rs:8` 注释已声明可指向特定编译器

**判断：C 在这个项目里事实上已经是"可移植汇编"，只是没被这样定义、也没被验证过。**

同时注意反面教训：**6 月已经试过"直接生成原生"这条路，并且失败了。**
`crates/whisper-codegen/src/native_gen.rs`（3220 行 x86-64 直编）与
`src/native/`（1105 行）在 2026-08 的 Phase 3 被整体删除，理由是
"10+ 致命缺陷、无执行测试"。重走这条路之前，必须先建对拍框架。

---

## 三、结论三：一个没人注意的硬缺口 —— C 链不能起子进程

> **状态更新（2026-09-21，Step 2a）**：本节描述的缺口**已修复**。
> `runtime.c` 现实现 `W__5`（`@5` exec，popen + stderr 临时文件 +
> 组包裹 + stdin 接空设备）。本节保留原始诊断，因为"为什么必须有
> Rust CLI 当驱动"的推理链条仍然是理解 Step 3 价值的前提。
> 诊断本身也有一处偏差：并非"缺 exec 一个词"，而是**整条 `@n` 能力
> ABI 在 C 链缺失**（详见 Step 2a）。

`runtime.c` 具备的文件/环境原语：

```
w_read_file (L2005)   w_write_file (L2019)   w_listdir (L1175)
w_mkdir (L1167)       w_env_get (L2028)
```

**没有 `system` / `popen` / `execv` / `CreateProcess`，也没有 `w_exec`。**
lexer/parser 里同样没有 `exec` 类关键字。

后果非常具体：**自举编译器能生成 C，但没有任何"自己调 gcc"的能力。**
这正是今天必须有 Rust CLI 当驱动的根本原因——不是 Rust 抢了活，
而是 Whisper 侧缺一只手。

同时这是一处**双链能力不对称**：Rust VM 侧 `ExecCap` 已实现
（`capability.rs`，`--allow-exec`，`run.rs` 绑定 id 5），
C 链当时完全没有对应物。

---

## 四、分层目标

| 层级 | 目标 | 现状 | 值不值得打 |
|------|------|------|-----------|
| L0 | 构建需要 Rust 或 gcc | 当前 | — |
| L1 | **构建零 Rust** | ✅ 已达成（`bootstrap.sh` 仅 gcc） | — |
| L2 | **产物零 Rust** | ✅ **已达成**（2026-09-21：`-t exe` 改走 C 链；原 Rust 打包行为更名 `-t bundle`） | — |
| L3 | **工具链零 Rust**（run/repl/fmt/pkg/check） | ⚠️ **主体已达成**：`bootstrap.sh` → `wd` 一条命令零 Rust 编+跑任何程序（2026-09-21）；**repl/fmt/pkg/check/LSP 仍是 Rust** | 剩余部分值得，是独立战役 |
| L4 | **C 降级为可替换接口** | ✅ **已达成**（`-fno-strict-aliasing` 已删；`WHISPER_CC` 可换编译器；CI 有 clang 腿） | — |
| L5 | 零 C 编译器（发射汇编） | ❌ | ⚠️ 性价比低，暂不做 |
| L6 | 零外部工具（直出机器码） | ❌ 已失败过一次 | ❌ 不建议 |

> **L3 的"主体"是什么**：L3 原本列的是 run/repl/fmt/pkg/check 五件事。其中
> **run** 是唯一一件"不做就没有独立可用性"的——`wd` 把它做掉了（编译 + 执行 +
> import 内联 + 权限交给 C 运行时）。repl/fmt/pkg/check/LSP 是"工具缺失"，
> 不影响"这台机器上不装 Rust 也能写、能编、能跑 Whisper 程序"这个结论。
> 见 CHANGELOG 2026-09-21o 与 `tests/closeout_preregistered.md`。

**核心判断：L2/L3/L4 是一个清晰可完成的项目；L5/L6 是另一个量级的工程，
且 L6 已被证伪。而 L1 其实早就达成了——这一点被严重低估。**

---

## 五、分步动作

### Step 1 —— 修 `-t exe` 的架构矛盾 ✅ 已完成（2026-09-21）

**先修正本文档初稿的一处错误**：初稿推荐把 `exe` 的 payload 换成 C 链产物。
但 `-t c2` 本来就产出原生可执行文件——那样改会让 `exe` 与 `c2` 完全重合，
而 `exe` 原本唯一的差异点（**构建期不需要任何工具链**）会静默消失。
真正的结论是：这是**两种分发模型**，不该共用一个名字。

**最终决策（用户裁定，方案 A）**：

| 目标 | 语义 | 构建期需要 | 产物内含 |
|------|------|-----------|---------|
| `-t exe` | 自举链 → 原生机器码 | gcc | **零 Rust** |
| `-t bundle` | Rust VM + wbin 尾部打包 | 无 | **整个 Rust VM** |

**实现**：

- `build.rs`：`exe` 分支改为 `super::c2::build_c2(...)`；旧逻辑整体搬到
  新增的 `bundle` 分支，输出文案如实标注 `embeds the Rust VM`。
- `main.rs`：启动期的 `WSHPEXE1` trailer 检测保留（bundle 产物仍需），
  注释与 `--target` 帮助文本同步。
- **顺带修一处既有瑕疵**：mingw gcc 会把 `-o app` 自行变成 `app.exe`，
  而 CLI 此前回显的是**请求路径**——现解析实际存在的文件再报告
  （影响 c2 / native / exe）。

**验收（已达成）**：

- 新增 `exe_target_is_native_and_carries_no_rust`：断言产物可运行、
  **不含** `WSHPEXE1` trailer、且确实经过 C 链（同级 `.c` 存在）。
- 原 `bundled_exe_*` 两个测试迁移为 `bundle_target_*`。
- `cargo test --workspace` **469/0**。

**未做**：真实静态链接。`-static` 在 mingw 上是**空操作**（实测加与不加
哈希完全相同，仍导入 `msvcrt.dll`），拿它当"自包含"卖点会是假承诺。
若将来要做，须按平台单独验证，而不是加个标志就宣称。

### Step 1.5 —— P0 回归：递归守卫阈值把 c2 链打断了 ✅ 已完成

**这是 Step 1 验收过程中意外发现的、比 Step 1 本身严重得多的问题。**

`eec695f`（HEAD）为 `: f { f } ; f` 的 30 GB 分配加了 2048 帧词调用上限。
但**自举编译器是"用递归代替循环"遍历 token 的**，所需深度 ≈ token 数：

| 输入 | 体积 | 所需帧数 |
|------|------|---------|
| `stdlib/math.ws` | 1.9 KB | 2 049 – 4 096 |
| `whisperc/compiler.all.ws` | 65 KB | 16 385 – 32 768 |

后果：`-t c2` 对**任何 import stdlib 模块的程序**报 `RecursionLimit`，
连编译器自己都编不过——而 `opcode_coverage.sh` 正是这样构建自举编译器的。
**五道门禁全部受影响**（stdlib / syntax / app / net / opcode 覆盖）。

之所以漏过：该提交的回归记录只写了 cargo 与固定点，而**固定点走 C 种子链，
根本不经过 Rust VM 的守卫**。

**修法**：默认上限 2048 → **262 144**（对最大已知工作量留 8× 余量；
48 字节/帧 ⇒ 最坏约 25 MB，而原 bug 是 30 GB）；新增
`WHISPER_MAX_CALL_DEPTH` 逃生口（诊断用，非安全边界）。

**教训（值得写进工程惯例）**：**守卫类改动必须跑全部 shell 门禁，
不能只跑 cargo + 固定点。** cargo 测试里没有任何用例经 c2 链编译一个
stdlib 规模的输入——这正是回归测试要补的洞，现已补上
（`c2_chain_compiles_stdlib_imports`）。

### Step 2a —— C 链能力 ABI（`@0/@1/@4/@5`）✅ 已完成（2026-09-21）

**背景更正**：`exec` 不是缺词——它是能力令牌 `@5`（`stdlib/os.ws`），
VM 侧一直可用。真正的洞是 **C 链不支持 `@n` 令牌**：cgen 把 `@n` 当普通
词引用，发射 `W__<n>()`（`compiler.all.ws:1293`），而这些函数从不生成。
后果远超"缺 exec"——`import "std/os"` 经 c2 **根本编不过**。长期未发现
的原因是**六道 c2 门禁里没有任何用例碰过能力令牌**。

**接线方式（未动冻结文件）**：在 `runtime.c` 里实现 cgen 既成的
`W__<n>()` ABI。命名难看（`W__4` 是 classify 把 `@4` 误判为词引用的
产物），但它是既成契约；而改 `compiler.all.ws` 必须连带重生
`seed/compiler_seed.c` 并重跑整条自举链，代价与风险都远大于收益。

| id | 能力 | 实现 |
|----|------|------|
| 0 | file read | `w_read_file` 接线 |
| 1 | file write | `w_write_file` 接线（返回字节数） |
| 4 | env | `w_env_get` 接线 |
| 5 | exec | **新增**：popen + stderr 重定向到临时文件 + 组包裹 + stdin 接空设备 |

`@5` 的三个细节都是对齐 VM 的 `ExecCap` 所必需：popen 只能捕获单流
（故 stderr 走临时文件）；命令须包在 `( ... )` 组里，重定向才覆盖整条
命令而非最后一节；stdin 必须接空设备，否则读 stdin 的命令会永久挂住
VM（Rust 的 `Command::output()` 默认就是这样）。

**顺带修掉两处 VM 缺陷**（对拍时暴露，都属于"双链不对称"）：

1. **`Opcode::CapCall` 恒弹一个操作数** —— 而 `FileWriteCap`/`HttpPostCap`
   需要两个，所以 `@1` 在 VM 里从来不可用。新增 `Capability::arity()`
   （默认 1，`FileWriteCap` 覆写为 2），`CapCall` 按其弹取并反序。
   HTTP 能力 arity 保持默认，行为不变（属 Step 2b）。
2. **`FileWriteCap` 不压任何结果** —— 静默丢弃失败，且与原生
   `write-file` opcode、C 链、类型模型三者都不一致。现返回字节数。

**第三处修复（类型层）**：`checker.rs` 把 `CapCall` 归入 `_ => {}`，
strict 默认下 `"cmd" exec 1 @nth .` 被误拒。现为 0/1/4/5 建模栈效应
（`@5` 异构结果用既有的 `Type::Union`），与 J-3 修的 `http-*` 签名缺失同类。

**验收（已达成）**：

- 双链对拍：`@0/@1/@4/@5` 五组用例 VM 与 c2 输出逐行一致（含非零退出码、
  独立 stderr 捕获）；`tools/opcode_coverage.sh` **52 → 56 例**，0 fail。
- Rust 集成测试 `capability_abi_matches_across_chains`（覆盖文件对）。
- 全门禁：cargo **470/0**、syntax 26/0、stdlib 58/0、typecheck FP=0、
  覆盖 56/0/0、net PASS、app PASS(13)、固定点 gen2==gen3。

**遗留（Step 2b）**：`@2`/`@3` 于 2026-09-21 补上 C 侧实现（见下节），
`@6`/`@7`/`@8`/`@9` 仍未实现但会**显式报错**。`path_is_allowed` 的
"相对路径永远被拒"一度记在这里待办——**收口战役核对时发现代码早已修好**
（`capability.rs:129` 起：相对路径先 join CWD 再归一化，注释里写明就是为修它），
是把旧描述留在文档里没跟上。已改正，勿再立项。

### Step 2b —— HTTP 能力上 C 链（**部分完成 2026-09-21**）

> **状态**：`@2`（http-get）与 `@3`（http-post）**已在 C 链实现并双链对拍**
> （body 与线上请求字节两个层面），判据见 `tests/step2b_preregistered.md`。
> `@6`/`@7`/`@8`/`@9`（会话/审计/raw/并行）仍未实现，但**调用时显式报错**，
> 不再把失败推迟到 gcc 的 `implicit declaration`。

**量级修正**：原文把 Step 2b 估为"需要一个 HTTP + TLS 客户端……中等工程"。
本轮核实发现该估算基于一个未核实的前提——**VM 侧的 `@2/@3` 本身就不支持
HTTPS**（`capability.rs::http_request` 直接拒绝 `https://`），实现只有约 80 行。
真正走 TLS 的是 `http_session.rs`（`@6` 家族），`tls.rs` 里的 rustls 只服务它。
因此"与 VM 对齐"的 `@2/@3` 实现**不含 TLS**，量级降到"补一个 HTTP/1.1 最小
客户端"（`runtime.c` 约 190 行，含此前缺失的客户端 `connect`）。

**已做**：

1. `runtime.c`：`w_url_parse` / `w_tcp_connect` / `w_http_request` /
   `w_http_read_all` + `W__2` / `W__3`，请求行、`Host`、`Connection: close`、
   POST 头与 body 提取规则与 VM 逐字节一致。
2. `W__6`-`W__9`：定义为 `w_cap_unimplemented()` → `w_fail()`，调用时打印
   `capability @N (…) is not implemented on the C chain; it is VM-only for now`
   并退出（可被 `try` 捕获）。**不用编译期 `#error`**：c2 会文本内联
   `std/http.ws`，编译期报错会把只用 `@2` 的程序一起打死。
3. HTTPS 拒绝语义两链一致，且有测试断言**拒绝发生在建连之前**。
4. 顺带修掉既有 bug：`HttpPostCap` 漏覆写 `arity()`（默认 1，实际要 2），
   导致 `@3` 在 VM 上怎么用都失败。

**仍未做**：`@6`/`@7`/`@8`/`@9` 的 C 侧实现（会话 cookie 罐、审计日志、raw
通道、并行会话）。它们共用同一套 HTTP 核心，补实现时只需在其上加状态。
TLS 缺口是两链**共同的**现状，不是 C 链独有的。

**另一处曾被记为待办、实为已修**：`path_is_allowed` 的相对路径问题——
**代码早已修好**（相对路径先 join CWD 再归一化，`capability.rs:129`），
只是这条描述没跟着改。收口战役核对时改正，勿再立项。

**C 链的宿主白名单**：VM 侧 `--allow-http` 有 `allowed_hosts`，C 链对
`@0/@1/@5` 本来也没有白名单。**这是刻意的，不是漏掉的**：独立 C 二进制没有
命令行，白名单只能靠环境变量，那与 VM 的 `--allow-http` 不是同一种保证——
"设了环境变量所以安全"是一种看起来安全的假象。要么整套做（C 侧自己的
策略文件 + 与 VM 同语义的拒绝路径），要么不做，不做半套。

### Step 3 —— Whisper 写驱动 `wd`（**2026-09-21 完成**）

> **状态**：`whisperc/wd.ws` + `scripts/build-wd.sh` 已落地，判据见
> `tests/step3_preregistered.md`。**零 Rust 构建**、**编译+运行程序**、
> **import 展开（含自动 `std/core` 与命名空间限定克隆）** 均已通过。
> **A3 的逐字节残差已清零**（收口战役 W1+W2，2026-09-21o）：自包含程序
> 1601 == 1601，带 import 的 17875 == 17875、17228 == 17228、22569 == 22569。
> 残差的两个来源分别是 **Rust lexer 未折叠 CRLF** 与 **`wd` 不克隆多行定义**，
> 与"`wd` 生成得不对"无关。
> **内容寻址缓存已落地**（收口 ③，2026-10-20）：C-1..C-6 的八条断言全绿
> （`bash scripts/wd-cache-gate.sh`），命中 801ms → 148ms。

目标：**gcc → whisper2 → wd → 跑任何 Whisper 程序，全链路零 Rust。**

`wd` 是纯 Whisper 程序，由 whisper2 编译成原生二进制，负责：

```
读源文件 → 内联 import → compile-full → cgen → 写 .c
        → run-cmd 调 $WHISPER_CC → 执行产物（缓存命中则直接 exec）
```

**已实现**：读源文件 → import 展开（与 `c2.rs::expand_imports` 同构，含
递归与防环）→ `compile-full` → `cgen` → 写 `.c` → `@5` 调 C 编译器 → 可选
执行产物并转发输出。`WHISPER_CC` / `WHISPER_ROOT` 两个环境变量。

**已实现（2026-10-20）**：**内容寻址缓存**。键 = hash(生成的 C ++
`runtime.c` 内容 ++ cc 名)，缓存位于 `WHISPER_ROOT/target/whisper-cache/<key>`；
命中即把条目拷到 `<out>[.exe]` 并直接执行，未命中才调 C 编译器，成功后才回填。
改源码、改运行时、换 cc 都会自然失效；gcc 失败不写缓存。判据 C-1..C-6 见
`tests/wd_cache_preregistered.md`，门禁 `scripts/wd-cache-gate.sh` 八条全绿。

**已收口**：**多行定义的限定名克隆**（收口战役 W2）——它曾是 `wd` 与
`-t c2` 的 C 输出残差的来源，现在四个程序（自包含 / `std/math` / `std/list` /
多行限定名）**逐字节相同**，由集成测试 `w2_wd_output_matches_c2_byte_for_byte`
常驻把关，跑的是零 Rust 链产出的 `target/wd/wd`。

**顺带发现的两个语言侧缺口**（`wd` 的体验因此打折，**已立项为战役 L-1 并于
2026-09-21 实施完成**，判据与执行记录见 `tests/l1_preregistered.md`）：
**C 链程序没有退出码**（cgen 固定 `return 0;`，Whisper 层没有 `exit`/`quit`
词——于是 `wd` 的错误路径无法让 shell 看到非零退出码）；**没有裸打印词**
（打印只有自带换行的 `.`，而 `striter` 是迭代器——于是 `wd run` 转发的输出
末尾会多一个换行）。**现状**：`exit`（opcode `0x94`）与 `print`（`0x93`）
已两链落地，`wd` 的失败路径现在给出 1（构建/IO 错）与 2（用法错），转发改用
`print`。**曾经的残留**（转发仍差子进程尾部空白，因为在能力 `@5` 里被
`trim()` 掉了）已由收口战役 W6 关闭：新增兄弟能力 `@10` = `exec-raw`
（同实现、同授权、两个流原样返回），`wd run` 的转发段与直接运行产物现在
**逐字节相同**。详见 `docs/language-backlog.md` 的 W-5（已关闭）。

**验收**：仅装 gcc 的干净机器上，`bootstrap.sh` → `whisper2` → 编出 `wd`
→ `wd run hello.ws` 输出 `"Hello, World!"`。全程无 Rust。
（`bootstrap.sh` 的 6/7 步已经在编 `wd`、7/7 步在冒烟跑它；2026-09-21
实测**七步全过、EXIT=0**——此前它因为本机缺 `cmp` 假报固定点破裂，从未跑到
第 5 步，见收口战役 W3。）

缓存策略与 `-t native` 同源，但键取**生成的 C** 而不是"展开后源码"：
键 = hash(生成的 C ++ `runtime.c` 内容 ++ cc 名)。这样改了 `wd` 自身也不会
吐出旧产物（C 变了，键就变了），不需要额外维护"编译器版本"这个脆弱不变式；
`runtime.c` 必须显式入键，因为它以 `#include` 形式被引用、不在 C 文本里。

**注意性能现实**：`run` 从"解释"变成"编译+缓存"后，首次调用有编译器
延迟（数百 ms 量级），缓存命中后才回到 ms 级。REPL 交互会有感知，
因此**保留快速路径选项**（见 Step 5）。

### Step 4 —— 工具链迁移（机械工程）

> **状态（2026-10-20）**：`fmt` 第一片已落地——`whisperc/fmt.ws`，
> 零 Rust 的 `wd` 构建，门禁 `scripts/fmt-gate.sh`（F-1..F-6）。
> v1 只做布局规范化（保留注释/字符串），AST 重排留 v2。
> 包管理/REPL/check/LSP 尚未开始。

按难度递增：

| 工具 | 难度 | 说明 |
|------|------|------|
| `fmt` | 低 | ✅ v1（布局规范化）；v2 AST printer |
| 包管理 | 低 | `read-file`/`write-file`/`mkdir` + HTTP + MD5 够用 |
| `repl` | 低 | `read-line` + Step 3 的编译路径 |
| `check` | **高** | 需把 typecheck 逻辑用 Whisper 重写（见下） |
| `lsp` | 高 | 协议层机械（std/json + std/net 够），但增量分析难 |

**typecheck 是最后一块实质的 Rust 逻辑，且它非常适合用 Whisper 写**
——本质是纯函数（AST → 诊断集），无 IO 无状态。但它也是
`docs/typecheck-strict-design.md` 记录的四阶段战役的成果（FP=0 棘轮），
重写必须**先搬判据再搬代码**，否则会丢掉"假阳性为零"这个来之不易的性质。

### Step 5 ——（可选）Whisper 写字节码 VM

**先澄清一个常见误解：写 VM 不是摆脱 Rust 的必要条件。**
Step 3 的驱动路线已经能达成 L3，且不需要任何语言新特性。

> **安全提醒（2026-10-20）**：VM **不是**当前的安全边界。`read-file`/
> `write-file`/`getenv`/`ffi-*` 等 ambient opcode 绕过 `CapabilityTable`；
> 原生 C 产物也没有语言级授权。不可信执行的路线是 WASM/WASI（Step 7），
> 不是“再写一个 VM”。详见 `docs/security-model.md`。

Whisper VM 只在两个场景有独立价值：

1. **无 gcc 的 `run` 快速路径**（Step 3 的编译延迟在 REPL 里可感知）
2. **元循环验证**：Whisper VM 跑 `compiler.all.ws`，接进现有双链门禁做
   三方对拍——很硬的正确定性证据

前置是两个语言原语（详见对话记录中的分析）：

- **`call`：一等引号的动态调用**。VM 侧代码路径已存在
  （`vm.rs:1554` `CapExec` 的 `Value::Ref(code) => execute_ref(code)` 分支），
  但**无表面语法**（全仓库搜 `cap.exec` 只有 `opcode.rs` 的名字与
  转译器里一句 `// cap_exec: no-op in native mode`）。除 `CapExec` 外，
  其余 8 处 `execute_ref` 全部是编译期已知引号（Filter 292、Map/Each/Fold
  1003/1018/1033、Try 1244、Times 1499、Cond 1608/1610）。
- **O(1) 可变随机写**。`map-set` 在 VM 侧整表克隆（`vm.rs:1273`
  `(*map).clone()`）、C 链是排序关联表插入，均为 O(n)/次；无 `list-set`。
  拿 map 当操作数栈 = 平方复杂度解释器。

**性能预期要诚实**：这是"解释器上叠解释器"。参考现有数据
（c2 原生 6.1ms / Rust VM 26ms），大概率是数百倍量级。
定位只能是正确性产物，不是生产运行时。

### Step 6 —— 关于"摆脱 C"的路线选择 ✅ 已完成（2026-09-21，路线 A）

**推荐路线 A：把 C 从"依赖"降级为"接口"**（已执行）

理由：第二节实测显示 C 层几乎没有 C 特有构造。与其花大力气消灭它，
不如把它定义清楚并验证：

1. ✅ **锁定发射目标为 C99 + POSIX/Win32**，写入 `docs/spec.md` §15
   （连同 5 条禁止项），从此违反即为**规范级缺陷**而非风格问题。
2. ✅ **`W_TLS` 条件宏**：MSVC → `__declspec(thread)`；C11 →
   `_Thread_local`；其余回退 `__thread`。原来无条件用 `__thread`，
   等于悄悄把运行时钉在 GCC/Clang 上。
3. ✅ **消除 `-fno-strict-aliasing` 依赖**——这是本次最实质的一步，
   做法是先证明再删，不是"试着删掉看看"：
   - **静态**：全文扫描无类型双关（`*(double*)&x` 类 0 处、无 union），
     位模式转换全部走 `memcpy`（`W_F64`/`w_as_f64`）。
     `w_str_new` 的 `*(uint32_t*)p` 合法——arena 存储无声明类型，
     首次写入即确定有效类型。
   - **动态**：取最重的工作量（`compiler.all.ws`，65 KB → 205 KB C），
     分别带/不带该标志编译出两个自举编译器，让它们编译同一程序，
     输出**逐字节一致**（SHA256 相同）。
   - 之后从 9 个构建路径全部移除（CLI 两个目标、4 个脚本、2 个测试、
     README 示例），并 bump `-t native` 的内容寻址缓存键版本
     （`-O3-flto-v2`），确保旧缓存条目不会被误用。
   - 删它的**决定性理由不是性能**：MSVC 根本没有这个标志，它是
     "C 可替换"的直接阻碍。
4. ⚠️ **编译器矩阵**：CI 新增 `c-backend-clang` 腿（`WHISPER_CC=clang`
   + `GCC=clang`，跑 C 后端测试、stdlib 门禁、覆盖门禁）。
   gcc 腿本机实测；clang 腿**只能由 CI 验证**（本机无 clang）。
   tcc / msvc 未纳入——msvc 需要的不止是标志（`unistd.h`/`popen`
   命名等），属另立项。
5. ✅ **表述**：C 现在是 Whisper 的**可移植汇编**，可用 `WHISPER_CC`
   指定任意 C 编译器。

**残留风险（如实记录）**：删 `-fno-strict-aliasing` 的依据是
"静态审计干净 + 最重工作量逐字节一致"。**通过测试不等于证明无 UB**——
若将来某个编译器/优化级别上出现异常，正确的反应是把标志加回来并
记录触发条件，而不是继续猜。

**不推荐路线 B：重写原生后端。** 6 月失败过一次（3220 行、10+ 缺陷、
无执行测试被删）。若将来真要做，**第一件事是建对拍框架**（生成 → 执行 →
与 VM 比对输出），而不是写 codegen——没有执行测试的 codegen 必然烂掉。

**路线 C（仅在未来真需要零 C 编译器时考虑）：把目标从 C 换成汇编。**
只需 `as` + `ld`（binutils），依赖面比 C 编译器更小。但要清醒：
比起"编码指令"，**指令选择与寄存器分配才是 80% 的工作量**，
且每个架构都要重做。这条路并不比路线 B 便宜多少。

---

### Step 7 —— WASM/WASI 安全执行目标（2026-10-20 立项，计划）

**动因**：C 链是 trusted-only；VM 的 `@n` 表又被 ambient opcode 与 FFI
绕过（实测 `read-file`/`getenv`/`ffi-open` 均无授权可用）。在“不改 Rust”
的约束下，原生安全的主路线是 **C → WebAssembly + WASI**，而不是再写一个
VM。

**架构**：`source.ws → C → clang --target=wasm32-wasip1 → .wasm`，
由 WAMR/wasmtime 执行；文件只通过 preopen，网络/执行/FFI 走宿主策略或
显式拒绝。Rust 代码不改；`-t wasm` 先由 `scripts/build-wasm.sh` 用现成的
`whisper bootstrap` 出 C，后续再给 `wd` 加 `c`/`wasm` 动词。

**明确不做**：v1 不支持 `@5`/`@10`/FFI/裸 socket/`serve-concurrent`；
这些必须显式报错，不能 no-op。native `exe`/`c2`/`native` 保持
trusted-only。

**判据**：`tests/wasm_preregistered.md`；计划与风险：
`docs/wasm-wasi-plan.md`；威胁模型：`docs/security-model.md`。
**工具链探测**：`scripts/wasm-preflight.sh`（已落地，本机当前缺
clang/wasi-sysroot/runtime）。

**状态**：文档/判据/preflight 已落地，WASM 代码未开始。

---

## 六、执行顺序与工作量

```
Step 1   (exe 修正)     半天      ✅ 已完成 2026-09-21
Step 1.5 (P0 递归守卫)  半天      ✅ 已完成 2026-09-21（意外发现，计划外）
Step 2a  (能力 ABI)     1 天      ✅ 已完成 2026-09-21（范围比原估大：不是"加一个词"）
Step 2b  (@2/@3 上 C 链) 半天      ✅ 已完成 2026-09-21（量级修正：不含 TLS，见该节）
                                  ⏳ @6/@7/@8/@9 仍未实现，但已改为显式报错
Step 3   (驱动 wd)      3-5 天    ✅ 已完成 2026-09-21（里程碑 A/B + L-1 + A3 逐字节一致）
                                  ✅ 内容寻址缓存 2026-10-20（C-1..C-6 八条全绿）
Step 4   (工具迁移)     1-2 周    🔄 fmt v1 已落地（2026-10-20，布局规范化 + 门禁）；其余未开始
Step 5   (Whisper VM)   ?         ← 可选，前置是 call + 可变数组；**不是安全方案**（见 Step 7）
Step 6   (C 降级为接口) 数天      ✅ 已完成 2026-09-21
Step 7   (WASM/WASI)    4-6 周    🚧 计划 2026-10-20（文档/判据已落地，代码未开始）
```

**门禁基线（2026-09-21 收口战役后，本机实测，gcc = msys64 mingw 16.1.0）**：
cargo 490/0 · syntax 26/0 · stdlib 58/0 · typecheck FP=0/TP=23 ·
opcode 覆盖 90/0/0 · net PASS · app PASS(13 路由) · 固定点 gen2==gen3 ·
`bootstrap.sh` 7/7 · `wd` 与 `-t c2` 的 C 输出逐字节一致。

**如果只做三件事**：Step 1（消 Rust 泄漏）+ Step 2（补 exec）+
Step 6（C 降级为接口）。这三步成本最低、收益最确定，
且不需要任何语言层面的新特性。**这三件事现已全部完成。**

**Step 3 是真正的分水岭**——它一旦完成，"摆脱 Rust"就从愿景变成事实。

---

## 七、明确不做

- 直接生成原生机器码（L6）：已被证伪，重做前须先有对拍框架
- 用 LLVM 替代 C：把"依赖一个 C 编译器"换成"依赖整个 LLVM"，
  与独立性目标相反，且体积暴涨
- 为摆脱 C 而把 `runtime.c`（2027 行、171 个 `w_*`）重写一遍：
  收益不成立，C 层已是可移植汇编

---

## 附：本文结论所依据的关键出处

| 结论 | 出处 |
|------|------|
| bootstrap 仅需 gcc | `scripts/bootstrap.sh:2-3, 27-56` |
| c2 用 Rust VM 当宿主 | `commands/c2.rs:16, 159, 169, 176` |
| exe 产物 = Rust 二进制 + 负载 | `commands/build.rs:85-92`、`main.rs:120-124` |
| C 链无进程派生 | `runtime.c` 全文无 `system/popen/execv/CreateProcess` |
| VM 侧 ExecCap 存在 | `capability.rs:474`、`run.rs:258-259` |
| `WHISPER_CC` 可换编译器 | `c2.rs:217`、`native.rs:58, 111` |
| native 内容寻址缓存 | `native.rs:2-8, 57-80` |
| C 层无可移植性障碍 | `runtime.c` 扩展扫描：仅 `__thread` 1 处 |
| `CapExec` 无表面语法 | `opcode.rs:736`、`transpiler.rs:769` |
| `map-set` 为 O(n) | `vm.rs:1273` |
| 原生直编已被删除 | CHANGELOG 2026-08-21「移除」节 |
