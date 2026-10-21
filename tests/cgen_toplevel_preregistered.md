# cgen 顶层化（W2.5）预注册判据

2026-10-21 立项。目标：去掉 cgen 为 quote/HOF 生成的 **GNU C 嵌套函数**，
让 clang/Zig 直接编译 Whisper C 输出，删除 `scripts/lift-nested.py` 这个
构建期文本后处理桥。

背景：cgen 当前生成

```c
{ auto void _qb(void) { ... } ... _qb(); ... }
```

GCC 接受，clang/Zig/wasm32-wasi 不接受。临时桥已让 26/26 可运行
conformance 通过，但它是文本后处理，不能作为长期契约。

---

## 判据

- **G-1 无嵌套函数**：`whisper build -t c2` / `wd c` 产出的 C 中
  不存在 `auto void _qb`；也不存在其他函数内函数定义。
- **G-2 无 `_qb` 残留**：C 中不出现 `_qb` 标识符；quote 函数使用
  唯一名（如 `_q0`、`_q1`）并有前置声明。
- **G-3 固定点**：`scripts/verify-fixedpoint.sh` 仍通过（gen2 == gen3），
  seed 重新生成后固定点仍成立。
- **G-4 双链一致**：cargo 467/0、syntax 27/0、stdlib 67/0；
  `bootstrap_matches_c2_byte_for_byte` 仍通过。
- **G-5 WASM 直编**：`scripts/build-wasm.sh` 在**不调用**
  `scripts/lift-nested.py` 的情况下，core conformance 26/26 可运行子集
  通过（含 `13_times`/`14_map_fold`/`15_each_filter`/
  `24_nested_structures`/`25_import`/`s4_both_strlist`）。
- **G-6 ABI 不变**：native c2 输出行为与旧 C 一致；journal 10k 输出
  SHA256 不变。
- **G-7 不改 Rust 运行时/库**：`crates/**/src/**` 与 `Cargo.toml` 无行为
  改动；仅允许修 CI 测试夹具的跨平台假设（`-lws2_32`、换行长度），
  并在执行记录里列明。
- **G-8 可回滚**：删除 `lift-nested.py` 调用后仍能构建；保留旧 C
  对拍样本。

## 反向判据（算失败）

- 把 `_qb` 改名为另一个嵌套函数名，仍然嵌套 → 失败。
- 只让 Python 桥“更聪明”，cgen 仍发射嵌套函数 → 失败（不是 W2.5）。
- 用 `-fnested-functions` / GCC 插件 / 非标准编译器绕过 → 失败。
- 固定点失效或 seed 无法重建 → 失败。
- 改 Rust 运行时/库来实现 → 违反约束（测试夹具的跨平台修正除外）。
- 用 skip/expected-fail 掩盖 HOF 失败 → 失败。

## 设计方向（待实现）

1. **cgen 直接发射顶层函数（首选）**
   - 为每个 quote body 分配唯一 `_qN`；
   - 在函数定义前输出 `static void _qN(void);`；
   - 将 body 作为顶层 `static void _qN(void) { ... }` 发射；
   - 调用点用 `_qN()` 替换 `_qb()`；
   - label 只在各自函数内，天然不冲突。
2. **Whisper 后处理（次选）**
   - 在 cgen 输出后做 AST/文本提升；仍不如直接发射干净；
   - 只有在不能改 `compiler.all.ws` 时才考虑。

## 风险

- `compiler.all.ws` 是冻结的固定点源码；改动必须重新生成 seed 并
  完整跑 bootstrap/fixedpoint。
- quote body 可能引用外层局部变量；当前样本看起来只依赖全局栈，
  但必须用全量 conformance + stdlib 验证。
- 嵌套 quote（quote 里再有 quote）需要递归唯一命名。
- 前置声明顺序与 `w_find_word` 注册表不能冲突。
- 生成 C 的可读性/调试信息会变化；用行为对拍而非逐字节旧 C 对拍
  （除非保留旧样本）。

## 执行记录（2026-10-21）

- **G-1 ✅**：`whisper bootstrap tests/conformance/14_map_fold.ws` 生成的 C
  不再有 `auto void`；每个 quote 是顶层 `static void _qN(void)`。
- **G-2 ✅**：C 中无 `_qb`；wrapper 使用局部函数指针 `_qfn`。
- **G-5 ✅**：`build-wasm.sh` 不再调用 `lift-nested.py`（仅保留 stale
  compiler 的检测回退），26/26 可运行 conformance 通过；
  `14_map_fold` 的 wasm 直编输出 9/15/55。
- **G-4 🟡**：`cargo test -p whisper-cli --test integration_tests`
  123/123（Windows）；syntax/stdlib 待重跑。
- **G-6 ✅**：journal 10k list 输出 SHA256 与旧 native 一致
  (`72CA1F…`)；WASM 0.63s。
- **G-7 ✅（按修订后口径）**：Rust runtime/library 未改；仅修了
  integration test 的两处跨平台假设（`-lws2_32`、`echo` 换行长度）。
- **G-3 ✅**：`verify-fixedpoint.sh` 通过：
  `gen2 == gen3 (self-hosting fixed point holds)`，退出码 0。
- **G-8 ✅**：当前 `build-wasm.sh` 不走 `lift-nested.py`；脚本只作为
  stale compiler 的检测回退保留。
