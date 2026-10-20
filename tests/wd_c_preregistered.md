# `wd c`（零 Rust C 发射）预注册判据

2026-10-21。目标：给 `wd` 增加只发射 C 的动词，使 WASM 构建不依赖
Rust CLI 的 `whisper bootstrap`。判据先于实现记录于此（实现后补执行记录）。

## 判据

- **C-1**：`wd c <src.ws> <out.c>` 只写 `<out.c>`；不产生 `<out.c>.exe`
  或 `<out>.exe`。
- **C-2**：同一源码的 `wd c` 输出与 `whisper bootstrap` 输出逐字节一致。
- **C-3**：`wd c` 不调用 C 编译器；`WHISPER_CC=/bin/false` 时仍成功。
- **C-4**：`wd build` 不回归：仍能编译并运行 `hello.ws`。
- **C-5**：usage 列出 `c`。

## 反向判据

- 为 `c` 复制一份 `build` 逻辑并顺带调用 cc → 失败。
- `wd c` 写 `.c.c` 或额外产物 → 失败。
- 输出与 bootstrap 有差异却用“等价”掩盖 → 失败。
- 改 Rust 代码实现 → 违反约束。

## 执行记录（2026-10-21）

```bash
bash scripts/wd-c-gate.sh
# C-1..C-5 5/5
```

实现位置：`whisperc/wd.ws` 的 `wd-build`，在缓存/cc 之前分流
`verb == "c"`。`wd.c` 与 `bootstrap.c` 的 hello SHA256 相同。
