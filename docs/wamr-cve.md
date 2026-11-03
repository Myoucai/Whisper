# WAMR CVE 跟踪

## 当前 pin

```text
WAMR_VERSION = 2.4.5
来源：.github/workflows/wasm.yml
```

## CI 门禁

```bash
python3 scripts/wamr-cve-gate.py
```

检查：

- 从 workflow 读取 pinned WAMR 版本
- 对照 `security/wamr-cve-watch.json`
- 命中受影响范围则失败
- watchlist 超过 `review_interval_days`（默认 180 天）未复核则失败

当前结果：

```text
wamr-cve-gate: PASS pinned=2.4.5 watch_reviewed=2026-11-03 (5 CVEs)
```

## 已记录 CVE

### CVE-2026-54912 / 54913 / 54914

- 影响：`< 2.4.5`
- 修复：2.4.5
- 类型：
  - 54912：fast interpreter heap buffer overflow
  - 54913：`load_init_expr` partial uninitialized memory read
  - 54914：WASI `poll_oneoff` heap buffer overflow
- 当前 2.4.5 **不受影响**
- 来源：[WAMR-2.4.5 release](https://github.com/wasm-micro-runtime/wasm-micro-runtime/releases/tag/WAMR-2.4.5)

### CVE-2025-64713

- 影响：`< 2.4.4`
- 修复：2.4.4
- 类型：fast interpreter 数组越界（CWE-119）
- 当前 2.4.5 **不受影响**
- 来源：[GHSA-gvx3-gg3x-rjcx](https://github.com/bytecodealliance/wasm-micro-runtime/security/advisories/GHSA-gvx3-gg3x-rjcx)

### CVE-2024-27532

- 影响：2.2.0（NULL pointer dereference）
- 当前 2.4.5 不在该版本
- 来源：[issue #3130](https://github.com/bytecodealliance/wasm-micro-runtime/issues/3130)

## 更新流程

1. 查看 [WAMR releases](https://github.com/bytecodealliance/wasm-micro-runtime/releases)
2. 查看 [GitHub Security Advisories](https://github.com/bytecodealliance/wasm-micro-runtime/security/advisories)
3. 更新 `security/wamr-cve-watch.json`
   - `reviewed_at` 改为当天
   - 添加/移除受影响 CVE
   - 写清 affected/fixed/source
4. 升级 `WAMR_VERSION`（如需要）
5. 跑：

```bash
python3 scripts/wamr-cve-gate.py
bash scripts/wasm-conformance.sh
bash scripts/wrun-gate.sh
```

## 限制

- 这是**人工复核的 watchlist**，不是实时 CVE feed
- CI 不联网查询 NVD/CIRCL；避免网络抖动
- 只覆盖已记录的 CVE；新 CVE 需要维护者复核
- 不覆盖 WAMR 之外的系统库（OpenSSL 由 `sbom` 记录）

## 为什么不用实时 API

CI 依赖外部 API 会引入：

- 网络抖动导致假失败
- 速率限制
- CVE 记录滞后/误报

因此采用“确定性门禁 + 定期人工复核”，并在 release 流程中归档
SBOM 与 watchlist。
