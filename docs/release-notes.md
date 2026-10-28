# Whisper 1.3.12 发布说明

## 主题

**WASM import 白名单 mutation smoke。**

## 新增

```bash
bash scripts/wasm-fuzz-gate.sh
WASM_FUZZ_ITERS=1000 bash scripts/wasm-fuzz-gate.sh
```

做三件事：

1. 构造合法 import section：
   `wasi_snapshot_preview1.fd_write`
2. 随机翻转/截断/插入/覆盖
3. 调用 `scripts/wasm-imports.py`

验收：

- 退出码只能是 0 或 1
- 2 秒内必须结束
- 崩溃、其他退出码、超时都失败

CI 默认 300 次。

## 验证

```text
wasm-fuzz-gate: PASS (300 mutations)
cargo check --workspace: pass
```

## 兼容性

- 无运行时行为变化
- 只新增 fuzz 工具与 CI 步骤
- WASM v1 仍无网络

## 已知限制

- 这是 mutation smoke，不是覆盖引导 fuzz
- 不覆盖 WAMR 内部
- 不覆盖 preopen/symlink 或 `.wbin` 解码
- 300 次是烟雾级

## 下一步

- preopen/symlink 路径 fuzz
- atheris/libFuzzer 覆盖引导 WASM import
- `wd` `.wbin` 解码 fuzz
- 完整安全审计
