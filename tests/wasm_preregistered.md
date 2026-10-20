# WASM/WASI 目标预注册判据

2026-10-20 立项。**判据先写死，再动代码。**
计划与架构见 `docs/wasm-wasi-plan.md`；安全模型见 `docs/security-model.md`。

状态：**pre-registered / 未实现**。下面每一条都必须在实现完成后用机器可验的
命令跑过，执行记录追加在文末。

---

## 判据

### W-1 工具链与构建

- `scripts/wasm-preflight.sh` 能报告 clang 的 wasm32 target、wasi-sysroot、
  WASM runtime；缺任何一项时给出可操作的安装提示。
- 对 `hello.ws`（`"Hello, WASM" .`）能产出 `.wasm` 文件，且
  `wasmtime run`（或 WAMR `iwasm`）输出与 c2 一致。
- 构建命令不要求修改 `crates/**`；`git diff --stat -- crates` 为空。

### W-2 基础语义

- 算术、字符串、列表、map、条件、循环、词定义、递归、引号/`call`、
  `try`、`print`/`exit`、`args` 在 wasm 上工作。
- `exit 3` 让宿主进程退出码为 3。
- stdout 逐字节：换行 LF，不出现 CRLF 翻译；非 ASCII 字符串按字节处理。
- `args` 传给 guest 的 argv 与 `wrun app.wasm a b` 一致。

### W-3 core conformance

- `tests/conformance/*.ws` 中不依赖网络/exec/FFI 的子集（目标 27 个）在
  VM、c2、WASM 三侧输出一致。
- 已知显示差异（VM 字符串带引号、C 链裸文本、`..` 格式）允许归一化，
  但必须在测试里写成**显式的归一化规则**，不能靠“看起来差不多”。
- 任一文件不一致 → 失败；不允许用 skip 掩盖。

### W-4 stdlib

- `std/core`、`std/math`、`std/list`、`std/str`、`std/json`、`std/conf`、
  `std/encoding`、`std/hash`、`std/csv` 的纯计算用例在 wasm 上通过。
- `read-file`/`write-file`/`listdir`/`mkdir`/`file-exists` 在 preopen 目录内
  通过。
- `apps/journal` 的基本 list/search（10k 条）在 preopen 临时目录上跑通，
  输出与 c2 一致。

### W-5 负向安全（最重要）

无 policy（默认）时，以下**全部必须失败**，且错误信息可定位：

- `"file" read-file` / `"file" write-file`
- `"PATH" getenv`
- `@0` / `@1` / `@4`
- `@2` / `@3`（无 host 代理时）
- `@5` / `@10`
- `ffi-open` / `ffi-sym` / `ffi-call`
- `listen` / `accept` / `recv` / `send` / `serve-concurrent`

“失败”可以是编译期缺少 import，也可以是运行期明确拒绝；**不能是 no-op、
返回空串/0、静默跳过**。

### W-6 policy 与路径

- `wrun --dir data:ro app.wasm` 后只能读 `data/` 下的文件；
  `../`、绝对路径、symlink 指向目录外 → 拒绝。
- `--dir data:rw` 才允许写；只读 preopen 下写失败。
- `--env FOO` 只暴露 FOO；未列出的 `getenv` 返回空或拒绝（二选一并写死）。
- `--http-allow example.com` 只允许该 host 及其子域；`evil-example.com`
  不得匹配；无 `--http-allow` 时网络拒绝。
- 未知 host import 在启动时拒绝，不能“忽略后继续”。

### W-7 显式 unsupported

- `@5`/`@10`/`ffi-*`/`listen`/`serve-concurrent` 在 wasm profile 下：
  - 编译期：缺少对应 opcode/import 时给出明确错误；或
  - 运行期：`unsupported on wasm profile: <word>`，退出非零。
- 不允许把这些词编译成空操作；不允许 `ffi-open` 返回 0 后继续。

### W-8 可复现与版本

- 固定 wasi-sdk/clang/runtime 版本，记录在 `scripts/wasm-preflight.sh`
  与 CI 配置中。
- 同一源码、同一工具链、同一路径下连续两次构建的 `.wasm` SHA256 相同；
  若 clang 将临时路径写入产物，则必须用 `-g0`/`--strip-all` 或固定工作目录
  消除；无法消除时记录原因并把“语义一致”作为降级判据。
- 不允许依赖宿主时区/随机数产生不确定输出（`now`/`random` 相关用例要
  固定 seed 或断言范围）。

### W-9 性能基线

- 记录 `apps/journal` 10k 条记录的 list/search、hello 启动时间；
  与 c2、VM 同机对比。
- 不设硬性“必须快于 X”阈值（首版只建立基线）；但必须记录，避免“感觉还行”。

---

## 反向判据（什么算失败）

- 用 Emscripten 的 POSIX 模拟把宿主文件系统/进程 API 直接暴露给 guest
  → **失败**（看起来能跑，但不是能力边界）。
- 为了“兼容”把 `@5`/`ffi-*` 编成 no-op 或返回 0 → **失败**。
- 没有 preopen 也能读任意路径 → **失败**。
- 只在默认策略下测“能跑”，不测“无授权必须失败” → **失败**。
- 改 Rust 代码来实现 WASM → **违反本轮约束**（需要单独批准）。
- 用 skip/expected-fail 掩盖 conformance 差异 → **失败**。
- 文档写“WASM 已安全”但 `@5`/FFI/网络仍可绕过 → **失败**。

---

## 执行记录

（实现完成后按 W-1..W-9 逐条追加：命令、结果、失败原因、修复提交。）
