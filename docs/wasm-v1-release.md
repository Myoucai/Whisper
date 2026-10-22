# WASM v1（无网络）发布说明

2026-10-22。状态：**v1.1.0 已发布**。本文件是 WASM v1 的支持矩阵、
安全边界和验收记录；后续版本见 [release-notes.md](release-notes.md) 和
[roadmap.md](roadmap.md)。

## 1. 定位

WASM v1 是 **零 Rust 运行期**的不可信代码执行目标：

```text
source.ws
  → wd c / wd wasm（Whisper 自身编译出的 C 程序）
  → C
  → zig cc / wasi-sdk clang
  → .wasm
  → WAMR（C 运行时）
```

它**不是**多租户生产沙箱。v1 的目标是：在没有网络、没有 exec/FFI 的前提下，
让 Whisper 程序在 WASI preopen 边界内运行，并且所有拒绝都是显式的。

## 2. 支持矩阵

| 能力 | v1 | 说明 |
|---|---|---|
| 核心语言 / 控制流 / HOF | ✅ | core conformance 27/27 |
| WASI 文件（preopen） | ✅ | 默认无 preopen；`--dir GUEST::HOST` 显式映射 |
| `--dir-ro` | 🟡 快照 | 复制到临时目录，guest 写入丢弃；**不是 EROFS** |
| 环境变量 | ✅ | `--env NAME` 显式白名单 |
| 时钟 / random / args | ✅ | WASI preview1 白名单 |
| `@2`/`@3`/`@6` HTTP | ❌ | 显式 unsupported；host proxy 未实现 |
| `@5`/`@10` exec | ❌ | 显式 unsupported |
| FFI | ❌ | 构建期/运行期拒绝 |
| raw socket / `listen` / `serve-concurrent` | ❌ | 显式 unsupported |
| `sock_*` / 非白名单 WASI import | ❌ | `wrun` 加载前拒绝（S-18/S-20） |
| symlink preopen | ❌ | `wrun` 拒绝（S-17） |

## 3. 安全属性

- **运行期零 Rust**：WAMR 是 C，`wd`/`wd wasm` 是 Whisper→C；没有 wasmtime/wasmer。
- **默认拒绝**：无 preopen、无 env、无网络、无 exec、无 FFI。
- **import 白名单**：只允许 `wasi_snapshot_preview1` 的显式函数；
  `env.*`、`sock_open` 等在加载前拒绝。
- **preopen symlink 拒绝**：裸 `iwasm --map-dir` 会跟随 symlink 逃逸，
  `wrun` 在启动前检查。
- **只读快照**：`--dir-ro` 的 guest 写入只落在临时副本，退出删除；
  宿主树不被修改。
- **固定点**：`gen2 == gen3`；seed 已用新 cgen 重建。

## 4. 已知限制（必须随发布说明）

1. `--dir-ro` **不是 EROFS**：guest 可能看到写入成功，只是不会持久化。
   严格只读需要 OS bind mount / 容器 / 只读文件系统。
2. **没有 OS 级 CPU/内存配额**；WASM 堆和宿主机资源由 WAMR/OS 约束，
   不能当作多租户隔离。
3. **无 host HTTP 代理**：v1 的 guest 不能访问网络。需要出网时必须单独
   设计 host proxy，不能通过放开 raw socket 实现。
4. **CI 的 `wd-build` 使用 `--fast`**：Rust CLI 只发射 C 供测试工具使用；
   生产默认 `scripts/build-wd.sh` 仍是零 Rust 三段链，零 Rust 全链由
   nightly workflow 覆盖。
5. WAMR 版本固定为 2.4.5；升级需要重跑安全门禁和 conformance。

## 5. 验收命令

```bash
cargo test --workspace
bash scripts/wasm-conformance.sh          # 27/27
bash scripts/wasm-security-gate.sh        # 20/20
bash scripts/wrun-gate.sh                 # 7/7
bash scripts/wd-c-gate.sh                 # 5/5
bash scripts/wd-wasm-gate.sh              # 4/4
bash scripts/wasm-preflight.sh --strict
bash scripts/verify-fixedpoint.sh         # gen2 == gen3
```

## 6. 发布前最后检查

- [x] 上述命令全绿（CI + 本地）
- [x] `README` / `spec` / `security-model` 与本文件一致
- [x] CHANGELOG 记录 v1 范围和限制
- [x] 已打 tag `v1.1.0` 并生成 GitHub Release
- [x] Release 附上本文件链接和已知限制

## 7. 不在 v1

- host HTTP proxy / host exec / host FFI
- WASI preview2
- 真 EROFS preopen
- AOT（wasm2c）性能路线
- 多租户资源配额
