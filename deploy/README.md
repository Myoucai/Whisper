# 多租户部署模板

这是 [docs/multi-tenant.md](../docs/multi-tenant.md) 的可执行版本。两种方式：

- systemd：每租户独立 UID + cgroup v2 + mount/network namespace
- Docker Compose：只读根 + 无网络 + `cap_drop: ALL` + cgroup 限额

> native C 不是沙箱；这些模板只用于 WASM guest。

## 1. systemd

### 安装

```bash
sudo useradd --system --no-create-home --shell /usr/sbin/nologin tenant-a
sudo mkdir -p /srv/whisper/tenant-a/data
sudo install -d -o tenant-a -g tenant-a /srv/whisper/tenant-a
sudo cp /opt/whisper/deploy/systemd/whisper-tenant@.service /etc/systemd/system/
sudo systemctl daemon-reload
```

目录：

```text
/opt/whisper/bin/iwasm           # pinned WAMR
/opt/whisper/scripts/wrun.sh
/srv/whisper/tenant-a/app.wasm
/srv/whisper/tenant-a/data/
```

启动：

```bash
sudo systemctl start whisper-tenant@tenant-a
journalctl -u whisper-tenant@tenant-a
```

限制：

- `MemoryHigh=64M`, `MemoryMax=128M`, `MemorySwapMax=0`
- `CPUQuota=50%`
- `TasksMax=16`
- `LimitNOFILE=64`
- `RuntimeMaxSec=120`
- `PrivateNetwork=yes`, `ProtectSystem=strict`, `ProtectHome=yes`
- `CapabilityBoundingSet=` 空
- `--dir-ro-strict`：宿主机卷已是 ro 时直接映射；否则需要 userns/mount

### 每租户 UID

模板用 `User=%i`，实例名必须是对应系统用户：

```bash
sudo systemctl start whisper-tenant@tenant-a
# => User=tenant-a
```

如果不想每租户建用户，可改成固定 `User=whisper-tenant`，但隔离弱一档。

## 2. Docker Compose

```bash
cd deploy
docker compose up
```

`compose.yaml` 使用：

- `read_only: true`
- `network_mode: none`
- `cap_drop: ALL`
- `no-new-privileges`
- `mem_limit: 128m`, `pids_limit: 16`, `cpus: 0.5`
- `/opt/whisper` 和 `/srv/tenant` 只读挂载
- `wrun --dir-ro-strict`：检测到只读卷后直接映射，不依赖容器内
  userns/mount；如果卷不是 ro 且容器禁止 userns，则 fail closed

## 3. 验证

```bash
# systemd
systemd-analyze verify deploy/systemd/whisper-tenant@.service
systemctl show -p MemoryMax -p CPUQuotaPerSecUSec whisper-tenant@tenant-a

# compose
docker compose -f deploy/compose.yaml config
```

只读挂载 fallback 已在 Linux 5.4 / OpenCloudOS 上用真实
`mount --bind` + `remount,bind,ro` 验证：`findmnt` 返回 `ro`，wrun 走
direct ro mapping；rw host 仍走 userns。

## 4. 注意

- `/opt/whisper/bin/iwasm` 必须来自固定 WAMR 版本
- 应用代码只放 WASM，不放 native
- 租户目录由管理员写入，guest 只读
- 需要更强的隔离时叠加独立节点/VM
- 模板不替代 [security-audit.md](../docs/security-audit.md) 的外部审计
