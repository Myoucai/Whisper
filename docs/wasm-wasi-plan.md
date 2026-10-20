# WASM/WASI 原生安全整改计划（2026-10-20）

## 0. 决策与边界

**决定**：以 WebAssembly + WASI 作为“不可信代码的原生安全执行目标”。
**约束**：不改 Rust 代码（`crates/**` 不动）；只改 `whisperc/runtime.c`、
`whisperc/*.ws`、构建脚本、宿主启动器、CI 和文档。

**不做**：

- 不先重写一个 Whisper VM 来“顺便解决安全”（见 `docs/independence-design.md`
  Step 5；那属于独立性目标，不是本计划的替代品）。
- 不把 native `exe`/`c2`/`native` 说成沙箱；它们保持 trusted-only。
- 不在 WASM v1 里假装支持 `@5`/FFI/裸 socket；不支持就显式报错。

**依据**：`docs/security-model.md` 的审计结果——当前 VM 的 ambient opcode
与 FFI 绕过 `CapabilityTable`，C 链完全没有授权层。

### 0.1 当前进展（2026-10-21）

- **M1 已打通**：`hello.ws → wd c → Zig cc → .wasm → WAMR iwasm`，
  输出 `Hello, World!`。运行链路零 Rust（Zig 是 C 工具链，WAMR 是 C）。
- `wd c` 只发射 C、不调 cc、不碰缓存；与 `whisper bootstrap` 的 C
  输出逐字节一致（hello SHA256 相同）。
- core conformance：**26/27 通过，1 个按原语义跳过**（`26_file_io`
  用宿主 `D:/tmp` 绝对路径）。新增 WASI 版文件 IO 用例，在
  `--map-dir` preopen 下通过；无 preopen 时写入返回 `-1`、读取为空。
- 发现的硬阻塞：cgen 为 quote/HOF 生成 **GNU C 嵌套函数**
  （`auto void _qb`），clang/Zig 不支持。已用
  `scripts/lift-nested.py` 做构建期提升（W2.5），全部 HOF 用例通过；
  永久修法仍是 cgen 直接发射顶层 `_qN()`。
- 安全门禁 `scripts/wasm-security-gate.sh`：**S-1..S-12 12/12**（文件
  默认拒绝/preopen、目录操作、env allowlist、`@2`/`@5`/`try` 显式失败、
  路径逃逸拒绝、FFI 构建期失败、无 Rust 运行时）。
- 宿主启动器 `scripts/wrun.sh`：W-1..W-7 7/7；默认无 preopen/env，
  `--dir`/`--env` 显式放行，`--http-allow`/`--exec`/`--ffi` 显式拒绝。
- 未完成：路径逃逸/symlink、FFI 链接失败、未知 host import、
  `-t wasm` CLI、W-9 性能基线。

---

## 1. 目标架构

```text
source.ws
   │  wd c（零 Rust 发射器）或 whisper bootstrap（Phase 1 回退）
   ▼
app.c  ── clang / zig cc --target=wasm32-wasip1 ──┐
   │                                      │
   │  runtime_core.c                      │
   │  runtime_wasi.c  ────────────────────┤
   ▼                                      ▼
app.wasm  ── wrun policy ──>  WAMR iwasm / wasm3 / wasm2c
                              （全部 C 实现，零 Rust）
                              │
                              ├─ preopen dirs（只读/读写）
                              ├─ env allowlist
                              ├─ host http proxy（可选）
                              ├─ host exec（仅 trusted profile）
                              └─ 无 FFI、无裸 socket
```

关键点：

1. **语言不变**：仍然写 `.ws`，编译链仍然是 Whisper → C → 目标；
   WASM 只是新的 `-t` 目标。
2. **能力在宿主**：`.wasm` 自己不能声明/放宽权限；preopen、env、网络策略
   都由 `wrun`/宿主运行时给。
3. **默认拒绝**：无 policy 时没有文件、网络、环境、执行、FFI。
4. **不支持即报错**：WASM profile 不支持的词要么编译期缺失，要么运行期
   `unsupported on wasm profile`，不能静默 no-op。

---

## 2. 现状与差距

