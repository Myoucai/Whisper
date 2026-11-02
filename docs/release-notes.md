# Whisper 1.3.17 发布说明

## 主题

**WAMR CVE reviewed watchlist + CI 版本门禁。**

## 新增

- `security/wamr-cve-watch.json`
- `scripts/wamr-cve-gate.py`
- `docs/wamr-cve.md`
- CI step `WAMR CVE watch`

门禁做三件事：

1. 从 `.github/workflows/wasm.yml` 读取 pinned `WAMR_VERSION`
2. 对照 reviewed CVE 范围
3. 超过 180 天未复核则失败

当前：

```text
WAMR_VERSION = 2.4.5
wamr-cve-gate: PASS pinned=2.4.5 watch_reviewed=2026-11-02 (2 CVEs)
```

## 已记录

- CVE-2025-64713：影响 `<2.4.4`，2.4.5 不受影响
- CVE-2024-27532：影响 2.2.0，2.4.5 不受影响

来源与更新流程见 [docs/wamr-cve.md](wamr-cve.md)。

## 为什么不是实时 feed

CI 不依赖 NVD/CIRCL 实时 API，避免：

- 网络抖动假失败
- 速率限制
- CVE 记录滞后/误报

采用“确定性门禁 + 定期人工复核”，发布时归档 SBOM 与 watchlist。

## 兼容性

- 无运行时行为变化
- 不改变 WAMR 版本
- 仅新增安全门禁与文档

## 已知限制

- 这是人工复核 watchlist，不是实时 CVE feed
- 只覆盖已记录 CVE
- 不覆盖 OpenSSL 等系统库
- 实时 feed / 自动升级 PR 仍待做
