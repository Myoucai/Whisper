# Whisper 1.3.3 发布说明

## 主题

**native HTTP 响应 headers 与 VM 完全对齐；VM/C 对拍扩展到 header 和 Location。**

## 变更

C/native `@6/@8/@9` 响应 headers 现在只暴露：

- `Location`
- `Content-Type`

`Set-Cookie` 只进入内部 cookie jar，不再出现在 headers JSON。
`Date` / `Server` / `Transfer-Encoding` 等其他响应头被隐藏。

这与 VM 的选择一致，避免调用方依赖两条链不同的 header 集合。

## 对拍

`http-parity-gate` 新增：

- `@6` 响应 header map 对比
- `@6` 302 的 `Location` 对比
- `@9` 每项 headers 对比

结果：

```text
header parity: content-type
header parity: location
parallel status/body/header parity: 2 items
http-parity-gate: PASS
```

## 兼容性

- 依赖 `Date`/`Server`/`Transfer-Encoding` 等非 VM header 的程序会看到变化
- `Set-Cookie` 的 cookie jar 行为不变
- 未设置 `WHISPER_NET_ALLOW` 时网络兼容旧行为
- WASM v1 仍无网络

## 已知限制

- `WHISPER_NET_REDIRECT=follow` 未实现
- VM `@7` dummy 实参缺陷仍记录在案
- 多地址连接超时总时长仍按地址累加
- 严格只读 preopen 仅 Linux，需要 user namespace

## 验证

- integration tests: 123/123
- http6-gate: PASS
- http-parity-gate: PASS
- CI: `345144f` success
