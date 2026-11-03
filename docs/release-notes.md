# Whisper 1.3.21 发布说明

## 主题

**Fuzz 覆盖扩展到 cookie/PSL，并加入 nightly 长跑。**

## 新增

### libFuzzer 覆盖

同一输入现在还会进入：

- `Set-Cookie` 解析/匹配（`w_jar_update` / `w_jar_header`）
- Public Suffix List（`w_public_suffix`）

原有覆盖保持不变：

- 响应 headers
- chunked 解码
- 用户 headers JSON
- hex body
- URL 解析

### Nightly

`.github/workflows/fuzz-nightly.yml`：

- 每天 03:17 UTC
- HTTP/URL/cookie/PSL：`FUZZ_MAX_TIME=900`（15 分钟）
- WASM import mutation：5000 次
- 支持 `workflow_dispatch`

## 验证

```text
fuzz-gate: PASS
wasm-fuzz-gate: PASS
CI: 450346f / 新 commit 通过
```

## 兼容性

- 无运行时行为变化
- 仅扩展 fuzz harness 与 CI
- nightly 失败不会阻塞普通 push CI

## 已知限制

- nightly 仍是单机 15 分钟，不是 OSS-Fuzz 集群
- WASM import 仍是 mutation smoke，不是覆盖引导
- `.wbin` 只有 property/mutation fuzz
- cookie/PSL fuzz 输入没有专用 corpus

## 下一步

- OSS-Fuzz 集成评估
- `.wbin` 覆盖引导 fuzz target
- 外部安全审计
- 容器/EROFS fallback
