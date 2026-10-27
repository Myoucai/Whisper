# Whisper 1.3.9 发布说明

## 主题

**WASI 文件描述符配额：限制 guest 打开的文件和 socket 数量。**

## 新增

```bash
WHISPER_WASM_MAX_FDS=64 bash scripts/wrun.sh app.wasm
```

行为：

- 通过 POSIX `ulimit -n` 限制 guest 进程 fd 上限
- 文件、管道、socket 都计入
- 范围 3..65535
- 无法应用时 fail closed
- 与 heap/stack/threads/timeout、preopen/env/import 白名单叠加

## 测试

`wrun-gate` 新增 W-10：

```text
W-9  ✅ WASM heap/stack/threads 配额参数接受
W-10 ✅ WASM fd 配额（ulimit -n）生效
wrun-gate: 10/10
```

## 兼容性

- 未设置 `WHISPER_WASM_MAX_FDS` 时保持旧行为
- `WHISPER_WASM_TIMEOUT` 仍依赖 `timeout` 命令
- `--dir-ro` 仍是快照；`--dir-ro-strict` 才是 Linux EROFS
- WASM v1 仍无网络

## 已知限制

- `ulimit -n` 是进程级软限制，不是 cgroup 聚合隔离
- 文件系统总容量、inode 数仍需 OS/容器层
- macOS 的 `ulimit -n` 上限可能受系统限制
- WASM 连接数无独立配额（guest 无网络）

## 升级建议

- 多租户 WASI 执行至少设置 heap/stack/fd/timeout
- 宿主绝对不被修改时使用 `--dir-ro-strict`
- 更严格的隔离叠加容器/cgroup
