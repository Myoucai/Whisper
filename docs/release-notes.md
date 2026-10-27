# Whisper 1.3.1 发布说明

## 主题

**native HTTP 会话补齐：并发、超时、重定向策略、结构化审计、macOS 与 raw socket 边界。**

## 新增

### `@9` parallel

- 同一请求 1..16 份并发
- 共享 cookie jar / audit，带锁
- 按请求序聚合 JSON
- 错误传播与超时

### 超时

- `WHISPER_NET_TIMEOUT` 连接/读写超时
- 默认 30 秒，0 关闭
- 超时明确报错，不再无限挂起

### 重定向策略

- `manual`（默认）：原样返回 3xx
- `deny`：拒绝带 Location 的 3xx
- `follow`：显式未实现，不静默跟随

### 结构化审计

`WHISPER_NET_AUDIT=json` 时，`@7` 返回：

```json
[
  {
    "method": "GET",
    "url": "http://127.0.0.1:18081/set",
    "host": "127.0.0.1",
    "status": 200,
    "bytes": 3
  }
]
```

### raw socket 边界

- `WHISPER_NET_RAW=0` 禁用 `listen/accept/recv/send`
- allowlist 模式默认关闭 raw
- `WHISPER_NET_RAW=1` 显式开启
- `WHISPER_NET_LISTEN_PORTS` 限制监听端口

### macOS

- macOS CI 使用 Homebrew `openssl@3`
- 绝对 libssl 路径 + `RTLD_LOCAL`
- 自签 CA / 未信任 CA / hostname mismatch 三项门禁通过

### VM/C 对拍

- `@6/@7/@8` 输出
- `@9` status/body
- 302 manual 状态
- 已知 VM `@7` 需要 dummy 实参（typechecker 声明 arity 0）

## 兼容性

- native 默认网络仍开启
- 未设置 `WHISPER_NET_ALLOW` 时 raw socket 兼容旧行为
- 配置 allowlist 后 raw 默认关闭（安全收紧）
- WASM v1 仍无网络

## 已知限制

- 严格只读 preopen 仍需 OS/容器层；`--dir-ro` 仍是快照
- 完整 header 语义对拍仍待补
- `WHISPER_NET_REDIRECT=follow` 未实现
- 结构化审计是 C/native 扩展，VM 仍返回文本行
- 连接超时是每个地址单独计时，多地址时总耗时可能累加

## 升级建议

- 可信 native 程序：默认兼容
- 收紧 HTTP：`WHISPER_NET_ALLOW` + 私网默认拒绝
- 需要 raw socket：显式 `WHISPER_NET_RAW=1`
- 运行不可信 WASM：保持默认无网络
