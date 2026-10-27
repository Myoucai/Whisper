# Whisper 1.3.2 发布说明

## 主题

**WASI 严格只读 preopen：Linux 真实 EROFS，不再把快照当作只读。**

## 新增

### `wrun --dir-ro-strict`

```bash
bash scripts/wrun.sh \
  --dir-ro-strict /data::target/data \
  app.wasm
```

- 仅 Linux
- 私有 user/mount namespace
- `mount --bind` + `remount,bind,ro`
- guest 写入返回 EROFS
- 宿主内容与 mtime 不变
- `unshare` / user namespace / mount 不可用时 fail closed
- 不静默回退到 `--dir-ro` 快照

旧模式保留：

```bash
--dir-ro /data::target/data
```

### 验证

在 OpenCloudOS 8.8（glibc 2.28）+ WAMR 2.4.5 上实测：

```text
write-file -> -1
read-file  -> 4（已有内容）
host content = orig
host mtime 不变
```

CI 的 `wrun-gate` 增加 W-8：

- 支持 unprivileged user namespace 时执行真实写入检查
- 无 userns 时明确 SKIP

## 兼容性

- `--dir-ro` 行为不变：快照，不是 EROFS
- `--dir-ro-strict` 是新增 opt-in
- macOS/Windows 不支持严格挂载，调用会明确失败
- WASM v1 仍无网络

## 已知限制

- 严格模式依赖 Linux user namespace；容器/受限内核可能不可用
- 完整 header 语义对拍仍待补
- `WHISPER_NET_REDIRECT=follow` 未实现
- VM `@7` dummy 实参缺陷仍记录在案

## 升级建议

- 需要“宿主绝对不被修改”时使用 `--dir-ro-strict`
- 需要可移植性时继续使用 `--dir-ro` 快照
- 运行不可信 WASM 仍应叠加 OS/容器隔离
