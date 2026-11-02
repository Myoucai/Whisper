# Whisper 1.3.18 发布说明

## 主题

**Cookie 属性语义：Domain / Path / Secure / HttpOnly。**

## 新增

`@6` cookie jar 从单个字符串改为 per-cookie 结构：

- 每个 cookie 记录 name/value/domain/path/secure/http_only
- `Domain` 必须等于 host 或是 host 的父域
  - 拒绝无点域
  - 拒绝 IP 的宽域
  - 拒绝 `evil-example.com` 冒充 `example.com`
- `Path` 按 RFC 前缀规则匹配
- `Secure` 只通过 TLS 发送
- HTTP 响应里的 `Secure` cookie 被拒绝
- `HttpOnly` 记录（无 JS 运行时，不影响发送）
- 最多 128 个 cookie

## 测试

`http6-gate` 增加：

```text
cookie-path     ✅ Path=/echo 的 cookie 只在 /echo 发送
cookie-secure   ✅ Secure cookie 不通过 HTTP 发送
cookie-domain   ✅ 外部 Domain 被拒绝，合法 Domain 生效
```

完整结果：

```text
http6-gate: PASS (cookie/header/audit-json/raw/timeout/redirect follow)
http-parity-gate: PASS
net-allow-gate: PASS
```

## 兼容性

- 普通 host-only cookie 行为不变
- 以前跨 host 共享的 cookie 不再共享（安全修复）
- 以前忽略的 Domain/Path/Secure 现在生效
- 旧的无属性 cookie 默认 host-only、path=目录、非 secure

## 已知限制

- 未实现 `Max-Age`/`Expires` 过期删除
- 未实现完整公共后缀列表（PSL）
- 最多 128 个 cookie
- 无 JS 运行时，`HttpOnly` 只记录不产生额外效果

## 下一步

- Cookie Max-Age/Expires + PSL
- 覆盖引导 fuzz / nightly
- 外部安全审计
- 容器/EROFS fallback
