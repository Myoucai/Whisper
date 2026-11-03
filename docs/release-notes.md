# Whisper 1.3.22 发布说明

## 主题

**`--dir-ro-strict` 容器/EROFS fallback：已有只读挂载直接映射。**

## 新增

此前 `--dir-ro-strict` 强制要求 unprivileged user namespace + mount。
在只读容器、EROFS rootfs、锁定 userns 的宿主上会失败。

现在：

1. 先检查 HOST 是否已在 kernel read-only mount 上：
   - `findmnt -no OPTIONS --target HOST`
   - 或解析 `/proc/self/mountinfo`
2. 是 → 直接 `--map-dir`，写入由内核 EROFS 拒绝
3. 否 → 仍走 userns + ro bind mount
4. 两者都不可用 → fail closed，不回退快照

支持：

- Docker/Podman `:ro` 卷
- EROFS rootfs
- 只读 bind mount
- rootfs 只读的容器

## 测试

`wrun-gate` 新增 W-12：

```text
W-12 ✅ 已只读挂载直接映射（无需 userns）
```

Linux 上用合成的 mountinfo 验证 fallback；真实内核只读行为仍由
`wrun-gate` W-8 和部署环境验证。

## 兼容性

- 普通 rw 目录行为不变，仍走 userns bind
- 已有 ro 挂载不再要求 unshare/mount
- 无 ro mount 且无 userns 时仍 fail closed
- `deploy/compose.yaml` 改用 `--dir-ro-strict`

## 已知限制

- fallback 只信内核 mountinfo/findmnt 的 `ro` 标志
- 无 userns 且宿主 rw 时没有安全 fallback
- mountinfo 解析是保守前缀匹配
- 仍需真实容器/EROFS 环境验证

## 下一步

- 真实容器/EROFS 环境验证
- 覆盖引导 fuzz / nightly 观察
- 外部安全审计
- 真实集群 cgroup 验证