| 项 | 现状 | WASM 目标需要 |
|----|------|---------------|
| 运行时 | `runtime.c` 2500 行，POSIX/Win32 混写 | 拆 core / host / native / wasi 四层 |
| 构建 | `whisper bootstrap` 输出 C；`wd` 调 gcc | 新增 clang wasm 构建；Rust CLI 不改 |
| 文件 | `fopen` 绝对路径 | WASI preopen；路径规范化；只访问授权目录 |
| 网络 | 直接 socket（`@2/@3`、listen/accept） | v1 无裸 socket；host 代理/白名单 |
| 执行 | `popen`（`@5/@10`） | v1 不支持；trusted profile 由 host exec |
| FFI | `dlopen`/`LoadLibrary` | v1 不支持（WASM 无 dlopen） |
| 环境 | `getenv` 直读 | env allowlist |
| 线程 | `serve-concurrent`、TLS | v1 单线程；无 serve-concurrent |
| 测试 | 无负向安全用例 | 每个 effect 的“拒绝”用例进 CI |
| 文档 | 能力表述过宽 | 按执行路径分档（本计划 + security-model） |

---

## 3. 工作包

### W0 — 安全模型与文档（本次，已完成）

- `docs/security-model.md`：威胁模型、边界、缺口、不变量。
- 本文件：整改计划。
- `tests/wasm_preregistered.md`：判据先写死。
- README/spec/stdlib/tutorial/backlog 同步。

**验收**：文档中的每个安全承诺都能对应到“实现 + 测试”或明确标注“planned”。

### W1 — 工具链 preflight（0.5–1 天）

- 确认 C→wasm 编译器：`wasi-sdk` 的 clang（+ wasi-libc）或 **Zig
  `zig cc`**（自带 wasi-libc）；目标 triple：`wasm32-wasip1`（新）或
  `wasm32-wasi`（旧，按工具链版本）。
- 确认运行时：**只接受 C 实现**——WAMR `iwasm`（推荐，官方有
  Windows/Linux 预编译）、wasm3，或 `wasm2c`（WABT，AOT 到 C + 宿主 shim）。
  **不接受 wasmtime/wasmer（Rust 实现）**。
- `scripts/wasm-preflight.sh` 已落地；本机把 Zig 0.17 与 WAMR 2.4.5
  放进 PATH 后 `--strict` **通过**（wasmtime/wasmer 不进入链路）。
  脚本默认只报告、不阻塞 CI；`--strict` 供 CI 使用。
- 记录 AOT 可行性（WAMR `wamrc`，C++ 工具）作为后续选项。

**验收**：preflight 能在一台干净机器上准确报告缺什么；CI 镜像可复现；
最终运行链路中没有任何 Rust 运行时。

### W2 — 运行时分层与 WASI 移植（5–8 天）

把 `runtime.c` 拆成：

```text
runtime_core.c     // 值、arena、字符串、列表、map、JSON、regex、pack、conf
runtime_host.h     // 所有宿主效果接口
runtime_native.c   // 现有 POSIX/Win32 实现
runtime_wasi.c     // WASI Preview 1 实现
runtime.c          // 聚合入口（按 __wasi__ 选择）
```

宿主接口（示意）：

```c
w_host_read_file(path) / w_host_write_file(path, data)
w_host_listdir(path) / w_host_mkdir(path) / w_host_exists(path)
w_host_env(name)
w_host_exec(cmd)            // native only
w_host_http_get/post(url)   // native only; WASM 走 host import
w_host_listen/accept/...    // native only
w_host_ffi_*                // native only
```

WASI 实现：

- 文件：`path_open`/`fd_read`/`fd_write`/`fd_readdir`/`path_create_directory`；
  只允许 preopen 内路径；`..`/绝对路径/symlink 逃逸拒绝。
- 时钟：`clock_time_get`；`sleep` 用 `poll_oneoff`；`strftime` 时区限制要写清。
- 随机：`random_get`。
- 环境：`environ_get`（只拿 `wrun --env` 允许的）。
- `args`/`exit`/stdout/stderr：`args_get`/`proc_exit`/`fd_write`。
- `try`：确认 wasi-libc 的 `setjmp/longjmp` 可用；不可用则改用显式错误传播
  （这会扩大 W2 工作量，必须在 M1 前验证）。

**验收**：`hello.ws`、算术、字符串、列表、JSON 在 wasm 上与 c2 输出一致；
`read-file` 只能读 preopen。

### W2.5 — 把 quote 函数提升到顶层（硬前置，1–2 周）

**事实**：cgen 对 `@map`/`@each`/`@fold`/`@times`/`@filter`/`try`
生成的 C 是

```c
{ auto void _qb(void) { ... } ... _qb(); ... }
```

