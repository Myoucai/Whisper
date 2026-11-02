# Whisper 1.3.15 发布说明

## 主题

**安全自审：cookie 跨 host 泄露、HTTP 请求注入加固。**

## 修复

### A-01 cookie 跨 host

此前 `@6` cookie jar 是进程级字符串，发往任意 host。现在：

- jar 记录所属 host
- `Cookie` 仅发给同一 host
- host 变化且新响应带 `Set-Cookie` 时清空旧 jar
- 跨源重定向仍不跟随

残余：未实现 `Domain`/`Path`/`Secure`/`HttpOnly` 语义。

### A-02 请求注入

- method 只允许 RFC token 字符
- URL 拒绝空格、CR/LF、控制字符
- header 名只允许 token 字符
- header 值继续拒绝 CR/LF
- 错误在连接前返回

回归：

```text
net-allow-gate: invalid HTTP method
net-allow-gate: invalid header name
net-allow-gate: Invalid URL
```

## 新增文档

[docs/security-audit.md](security-audit.md)：

- 信任模型
- A-01..A-07 发现与状态
- 残余风险与复核清单
- 验证命令

结论：

- native 不是多租户边界
- WASM 仍依赖 WAMR/WASI/wrun 策略
- 仍需外部审计与 cgroup 模板

## 验证

```text
http6-gate: PASS
http-parity-gate: PASS
net-allow-gate: PASS
integration tests: 123/123
```

## 兼容性

- 正常同 host cookie 行为不变
- 跨 host 不再发送 cookie；这是安全修复
- 非法 method/URL/header 从“可能拼接”改为明确拒绝

## 已知限制

- Cookie Domain/Path/Secure/HttpOnly 未实现
- 一个进程同时只保留一个 host 的 jar
- `@9` 跨 host 并发会安全地丢弃/重置 cookie
- preopen TOCTOU、WAMR CVE、cgroup 仍待处理
- 本文是自审，不是第三方认证
