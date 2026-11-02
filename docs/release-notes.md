# Whisper 1.3.19 发布说明

## 主题

**Cookie 过期与公共后缀防护。**

## 新增

### Max-Age / Expires

- cookie 记录 `expires`
- `Max-Age` 优先于 `Expires`
- `Max-Age=0` 或负数删除同名/同域/同路径 cookie
- `Expires` 支持 RFC 1123 日期（`Thu, 01 Jan 1970 00:00:00 GMT`）
- 过期 cookie 不进入 `Cookie` 请求头
- 无过期时间的 cookie 仍是 session cookie

### 公共后缀防护

`Domain` 属性现在额外拒绝常见多标签 public suffix：

```text
co.uk, com.au, co.jp, com.cn, github.io, vercel.app, ...
```

这不是完整 PSL，但阻止了最常见的“把 public suffix 当 Domain”攻击。

## 测试

`http6-gate` 增加：

```text
cookie-maxage          ✅ Max-Age=3600 发送
cookie-maxage-delete   ✅ Max-Age=0 删除
cookie-expired         ✅ Expires=1970 不发送
```

完整结果：

```text
http6-gate: PASS
http-parity-gate: PASS
net-allow-gate: PASS
integration tests: 123/123
```

## 兼容性

- 原有 cookie 行为不变
- 带 Max-Age/Expires 的 cookie 现在会过期
- `Domain=co.uk` 等 public suffix 被拒绝
- 无过期时间的 cookie 仍为 session

## 已知限制

- PSL 是保守的常见后缀表，不是完整 Public Suffix List
- Expires 解析覆盖 RFC 1123 常见格式，不覆盖所有历史格式
- 最多 128 个 cookie
- `HttpOnly` 仍只记录（无 JS 运行时）

## 下一步

- 完整 PSL
- 覆盖引导 fuzz / nightly
- 外部安全审计
- 容器/EROFS fallback
