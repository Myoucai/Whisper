# Whisper 1.3.10 发布说明

## 主题

**多租户隔离评审 + 依赖 SBOM。**

## 新增

### 多租户隔离评审

[docs/multi-tenant.md](multi-tenant.md)：

- native / VM / WASM 信任模型
- 每租户推荐启动参数
- 文件、环境、网络、FFI、资源、侧信道隔离矩阵
- 部署层要求：独立 UID、cgroup、网络命名空间、只读根
- 评审清单

结论：

- native C **不是**多租户边界
- WASM 租户使用 `--dir-ro-strict` + 配额 + 独立进程
- userns 不可用时 fail closed，不回退快照

### SBOM

```bash
bash scripts/sbom.sh target/sbom.json
bash scripts/sbom-gate.sh
```

- CycloneDX 1.4 风格
- workspace crates
- WAMR iwasm 固定版本
- 动态可选 OpenSSL
- `cargo metadata --locked`，离线时降级为 workspace-only
- CI 校验 bomFormat/specVersion/whisper 版本/WAMR 组件

```text
sbom-gate: PASS (9 components, whisper 1.3.10)
```

## 验证

- SBOM gate: PASS
- CI YAML: valid
- cargo check --workspace: pass
- 既有 HTTP/WASM/配额门禁保持不变

## 兼容性

- 无运行时行为变化
- 仅新增文档、脚本和 CI 步骤
- WASM v1 仍无网络

## 已知限制

- SBOM 离线 fallback 只含 workspace，不含完整 Rust 依赖图
- WAMR CVE 跟踪仍是流程项，不是自动告警
- 多租户强隔离仍需 cgroup/容器/独立节点
- 完整安全审计与 fuzz 仍待做

## 升级建议

- 多租户部署前按 [multi-tenant.md](multi-tenant.md) 清单执行
- 每次发布归档 `target/sbom.json`
- 生产环境叠加 cgroup v2 和独立 UID
