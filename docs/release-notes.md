# Whisper 1.3.25 发布说明

## 主题

**systemd cgroup 真实生效验证；WAMR CVE watch 补 2026 漏洞。**

## systemd 239 / OpenCloudOS 实测

```text
systemd-analyze verify: clean
whisper-tenant@tenant-a: success
MemoryMax=134217728
CPUQuotaPerSecUSec=500ms
TasksMax=16
LimitNOFILE=64
TimeoutStartUSec=2min
```

内存上限实际生效：

```text
fake runtime 分配 256 MiB
-> status=137 (killed)
```

模板兼容性修复：

- 移除 systemd 239 不支持的 `ProtectClock` / `ProtectHostname`
- 用 `TimeoutStartSec=120` 替代 oneshot 下无效的 `RuntimeMaxSec`
- `systemd-analyze verify` 无 warning

## WAMR CVE watch

补 2.4.5 release 修复的三个 CVE：

- CVE-2026-54912：fast interpreter heap buffer overflow
- CVE-2026-54913：`load_init_expr` partial uninitialized read
- CVE-2026-54914：WASI `poll_oneoff` heap buffer overflow

影响：`<2.4.5`，当前 pinned 2.4.5 不受影响。

```text
wamr-cve-gate: PASS pinned=2.4.5 watch_reviewed=2026-11-03 (5 CVEs)
```

## 文档

- `deploy/README.md`
- `docs/security-audit.md`
- `docs/wamr-cve.md`
- `security/wamr-cve-watch.json`

## 兼容性

- 无运行时行为变化
- systemd unit 对 239 更兼容；新版本仍可用
- 仅补验证和 CVE 记录

## 已知限制

- 测试 runtime 用 fake 脚本，不是 WAMR
- 未在 cgroup v2-only 集群/K8s 验证
- 容器内真实 WAMR 写拒绝仍待验证（服务器无法访问 GitHub 下载 WAMR）
- 外部安全审计仍未做
