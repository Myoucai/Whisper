# native HTTPS 设计（P1）

状态：**C 链 `@6` 基本完成（2026-10-23）**。支持 HTTP/HTTPS 的
status/headers/body、cookie jar、用户 headers、chunked 解码和基础
`@7` 审计；TLS 通过运行时动态加载 OpenSSL，不增加链接依赖。
`@8`/`@9` 未实现；完整 VM/C 对拍和 macOS 验证待补。

目标：让 C/native 目标实现与 VM 一致的 `@6` HTTPS，同时不把不可信
WASM 的网络边界重新打开。

## 1. 需求

- `@6`（http-req）在 C 链可用：`method url headers-json body -> status headers-json body`
- `https://`：证书链校验、hostname 校验、SNI、ALPN http/1.1
- 不静默降级到明文；HTTPS 失败必须显式
- 不自动跟随重定向（与 VM 一致，302 原样返回 Location）
- cookie jar / audit 的行为与 VM 对齐
- `WHISPER_NET=0` 同样覆盖 HTTPS
- Windows / Linux / macOS 可构建；不引入 Rust runtime

## 2. TLS 选型

| 方案 | 优点 | 缺点 | 结论 |
|---|---|---|---|
| 系统 OpenSSL | 成熟、本机已有 | Windows/macOS 需额外开发库；用户安装负担 | 备选 |
| 静态 vendored mbedTLS | 无系统依赖、跨平台、体积可控 | 仓库增加源码、需维护升级 | **推荐** |
| libtls/LibreSSL | API 简单 | Windows 支持差 | 否 |
| 平台 TLS（SChannel/SecureTransport/OpenSSL） | 无第三方库 | 三套后端，测试矩阵大 | 远期 |

**首次实现（2026-10-23）**：采用**运行时动态加载 OpenSSL**，因此不增加
链接依赖；libssl 缺失时 `@6` 显式失败。vendored mbedTLS 仍是后续的
可复现/免系统依赖方案，尚未替换。
| libcurl | 功能全 | 依赖/证书/构建复杂；容易变成另一个网络栈 | 否 |
| 调 `curl` 子进程 | 快速原型 | 进程/临时文件/注入面，不可作为安全实现 | 仅原型，不发布 |

推荐：**vendored mbedTLS 静态链接**，先实现最小 TLS 客户端；
OpenSSL 作为开发机可选后端，不进入默认发布。

## 3. C 侧接口

```c
typedef struct w_tls w_tls;
w_tls *w_tls_connect(const char *host, int port, char *err, size_t errlen);
int   w_tls_write(w_tls *, const void *, size_t);
int   w_tls_read(w_tls *, void *, size_t);
void  w_tls_close(w_tls *);
```

- 只允许 TLS 1.2/1.3
- 默认系统/内置根证书；支持 `WHISPER_TLS_CA` 覆盖（测试）
- hostname 校验必须开启；禁止 `VERIFY_NONE`
- 连接/读写超时与 `@2/@3` 一致
- `WHISPER_NET=0` 在 `w_http_request` 入口已拒绝

## 4. @6 响应解析

需要新增：

- 读取状态行：`HTTP/1.1 <status>`
- 读取 headers，返回 JSON object（至少 content-type/location/set-cookie）
- `Set-Cookie` 收集到进程级 cookie jar
- body 按 `Content-Length`/chunked/EOF 读取
- 无响应返回 `(0, "{}", "")`，与 VM 一致
- 审计：`METHOD URL STATUS`，供 `@7` 导出

## 5. 构建接入

- 生成 C 含 `#include "runtime.c"`；TLS 实现作为可选的
  `whisperc/tls_mbedtls.c` / `tls_openssl.c`
- c2/wd 构建命令在检测到 `@6` 时才追加 TLS 源文件和库
- 无 TLS 依赖时，`@6` 仍显式报错，而不是链接失败
- CI：Linux 跑 mbedTLS 主路径；Windows/macOS 至少编译 + 证书负向测试

## 6. 测试矩阵

正向：

- 本地 HTTPS server（自签 CA）GET/POST
- 状态码/headers/body
- cookie 往返
- SNI / hostname 正确

负向：

- 自签但未受信任 → 失败
- 过期证书 → 失败
- hostname mismatch → 失败
- TLS 版本过低 → 失败
- `https://` 绝不降级到明文（监听 443 不得收到请求）
- 重定向不自动跟随
- `WHISPER_NET=0` 下 HTTPS 拒绝

## 7. 分阶段

1. 选型定稿 + vendor mbedTLS：选型完成，动态 OpenSSL 已先落地；mbedTLS 待定
2. `w_tls_*` + 本地 HTTPS 往返：动态 OpenSSL 版完成
3. `@6` 状态/headers/body/cookie/audit：完成（`@7` 基础审计）
4. VM/C 对拍 + 负向测试：HTTPS 门禁 3/3；完整对拍待补
5. Windows/macOS 构建矩阵：Windows 实测；Linux CI；macOS 待验证

## 8. 决策门

- 是否接受仓库 vendored mbedTLS（体积/升级责任）？
- 是否要求 macOS 默认无 Homebrew 依赖？
- `@2/@3` 是否也升级为 HTTPS，还是只让 `@6` 支持？

默认答复：接受 vendored mbedTLS；macOS 无 Homebrew；`@2/@3` 维持
明文限制，`@6` 是 HTTPS 入口。
