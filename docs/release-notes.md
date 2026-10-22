# Whisper 1.2.0 发布说明（未发布）

## 主题

**native 网络权限开关 + WASM v1 保持无网络。**

## 新增

- `WHISPER_NET=0`：让当前 native 进程拒绝所有网络 opcode：
  `@2/@3/@6`、`listen/accept/recv/send`、`serve-concurrent`。
- `WHISPER_NET=1` / 未设置：保持历史行为，native 网络开启。
- 失败信息明确：
  `network disabled (WHISPER_NET=0; WASM v1 has no network at all)`。
- `scripts/net-gate.sh` 增加关闭网络的正向拒绝断言。
- native HTTP 集成测试显式设置 `WHISPER_NET=1`，不依赖宿主环境。

## WASM v1（不变）

- core conformance 27/27
- WASI 安全门禁 20/20
- `wd c` / `wd wasm` 零 Rust 入口
- preopen、env allowlist、symlink 拒绝、import 函数白名单
- `--dir-ro` 仍是**快照**，不是 EROFS
- **guest 仍然无网络**；host HTTP proxy 未实现

## 兼容性

| 场景 | 1.1.0 | 1.2.0 |
|---|---|---|
| native 默认网络 | 开 | 开（兼容） |
| `WHISPER_NET=0` | 无此开关 | 关闭全部网络 |
| WASM guest 网络 | 无 | 无 |
| `--dir-ro` | 快照 | 快照 |

## 验收

```bash
cargo test --workspace
bash scripts/net-gate.sh          # 含 WHISPER_NET=0 拒绝
bash scripts/app-gate.sh          # 13 routes
bash scripts/wasm-conformance.sh  # 27/27
bash scripts/wasm-security-gate.sh# 20/20
bash scripts/wrun-gate.sh         # 7/7
bash scripts/wd-c-gate.sh         # 5/5
bash scripts/wd-wasm-gate.sh      # 4/4
```

## 已知限制

- native 网络开关是整体开关，没有按域名/端口白名单；见 roadmap P2。
- native HTTPS 仍未在 C 链实现；见 roadmap P1。
- WASM 插件出网需要 host proxy；见 roadmap P3。
- 严格只读 preopen 需要 OS/容器层；见 roadmap P4。

## 升级建议

- 可信 native 程序：无需改动。
- 运行不可信 native 程序：使用 `WHISPER_NET=0`，但仍建议 OS 级隔离。
- 运行不可信 WASM：保持默认无网络；需要出网前先完成 host proxy。