这是 **GNU C 嵌套函数扩展**，GCC 能编译，clang/Zig/wasm32-wasi 不能。
这不是“WASI 缺功能”，而是 C 发射产品本身不可移植；spec 的
“C 当作可移植汇编”表述因此在 HOF 上不成立。

**临时桥（已落地）**：`scripts/lift-nested.py` 在构建期把每个
`_qb` 提升为文件级 `static void _qN(void)`，并替换调用点。
26/26 可运行 conformance 通过；但它是文本后处理，不应作为长期契约。

**永久修法**：在 `compiler.all.ws` 的 cgen 中直接发射顶层 `_qN()`，
带唯一名字/前置声明；或改成不依赖嵌套函数的闭包 ABI。
这会改动冻结的编译器源码，必须走固定点/seed 重新验证。

**判据**：

- `.c` 中不再出现 `auto void _qb`；
- 同一源码经 cgen 直接产出的 C 与“cgen + lift”的语义一致；
- 固定点仍成立；
- 迁移完成后删除 `scripts/lift-nested.py` 的调用（保留测试）。

### W3 — 构建目标（2–3 天）

**Phase 1（不改 Rust）**：

```bash
# 零 Rust C 发射（wd 是 Whisper 自身编译出的 C 程序）
wd c hello.ws hello.c
# 方案 1（零额外 sysroot）：Zig 自带 wasi-libc
zig cc -target wasm32-wasi -O2 \
  -D_WASI_EMULATED_MMAN -D_WASI_EMULATED_SIGNAL \
  -I whisperc hello.c -o hello.wasm \
  -lwasi-emulated-mman -lwasi-emulated-signal
# 方案 2：wasi-sdk clang
# clang --target=wasm32-wasip1 --sysroot="$WASI_SDK/share/wasi-sysroot" ...
iwasm --dir=. hello.wasm        # WAMR（C 运行时，零 Rust）
```

以上已封装为 `scripts/build-wasm.sh`：参数 `<src> <out.wasm>`，
内部用 `wd c`（缺失时回退 `whisper bootstrap`）+ `lift-nested.py` +
Zig/clang；支持 `WASM_CC`/`WASI_SDK_PATH`/`WASM_RUNTIME` 环境变量。
`wd c` 与 `whisper bootstrap` 的 C 输出已逐字节对拍（hello）。

**状态**：

- hello：✅ `Hello, World!`
- core conformance：✅ 26/27（1 个原语义 skip；WASI 文件 IO 单独用例通过）
- HOF：✅ 经 `lift-nested.py`，全部通过；cgen 直接顶层化仍是 W2.5
- `-t wasm` CLI：❌ 尚未加（当前用 `scripts/build-wasm.sh`）

**验收**：同一源码经 Phase 1/Phase 2 产出语义一致的 wasm；构建可复现
（固定 `-g0`/路径/版本；若 clang 仍带路径，记录差异原因）。

### W4 — `wrun` 宿主策略（3–5 天）

`wrun` 是宿主启动器（当前为 bash 薄包装，后续可用 C/Whisper 重写）：

```text
wrun [--dir GUEST::HOST]... [--env NAME]... \
     [--no-network] [--no-exec] [--no-ffi] app.wasm [args...]
```

- 默认：无 dir、无 env、无 network、无 exec、无 ffi。
- `--dir GUEST::HOST` 映射到 WASI preopen；**v1 只有 rw**（WAMR
  没有 read-only preopen 开关），只读需求要靠 OS 账户/容器。
- `--env NAME` 白名单；只透传显式列出的变量。
- `--http-allow`/`--exec`/`--ffi` **显式拒绝**（“not implemented in
  wasm v1”），不静默忽略；host 代理/exec 留到 Phase 4。
- 策略在 wasm 外；`.wasm` 无法自我提权。

**状态（2026-10-21）**：`scripts/wrun.sh` + `scripts/wrun-gate.sh`
（W-1..W-7 7/7）已落地；默认拒绝、preopen、env、unsupported 均实测。

**验收**：无 policy 时 `read-file`/`getenv`/`@2`/`@5`/`ffi-*` 全拒绝；
有 policy 时只允许指定范围；错误信息可定位（不泄漏未授权路径）。

### W5 — 功能矩阵与降级（5–10 天）

