# Whisper 1.3.16 发布说明

## 主题

**把多租户隔离清单落成 systemd cgroup 与 Docker Compose 模板。**

## 新增

### systemd

`deploy/systemd/whisper-tenant@.service`：

- 每租户 `User=%i`
- `MemoryHigh=64M` / `MemoryMax=128M`
- `CPUQuota=50%`
- `TasksMax=16`
- `LimitNOFILE=64`
- `RuntimeMaxSec=120`
- `PrivateNetwork=yes`
- `ProtectSystem=strict` / `ProtectHome=yes`
- 空 `CapabilityBoundingSet`
- `--dir-ro-strict` 保持 userns/mount 可用

### Docker Compose

`deploy/compose.yaml`：

- `read_only: true`
- `network_mode: none`
- `cap_drop: ALL`
- `no-new-privileges`
- `mem_limit: 128m`
- `pids_limit: 16`
- `cpus: 0.5`
- 只读挂载 repo 与租户目录
- 使用 `--dir` + 只读卷（不依赖容器内 userns/mount）

### 验证

`scripts/deploy-gate.sh`：

- `systemd-analyze verify`（可用时）
- Compose YAML 关键字段校验
- CI 接入

## 使用

```bash
cat deploy/README.md
sudo systemctl start whisper-tenant@tenant-a
docker compose -f deploy/compose.yaml config
```

## 兼容性

- 无运行时行为变化
- 模板是可选的部署层
- cgroup 限制由 systemd/Docker 执行，不是 wrun 内部
- native C 仍不是沙箱

## 已知限制

- 模板未在真实集群验证
- 容器内 `--dir-ro-strict` 取决于 seccomp 是否允许 userns/mount
- 文件系统容量/inode quota 未包含
- WAMR CVE 跟踪、外部审计仍待做
