# Whisper 1.3.4 发布说明

## 主题

**native 同源重定向跟随：最多 5 次，严格方法/header 规则，跨源不跟随。**

## 新增

`WHISPER_NET_REDIRECT=follow`：

- 只跟随同源（scheme + host + port）重定向
- 最多 5 次，超过明确失败
- 301/302/303：
  - 改为 GET
  - 丢弃原 headers/body
- 307/308：
  - 保留 method 和 body
- 跨源重定向：
  - 不跟随
  - 原样返回 3xx
  - 不转发 Cookie / Authorization
- 每一跳重新经过 allowlist、私网、方法、端口、超时策略
- @6/@8/@9 均使用同一跟随逻辑

手动/拒绝模式不变：

```bash
WHISPER_NET_REDIRECT=manual   # 默认，返回 3xx
WHISPER_NET_REDIRECT=deny     # 拒绝带 Location 的 3xx
```

## 测试

`http6-gate` 覆盖：

- `/redirect` → `/set`：follow 得到 200
- `/loop`：follow 触发 redirect limit
- `/cross`：跨源不跟随，保留 302
- deny / manual 行为不变

结果：

```text
http6-gate: PASS (cookie/header/audit-json/raw/timeout/redirect follow)
```

## 兼容性

- 默认仍是 manual，不改变旧行为
- follow 是 opt-in
- 跨源不跟随是安全边界，不是 bug
- WASM v1 仍无网络

## 已知限制

- 不跟随跨源重定向
- `Location` 只支持绝对 URL、`/path`、`//host/path` 和相对路径
- 连接超时仍按地址逐个计时
- VM `@7` dummy 实参缺陷仍记录

## 验证

- integration tests: 123/123
- http6-gate: PASS
- http-parity-gate: PASS
- CI: `5a411d5` success
