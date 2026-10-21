# `wd wasm` 预注册判据

2026-10-21。目标：在不改 Rust、不引入 Rust runtime 的前提下，给 `wd`
增加 `wasm` 动词：发射 C，然后直接调用 Zig 编译为 WASI 模块。

## 判据

- **W-1**：`wd wasm hello.ws out.wasm` 产出 `out.wasm` 与 `out.wasm.c`；
  WAMR 运行输出 `Hello, World!`。
- **W-2**：对 HOF 用例 `14_map_fold.ws`，输出 `9/15/55`；C 由顶层
  `_qN` 生成，不依赖 `lift-nested.py`。
- **W-3**：生成的 C 中不存在 `auto void _qb`。
- **W-4**：`wd` usage 列出 `wasm`。

## 反向判据

- 调用 `bash`/`build-wasm.sh` 才能工作 → 失败（应为直接的 C 工具链调用）。
- 需要修改 Rust CLI 或 Rust runtime → 违反语言独立约束。
- 生成的 wasm 需要 SIMD（WAMR 预编译不支持）→ 失败。
- 把 `wd wasm` 实现成 no-op/静默失败 → 失败。

## 执行记录（2026-10-21）

```bash
bash scripts/wd-wasm-gate.sh
# W-1 hello ✅
# W-2 HOF 9/15/55 ✅
# W-3 无 auto void _qb ✅
# W-4 usage 列出 wasm ✅
# wd-wasm-gate: 4/4
```

实现：`whisperc/wd.ws` 的 `wd-wasm` / `wd-wasm-cmd`，直接执行
`zig cc -target wasm32-wasi -O2 -mno-simd128 ...`，不经过 bash。