| 功能 | WASM v1 | 处理 |
|------|---------|------|
| 算术/字符串/列表/map/JSON/regex/pack/conf | ✅ | 必须过 conformance |
| `read-file`/`write-file` | ✅ preopen | 路径规范化 |
| `listdir`/`mkdir`/`file-exists` | ✅ preopen | 同上 |
| `getenv`/`@4` | ✅ allowlist | 只传允许的变量 |
| `now`/`sleep`/`strftime` | ✅（时区受限） | 文档标注 |
| `random`/`seed!` | ✅ | `random_get` |
| `read-line`/stdin | ✅ | fd 0 |
| `args`/`exit`/`print`/`.` | ✅ | WASI |
| `@0/@1` | ✅ 同 preopen | 与 builtin 共用 host 接口 |
| `@2/@3` HTTP | ⚠️ host 代理 | Phase 4 |
| `@5/@10` exec | ❌ v1 | 显式 unsupported；trusted 由 host exec |
| `@6`-`@9` | ❌ v1 | 显式 unsupported |
| `ffi-*` | ❌ v1 | 显式 unsupported |
| `listen`/`accept`/`recv`/`send`/`serve-concurrent` | ❌ v1 | 显式 unsupported（WASI P1 无 socket/thread） |
| `buf-*` 裸内存 | ⚠️ 审计 | 不允许用于 guest 可见的任意地址 |
| `kv-*` | ✅ 进程内 | 不跨进程 |

**原则**：不支持的词必须在编译期（缺少 host import）或运行期（明确错误）
失败；不能返回空值/0 让程序“看起来能跑”。

### W6 — 测试与门禁（3–5 天）

- `tests/wasm_preregistered.md` 的 W-1..W-9。
- 正向：
  - conformance 27（core 子集；按显示差异归一化）；
  - stdlib 20 模块的“纯计算”子集；
  - `apps/journal` 的文件读写（preopen 临时目录）。
- 负向：
  - 无 preopen 读文件失败；
  - `../`、绝对路径、symlink 逃逸失败；
  - 未允许 env 失败；
  - `@5`/`@10`/`ffi-*`/`listen`/`serve-concurrent` 显式失败；
  - 未知 host import 被拒绝；
  - 策略外路径不可见。
- 双跑：VM/c2/WASM 三方输出对拍（先允许已记录显示差异）。
- CI：新增 `wasm` job；工具链 pin；不阻塞现有 gcc/Rust 门禁。

### W7 — 打包、CI 与性能（2–3 天）

- CI 镜像/缓存：wasi-sdk + runtime；构建时间目标 < 2 分钟（hello）。
- 性能基线：`apps/journal` 10,000 条记录 list/search；记录 wasm vs c2 vs VM，
  不设“必须多快”的硬阈值，先建立曲线。
- 发布说明：`-t wasm` 输出 `.wasm`，不是单文件 exe；需要 runtime。
- 文档：README 用法、限制、policy 示例、故障排查。

### W8 — 收口与评审（1–2 天）

- 安全模型、spec、stdlib、tutorial、backlog 全部对齐。
- 评审威胁模型：T1/T3 是否覆盖；T2（恶意字节码）是否需要额外验证器。
- 决定是否继续 Phase 4（HTTP/exec host import）或停在“纯计算 + 文件”v1。

---

## 4. 里程碑

| 里程碑 | 内容 | 预计 | 状态 |
|--------|------|------|------|
| M0 | 文档/安全模型/判据（本次） | 1–2 天 | ✅ 已落地 |
| M1 | 工具链 + hello.wasm + preflight | 2–3 天 | ✅ hello 全链路通过 |
| M2 | 运行时分层 + core conformance | 1–2 周 | 🟡 runtime WASI 守卫 + 26/27；cgen 顶层化待做 |
| M3 | preopen/policy + 负向安全测试 | 1 周 | 🟡 preopen 正/负各一例；完整套件待补 |
| M4 | journal/文件应用 + CI wasm job | 3–5 天 | ❌ |
| M5 | HTTP 代理/exec host（可选） | 1–2 周 | ❌ |
| M6 | 性能/发布/评审 | 3–5 天 | ❌ |

总计：**4–6 周（1 人，低置信度；W2 的 libc/`setjmp`/路径问题可能拉长）**。

---

## 5. 关键决策记录

- **D1**：先 WASI Preview 1（`wasi_snapshot_preview1`），不追 Preview 2
  的 component model；C 工具链成熟度优先。
