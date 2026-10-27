# Whisper 1.3.8 发布说明

## 主题

**native 多地址连接共享超时预算。**

## 修复

此前 `WHISPER_NET_TIMEOUT` 按每个候选地址单独计时；一个主机解析出
多个地址时，总耗时可能达到 `N × timeout`。

现在连接在所有候选地址之间共享一个总预算：

```bash
WHISPER_NET_TIMEOUT=10 ./app
```

- 总预算固定为 10 秒
- 地址之间不会重置计时
- 预算耗尽立即停止尝试
- 错误明确为：

```text
Connect to host:port timed out
```

DNS 解析仍可能占用预算；解析完成后剩余时间才是连接可用时间。

## 验证

```text
不可路由地址 + timeout=1:
  实际约 1.1s 返回 Connect ... timed out
http6-gate: PASS
http-parity-gate: PASS
integration tests: 123/123
```

## 兼容性

- 未设置 `WHISPER_NET_TIMEOUT` 时仍默认 30s
- `0` 仍关闭超时
- 读写超时不受影响
- WASM v1 仍无网络

## 已知限制

- DNS 解析本身不能被中断；预算从解析前开始计算
- 不包含 TLS handshake 的独立预算（受连接/读写超时影响）
- WASM 文件数/连接数配额仍需 OS/容器层

## 升级建议

- 多 A/AAAA 记录的服务建议显式设置 `WHISPER_NET_TIMEOUT`
- 需要更严格的总请求预算时叠加 `WHISPER_NET_MAX_REQUESTS`
