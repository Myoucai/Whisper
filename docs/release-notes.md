# Whisper 1.3.6 发布说明

## 主题

**WASI 资源配额：heap、stack、线程数和 wall-clock 超时。**

## 新增

`wrun` 支持：

```bash
WHISPER_WASM_MAX_MEMORY_MB=64 \
WHISPER_WASM_STACK_KB=256 \
WHISPER_WASM_MAX_THREADS=4 \
WHISPER_WASM_TIMEOUT=30 \
bash scripts/wrun.sh app.wasm
```

映射：

- `WHISPER_WASM_MAX_MEMORY_MB` → WAMR `--heap-size`
- `WHISPER_WASM_STACK_KB` → WAMR `--stack-size`
- `WHISPER_WASM_MAX_THREADS` → WAMR `--max-threads`
- `WHISPER_WASM_TIMEOUT` → `timeout` wall-clock 上限

安全语义：

- 数值只接受整数并检查范围
- 缺少 `timeout` 命令时，设置 `WHISPER_WASM_TIMEOUT` 会 fail closed
- 与 `--dir-ro-strict` 可同时使用
- 与 preopen/env/import 白名单叠加

## 测试

`wrun-gate` 新增 W-9：

- 设置 heap/stack/threads 配额后运行 hello
- 输出仍为 `Hello, World!`

结果：

```text
W-9 ✅ WASM heap/stack/threads 配额参数接受
wrun-gate: 9/9
```

## 兼容性

- 未设置配额时保持旧行为
- 配额是 host launcher 的 opt-in
- WASM v1 仍无网络
- `--dir-ro` 仍是快照；`--dir-ro-strict` 才是 Linux EROFS

## 已知限制

- heap/stack 是 WAMR 参数上限，不是 cgroup RSS 隔离
- 文件数/连接数配额仍需 OS/容器层
- `timeout` 是 wall-clock，不是 CPU 时间
- macOS 默认缺少 GNU `timeout`，设置超时会明确失败

## 验证

- wrun-gate: 9/9
- WASM CI: success
- integration tests: 123/123
