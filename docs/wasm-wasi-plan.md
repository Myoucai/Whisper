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

---

## 1. 目标架构

```text
source.ws
   │  whisper bootstrap（现成，Rust CLI 不改）或未来的 wd c
   ▼
app.c  ── clang --target=wasm32-wasip1 ──┐
   │                                      │
   │  runtime_core.c                      │
   │  runtime_wasi.c  ────────────────────┤
   ▼                                      ▼
app.wasm  ── wrun policy ──>  WAMR / wasmtime / AOT
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

- 确认 `wasi-sdk`（clang + wasi-libc）版本；目标 triple：
  `wasm32-wasip1`（新）或 `wasm32-wasi`（旧，按 SDK 版本）。
- 确认运行时：`wasmtime`（开发方便）或 `WAMR`（C，适合零 Rust 分发）；
  记录版本并固定。
- `scripts/wasm-preflight.sh` 已落地（2026-10-20）；本机实测报告
  clang/wasi-sysroot/runtime 三项缺失，属于预期（尚未安装工具链）。
  脚本默认只报告、不阻塞 CI；`--strict` 供 CI 使用。
- 记录 AOT 可行性（`wamrc`）作为后续选项。

**验收**：preflight 能在一台干净机器上准确报告缺什么；CI 镜像可复现。

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

### W3 — 构建目标（2–3 天）

**Phase 1（不改 Rust）**：

```bash
whisper bootstrap hello.ws > hello.c
clang --target=wasm32-wasip1 --sysroot="$WASI_SDK/share/wasi-sysroot" \
      -O2 -I whisperc hello.c -o hello.wasm
wasmtime run --dir=.:ro hello.wasm
```

封装为 `scripts/build-wasm.sh`，参数：`<src> <out.wasm>`；支持
`WHISPER_CC`/`WASI_SDK`/`WASI_RUNTIME` 环境变量。

**Phase 2（零 Rust 构建）**：

- 给 `wd` 增加 `wd c <src> <out.c>`（只生成 C，不调 gcc）或
  `wd wasm <src> <out.wasm>`（生成 C 后调 clang）。
- `wd` 仍是纯 Whisper；不依赖 Rust CLI。
- 与 `whisper bootstrap` 的输出做逐字节对拍（复用现有 c2/bootstrap 契约）。

**验收**：同一源码经 Phase 1/Phase 2 产出语义一致的 wasm；构建可复现
（固定 `-g0`/路径/版本；若 clang 仍带路径，记录差异原因）。

### W4 — `wrun` 宿主策略（3–5 天）

`wrun` 是宿主启动器（可以是 shell/PowerShell 薄包装，后续用 C/Whisper 写）：

```text
wrun [--dir DIR[:ro|rw]]... [--env NAME]... [--http-allow HOST]... \
     [--no-network] [--no-exec] [--no-ffi] app.wasm [args...]