- **D2**：运行时**只用 C 实现**：WAMR `iwasm` 首选，wasm3 备选，
  `wasm2c`（WABT，AOT 到 C）用于单文件/无外部 runtime 的场景。
  **wasmtime/wasmer 是 Rust 实现，不采用**。
- **D3**：不改 Rust 源码；`wd c` 已能零 Rust 发射 C；Zig `cc`/wasi-sdk
  clang 负责 C→wasm，`whisper bootstrap` 只作回退。
- **D4**：v1 不支持 `@5`/FFI/裸 socket/多线程 server；显式报错。
- **D5**：native `exe/c2/native/wd` 标记 trusted-only；WASM 是不可信代码
  的唯一目标。
- **D6**：策略在宿主（`wrun`），不写进 `.wasm`。

---

## 6. 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| WASI P1 无 socket/thread/exec/ffi | 功能缺口 | v1 明确不支持；HTTP/exec 走 host import（Phase 4） |
| `runtime.c` POSIX 假设多 | 移植超期 | 已加 WASI 守卫（文件/env/时间/core 可跑；network/exec/FFI 排除）；M1 通过 |
| `setjmp/longjmp` 在 wasi-libc 不可用 | `try` 需重写 | 已确认不可用；v1 `try` 显式 unsupported，后续用错误传播改造 |
| cgen 生成 GNU C 嵌套函数（HOF） | clang/Zig 直接编译失败 | `scripts/lift-nested.py` 临时桥；永久修法 W2.5 |
| 文本后处理提升嵌套函数 | 解析脆弱/可能与新 cgen 输出漂移 | 固定生成器版本 + 对拍；W2.5 顶层化后删除桥 |
| `wasm32` 4GB 线性内存 | 大数据受限 | 记录上限；必要时 memory64（远期） |
| 路径语义（Windows 盘符/`\`/symlink） | 逃逸或不可用 | 统一转 `/`；preopen + `..`/绝对路径拒绝；负向测试 |
| 时区/`strftime` | 输出差异 | v1 标 UTC/受限；测试记录 |
| 性能/启动 | 不如 native | 记录基线；计算密集可接受；host call 优化 |
| 工具链不可复现 | 构建差异 | pin Zig/wasi-sdk 版本；`-g0`；固定 sysroot；记录 hash |
| `whisper bootstrap` 依赖 Rust CLI | 构建链非零 Rust | `wd c` 已落地并逐字节对拍；bootstrap 仅回退 |
| 未知 host import 静默失败 | 安全假象 | 启动时校验 import 表；未知即拒绝 |
| `serve-concurrent` 迁移 | webapp 不可用 | v1 明确不支持；保留在 VM/native trusted |

---

## 7. Definition of Done（WASM v1）

- [x] `scripts/build-wasm.sh` 能从 `.ws` 产出 `.wasm`，hello 输出正确。
- [x] core conformance：26/27 可运行用例通过；原 `26_file_io` 用宿主
      绝对路径，按语义 skip，另有 WASI preopen 版本通过。
- [x] `read-file`/`write-file`/`mkdir`/`file-exists` 只能访问 preopen；
      `../` 与绝对路径被拒（S-2/S-9/S-10/S-11）；`listdir` 尚未单列门禁。
- [x] 无 policy 时，文件/env/network/exec/ffi 全部拒绝；有负向测试
      （S-1/S-3/S-5/S-6/S-12）。
- [ ] `@5`/`@10`/`ffi-*`/`listen`/`serve-concurrent` 显式 unsupported
      （`@2/@3/@5/@10`、`try`、FFI 已验；listen/serve 待验）。
- [ ] CI 有 `wasm` job，工具链 pin，不阻塞现有门禁。
- [x] README/spec/security-model/stdlib/tutorial 与实现一致（持续维护）。
- [ ] 性能基线记录在案（journal 10k）。
- [x] `crates/**` 无改动（`git diff --stat -- crates` 为空）。

---

## 8. 与 Step 5（Whisper VM）的关系

WASM 目标是**安全执行**路线；Step 5 的 Whisper VM 是**独立/元循环**路线。
两者不冲突，但不能互相替代：

- WASM：借用成熟运行时，先拿到“不可信代码可跑”的安全边界；
- Whisper VM：证明语言能自我承载，性能与安全仍要单独设计。

如果未来两者都做，建议三方对拍：Rust VM / C 链 / Whisper VM / WASM，
其中 WASM 作为安全基线，Whisper VM 作为独立性验证。
