# Whisper 安全审计（1.3.15）

> 范围：native C 运行时、HTTP/HTTPS 能力、WASI 启动器、`.wbin` 解码器、
> 多租户边界。本文是项目自审，不是第三方认证。

## 1. 结论摘要

| 编号 | 问题 | 严重性 | 状态 |
|---|---|---|---|
| A-01 | `@6` cookie jar 跨 host 发送 | 高 | **已修复** |
| A-02 | HTTP method / URL / header 名可注入 CRLF 或空格 | 高 | **已修复** |
| A-03 | `.wbin` 长度/递归/目标未校验，可 OOM/栈溢出 | 高 | 已修复（1.3.14） |
| A-04 | `w_ci_eq_n` 字面量越界读 | 中 | 已修复（1.3.11） |
| A-05 | preopen 相对 guest 路径 / symlink 根未拒绝 | 中 | 已修复（1.3.13） |
| A-06 | 跨源重定向跟随会泄露凭据 | 高 | 设计上不跟随 |
| A-07 | 响应 header 暴露过多 | 低 | 已收敛为 Location/Content-Type |

本版新增修复 A-01/A-02，并补充回归测试。

## 2. 信任模型

| 执行体 | 信任 | 边界 |
|---|---|---|
| native C | 可信 | 无沙箱，依赖 OS/容器 |
| Rust VM | 可信工具 | 不是多租户边界 |
| WASM guest | 不可信 | WAMR + WASI + wrun 策略 |

native 程序拥有当前用户权限；**不要把不可信代码编译成 native 运行**。

## 3. A-01 cookie 跨 host

### 问题

`w_cookie_jar` 是进程级字符串，`w_jar_header()` 不检查 host，导致：

```text
GET https://a.example  (Set-Cookie: sid=secret)
GET https://b.example  (Cookie: sid=secret)   ← 泄露
```

### 修复

- cookie 改为 per-cookie 结构，记录 name/value/host/domain/path/secure/http_only
- `Domain` 校验：必须与 host 相同或是 host 的父域，且拒绝无点域/IP 的宽域
- `Path` 按 RFC 前缀规则匹配
- `Secure` 只通过 TLS 发送；HTTP 响应的 Secure cookie 被拒绝
- `HttpOnly` 记录（无 JS 运行时，不影响发送）
- 跨源重定向仍不跟随
- 锁保护 cookie 列表

### 残余

- `Max-Age`/`Expires` 已实现；`Max-Age=0` 删除
- PSL 只有保守的常见多标签后缀表，不是完整 Public Suffix List
- 最多 128 个 cookie；超出时拒绝新 cookie
- `@9` 并发写 cookie 有锁，但不同 host 的 cookie 会共存

## 4. A-02 请求注入

### 问题

请求行/header 直接由 `%s` 拼接：

```text
method  "GET\r\nX-Evil: 1"
url     "http://host/a b"
header  {"Bad Name":"x"}
```

未校验时会注入请求行或 header。

### 修复

- `w_method_valid`：只允许 RFC token 字符
- `w_url_parse`：拒绝 `<= 0x20`、DEL、空格、CR/LF
- `w_header_name_valid`：header 名只允许 token 字符
- header 值继续拒绝 CR/LF
- 错误在连接前返回

回归：

```text
net-allow-gate: invalid HTTP method
net-allow-gate: invalid header name
net-allow-gate: Invalid URL
```

## 5. A-03 `.wbin`

已修复：

- name/string/count 长度 ≤ 剩余输入
- 单次分配 ≤ 64 MiB
- PushRef 深度 ≤ 128
- v2 entry/jump/branch 目标 < block_count
- 4096 随机输入 property test

残余：

- 不是覆盖引导 fuzz
- 不覆盖 VM 执行阶段
- CLI 读取文件前没有总大小配额

## 6. A-05 preopen

已修复：

- guest 路径必须绝对
- 拒绝 `.`/`..`/`//`/`:`/换行
- 宿主 preopen 根/内部 symlink 拒绝
- userns 不可用 fail closed

残余：

- 检查与挂载之间 TOCTOU
- Windows junction/reparse point 未覆盖
- `find` 依赖

## 7. A-06 重定向

设计：`follow` 只跟随同源（scheme+host+port），最多 5 次；跨源返回原 3xx。

残余：

- 同源 307/308 会重放 body；调用方需确保 body 不含一次性机密
- 未实现跨源显式 opt-in

## 8. 其他检查

- TLS：动态 OpenSSL，证书校验开启；无 link-time 依赖
- 网络 allowlist 在连接前检查 host，私网/元数据地址默认拒绝
- raw socket 在 allowlist 模式默认关闭
- `@9` 线程共享 cookie/audit 有锁
- 响应 body 有 `WHISPER_NET_MAX_RESPONSE`，请求 body/header 有配额
- WASM import 只允许 `wasi_snapshot_preview1` + 函数白名单
- WASM 默认无网络/exec/FFI
- 多租户：独立进程/UID/cgroup；native 不是边界

## 9. 残余风险

| 风险 | 严重性 | 建议 |
|---|---|---|
| PSL 不完整（保守表） | 低 | 需要完整 cookie 语义时引入完整 PSL |
| preopen TOCTOU | 中 | 生产用不可变挂载/独立租户目录 |
| WAMR CVE | 高 | 固定版本 + reviewed watchlist CI（docs/wamr-cve.md） |
| 无 cgroup 聚合配额 | 中 | 部署层 cgroup v2 |
| WASM 侧信道 | 中 | 高价值租户独立节点 |
| 覆盖引导 fuzz 缺失 | 中 | 接 OSS-Fuzz/atheris |
| 自审非第三方 | — | 高风险场景做外部审计 |

## 10. 验证

```text
http6-gate: PASS
http-parity-gate: PASS
net-allow-gate: PASS
cargo test --workspace: pass
wbin unit tests: 10 passed
wrun-gate: 14/14
```

## 11. 复核清单

- [x] cookie 不跨 host
- [x] method/URL/header 名拒绝控制字符
- [x] `.wbin` 长度/递归/跳转校验
- [x] preopen 绝对路径 + symlink 拒绝
- [x] 跨源重定向不跟随
- [x] 网络 allowlist / 私网拒绝 / 配额
- [x] Cookie Domain/Path/Secure/HttpOnly
- [x] Cookie Max-Age/Expires
- [ ] 完整 PSL
- [ ] cgroup 聚合配额模板
- [ ] WAMR CVE 自动告警
- [ ] 外部安全审计