```

- 默认：无 dir、无 env、无 network、无 exec、无 ffi。
- `--dir` 映射到 WASI preopen；只读/读写分开。
- `--env` 白名单；不透传整份环境。
- `--http-allow` 由 host 代理执行；WASM 侧只调用 `whisper.http_fetch`，
  不直接拿 socket。
- `--exec` 仅 trusted profile；argv 数组，不走 shell；命令白名单。
- FFI：v1 不提供 host import；未知 import 由链接/运行时拒绝。
- 策略在 wasm 外；`.wasm` 无法自我提权。

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

| 里程碑 | 内容 | 预计 |
|--------|------|------|
| M0 | 文档/安全模型/判据（本次） | 1–2 天 |
| M1 | 工具链 + hello.wasm + preflight | 2–3 天 |
| M2 | 运行时分层 + core conformance | 1–2 周 |
| M3 | preopen/policy + 负向安全测试 | 1 周 |
| M4 | journal/文件应用 + CI wasm job | 3–5 天 |
| M5 | HTTP 代理/exec host（可选） | 1–2 周 |
| M6 | 性能/发布/评审 | 3–5 天 |

总计：**4–6 周（1 人，低置信度；W2 的 libc/`setjmp`/路径问题可能拉长）**。

---

## 5. 关键决策记录

- **D1**：先 WASI Preview 1（`wasi_snapshot_preview1`），不追 Preview 2
  的 component model；C 工具链成熟度优先。
- **D2**：运行时开发期用 wasmtime；若要求“零 Rust 运行时”，发布用
  WAMR（C）或 AOT（`wamrc`）。本计划不强制二选一。
- **D3**：不改 Rust 源码；Phase 1 用现成 `whisper bootstrap` 出 C，
  Phase 2 给 `wd` 加 C/wasm 动词。
- **D4**：v1 不支持 `@5`/FFI/裸 socket/多线程 server；显式报错。
- **D5**：native `exe/c2/native/wd` 标记 trusted-only；WASM 是不可信代码
  的唯一目标。
- **D6**：策略在宿主（`wrun`），不写进 `.wasm`。

---

## 6. 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| WASI P1 无 socket/thread/exec/ffi | 功能缺口 | v1 明确不支持；HTTP/exec 走 host import（Phase 4） |
| `runtime.c` POSIX 假设多 | 移植超期 | 先拆 host 接口，逐词迁移；M1 先跑 hello |
| `setjmp/longjmp` 在 wasi-libc 不可用 | `try` 需重写 | M1 验证；不可用则 W2 增加错误传播改造 |
| `wasm32` 4GB 线性内存 | 大数据受限 | 记录上限；必要时 memory64（远期） |
| 路径语义（Windows 盘符/`\`/symlink） | 逃逸或不可用 | 统一转 `/`；preopen + `..`/绝对路径拒绝；负向测试 |
| 时区/`strftime` | 输出差异 | v1 标 UTC/受限；测试记录 |
| 性能/启动 | 不如 native | 记录基线；计算密集可接受；host call 优化 |
| wasmtime 是 Rust | 与“零 Rust”冲突 | 发布用 WAMR/AOT；wasmtime 只做开发/对照 |
| 工具链不可复现 | 构建差异 | pin 版本；`-g0`；固定 sysroot；记录 hash |
| `whisper bootstrap` 依赖 Rust CLI | Phase 1 不是零 Rust | Phase 2 补 `wd c/wasm`；验收时区分 |
| 未知 host import 静默失败 | 安全假象 | 启动时校验 import 表；未知即拒绝 |
| `serve-concurrent` 迁移 | webapp 不可用 | v1 明确不支持；保留在 VM/native trusted |

---

## 7. Definition of Done（WASM v1）

- [ ] `scripts/build-wasm.sh` 能从 `.ws` 产出 `.wasm`，hello 输出正确。
- [ ] core conformance（27 个子集）VM/c2/WASM 输出一致（允许已记录差异）。
- [ ] `read-file`/`write-file`/`listdir`/`mkdir`/`file-exists` 只能访问 preopen。
- [ ] 无 policy 时，文件/env/network/exec/ffi 全部拒绝；有负向测试。
- [ ] `@5`/`@10`/`ffi-*`/`listen`/`serve-concurrent` 显式 unsupported。
- [ ] CI 有 `wasm` job，工具链 pin，不阻塞现有门禁。
- [ ] README/spec/security-model/stdlib/tutorial 与实现一致。
- [ ] 性能基线记录在案（journal 10k）。
- [ ] `crates/**` 无改动（`git diff --stat -- crates` 为空）。

---

## 8. 与 Step 5（Whisper VM）的关系

WASM 目标是**安全执行**路线；Step 5 的 Whisper VM 是**独立/元循环**路线。
两者不冲突，但不能互相替代：

- WASM：借用成熟运行时，先拿到“不可信代码可跑”的安全边界；
- Whisper VM：证明语言能自我承载，性能与安全仍要单独设计。

如果未来两者都做，建议三方对拍：Rust VM / C 链 / Whisper VM / WASM，
其中 WASM 作为安全基线，Whisper VM 作为独立性验证。
