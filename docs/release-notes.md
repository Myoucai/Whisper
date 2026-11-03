# Whisper 1.3.23 发布说明

## 主题

**真实 Linux 只读挂载 fallback 验证。**

## 验证环境

- Linux 5.4 / OpenCloudOS 8.8
- root 创建：
  ```bash
  mount --bind host mnt
  mount -o remount,bind,ro mnt
  ```
- `findmnt -no OPTIONS --target mnt` → `ro,noatime`
- `touch mnt/x` → `Read-only file system`

## 结果

```text
wrun: preopen ... is already on a read-only mount; using direct ro mapping
RC=0
```

rw host 仍走 userns：

```text
WRUN_IWASM=/bin/true wrun --dir-ro-strict /data::rw-host hello.wasm
RC=0
```

（测试用 `/bin/true` 替代 WAMR，以隔离 mount 行为；WAMR 的 guest 写入
EROFS 已在 1.3.13 / OpenCloudOS 上验证。）

## 文档

- `docs/wasm-wasi-plan.md`
- `docs/security-audit.md`
- `deploy/README.md`

`wrun-gate: 15/15`，W-12 使用合成 mountinfo 做稳定回归。

## 兼容性

- 无运行时行为变化
- 仅补真实环境验证记录
- 真实容器（Docker/Podman）仍需在目标环境验证

## 已知限制

- 本次未安装 Docker/Podman；容器验证使用真实 ro bind mount 等价路径
- 无 userns 且宿主 rw 时仍 fail closed
- 容器 seccomp 可能影响 userns；已有 ro 卷不受影响
