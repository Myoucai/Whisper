# Whisper 1.3.5 发布说明

## 主题

**native 资源配额：内存、请求数、请求/响应大小、并发连接。**

## 新增

### 内存

```bash
WHISPER_MAX_MEMORY_MB=128
```

等价于 `WHISPER_HEAP_MB`，覆盖 native/WASI heap 初始化。

### HTTP

```bash
WHISPER_NET_MAX_REQUESTS=1000
WHISPER_NET_MAX_BODY=1048576
WHISPER_NET_MAX_HEADERS=65536
WHISPER_NET_MAX_RESPONSE=8388608
WHISPER_NET_MAX_CONNECTIONS=8
```

- 请求计数覆盖 `@2/@3/@6/@8/@9`，重定向每一跳计数
- body 限制文本 body 和 `@8` 解码后的二进制 body
- header 限制请求用户 headers 和响应 header 区
- connection 限制 `@9` 单次并发
- 超限均返回明确错误，不 OOM/挂死

## 验证

```text
http6-gate:
  - response cap
  - header quota
  - request quota
http9-gate:
  - connection quota
net-allow-gate:
  - body quota
integration tests: 123/123
```

## 兼容性

- 未设置配额时保持旧行为
- 配额是进程级 opt-in
- 未设置 allowlist 时网络仍兼容
- WASM v1 仍无网络

## 已知限制

- 内存配额是 heap 上限，不是 RSS 硬隔离
- 文件描述符/线程总数尚未全局配额
- WASM CPU/文件数配额仍在后续
- VM `@7` dummy 实参缺陷仍记录
- 多地址连接超时仍逐个计时

## 升级建议

- 长生命周期服务设置 `WHISPER_NET_MAX_REQUESTS` 和 `MAX_RESPONSE`
- 处理不可信 body 时设置 `WHISPER_NET_MAX_BODY`
- `@9` 并发服务设置 `WHISPER_NET_MAX_CONNECTIONS`
