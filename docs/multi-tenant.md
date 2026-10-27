# 多租户隔离评审

> 适用版本：1.3.10。本文是多租户部署前的安全评审清单，不是法律/合规认证。

## 1. 信任模型

| 执行体 | 信任级别 | 隔离手段 |
|---|---|---|
| native C (`whisper build -t c2`) | **可信** | 无沙箱；依赖 OS 用户/容器 |
| Rust VM (`whisper run`) | 可信工具 | 仅开发/自举；不是多租户边界 |
| WASM guest (`wrun`/`wd wasm`) | **不可信** | WAMR + WASI + `wrun` 策略 |

**不要用 native C 运行不可信租户代码。** native 程序拥有当前用户权限。

## 2. 每个租户的推荐启动方式

```bash
WHISPER_WASM_MAX_MEMORY_MB=64 \
WHISPER_WASM_STACK_KB=256 \
WHISPER_WASM_MAX_THREADS=4 \
WHISPER_WASM_MAX_FDS=64 \
WHISPER_WASM_TIMEOUT=30 \
bash scripts/wrun.sh \
  --dir-ro-strict /data::/srv/tenant/data \
  tenant.wasm
```

要求：

- 独立进程
- 独立 OS 用户
- `--dir-ro-strict`（Linux，userns + ro bind）
- 不传 `--env`，或只传显式白名单
- 不传 `--dir` 可写 preopen
- 不传 `--http-allow`/`--exec`/`--ffi`
- 外层再叠加 cgroup/容器（见第 5 节）

## 3. 隔离矩阵

| 边界 | 已实现 | 残余风险 |
|---|---|---|
| 文件读取 | preopen + symlink 拒绝 | 宿主路径 TOCTOU；WAMR CVE |
| 文件写入 | `--dir-ro-strict` 真实 EROFS | 无 userns 时 fail closed；`--dir-ro` 仍是快照 |
| 环境变量 | `--env` 白名单，默认空 | 宿主自身环境不会透传 |
| 网络 | guest 无网络 import | 无 host HTTP proxy；未来需独立 broker |
| FFI | 显式拒绝 | 无 |
| 子进程 | 显式拒绝 | 无 |
| 内存 | WAMR heap/stack + fd | 非 cgroup RSS 硬隔离 |
| CPU | wall-clock `timeout` | 非 CPU 时间/cgroup |
| 文件数 | `ulimit -n` | 非文件系统 inode/容量 |
| 并发 | `--max-threads` | guest 无网络，无连接数问题 |
| 审计 | `wrun` 拒绝原因 + 宿主日志 | 无 per-tenant 结构化审计 |

## 4. 主要威胁与缓解

### T1 租户读取宿主文件

- 默认无 preopen
- `--dir-ro-strict` 只读绑定
- symlink 拒绝
- **残余**：宿主目录在挂载前被替换、内核/WAMR 漏洞

### T2 租户写宿主文件

- `--dir-ro-strict` 内核 EROFS
- 无 `--dir` 可写 preopen
- **残余**：无 userns 环境必须拒绝，不能回退快照

### T3 租户逃逸网络

- WASM v1 没有 socket import
- `wrun` import 白名单
- **残余**：不得为了“方便”添加 raw socket import；未来只做命名 host proxy

### T4 资源耗尽

- heap/stack/threads/fd/timeout
- **残余**：无 cgroup 聚合限制；多租户需外层 cgroup/容器

### T5 侧信道/共享内核

- 独立进程/用户只能降低，不能消除
- 高价值租户需要独立 VM/节点

### T6 供应链

- WAMR 固定版本（当前 2.4.5）
- Rust 依赖 `cargo --locked`
- SBOM：`scripts/sbom.sh`

## 5. 部署层要求

- 每租户独立 UID
- 只挂载租户目录
- 只读根文件系统
- cgroup v2：memory.max、cpu.max、pids.max
- 禁止 `CAP_SYS_ADMIN`（userns 挂载需要的能力按平台评估）
- 网络命名空间默认无网卡
- 日志集中收集

## 6. 评审清单

- [ ] 租户代码只以 WASM 运行
- [ ] 无 `--dir` 可写 preopen
- [ ] 使用 `--dir-ro-strict`，无 userns 时拒绝启动
- [ ] 设置 heap/stack/threads/fd/timeout
- [ ] 不传 `--env` 或只传白名单
- [ ] 外层 cgroup 限制 memory/cpu/pids
- [ ] 独立 UID/网络命名空间/只读根
- [ ] WAMR 版本与 CVE 跟踪
- [ ] SBOM 随版本归档

## 7. 参考

- [security-model.md](security-model.md)
- [wasm-wasi-plan.md](wasm-wasi-plan.md)
- [wasm-v1-release.md](wasm-v1-release.md)
- [release-notes.md](release-notes.md)
