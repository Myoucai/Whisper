# Whisper 1.3.0 发布说明

## 主题

**native HTTP(S) 会话 + 网络权限策略；WASM v1 继续保持无网络。**

## 新增

### native `@6` / `@7` / `@8`

- `@6`：HTTP/HTTPS 的 `status` / `headers` / `body`
- TLS：运行时动态加载 OpenSSL，不增加链接依赖
- 证书链 + hostname 校验；`WHISPER_TLS_CA` 可指定 CA
- SNI；不自动跟随重定向；chunked 自动解码
- cookie jar（`Set-Cookie` 收集并回带）
- 用户 headers（`headers-json`；拒绝 CR/LF 注入）
- `@7`：基础 `METHOD URL STATUS` 审计
- `@8`：hex 原始 body（二进制 POST）

### native 网络权限

- `WHISPER_NET=0`：整体关闭网络
- `WHISPER_NET_ALLOW=host1,*.example.com`：HTTP/HTTPS host 白名单
- allowlist 下默认拒绝 loopback/私网/link-local/云元数据
- `WHISPER_NET_ALLOW_PRIVATE=1`：显式放行本地测试
- `WHISPER_NET_DENY_PRIVATE=1`：无 allowlist 时也可拒绝私网
- `WHISPER_NET_METHODS=GET,POST`：方法限制
- `WHISPER_NET_PORTS=80,443`：端口限制
- `WHISPER_NET_MAX_RESPONSE=1048576`：响应字节上限

### WASM v1（不变）

- core conformance 27/27
- WASI 安全门禁 20/20
- `wd c` / `wd wasm` 零 Rust 入口
- preopen、env allowlist、symlink/import 拒绝
- `--dir-ro` 仍是快照，不是 EROFS
- **guest 无网络**；host HTTP proxy 未实现

## 兼容性

| 场景 | 1.2.0 | 1.3.0 |
|---|---|---|
| native 默认网络 | 开 | 开（兼容） |
| native `@6` HTTPS | 无 | 有（动态 OpenSSL） |
| `WHISPER_NET=0` | 有 | 有 |
| `WHISPER_NET_ALLOW` | 无 | 有 |
| WASM guest 网络 | 无 | 无 |

## 验收

```bash
cargo test --workspace
bash scripts/https-gate.sh
bash scripts/http6-gate.sh
bash scripts/net-allow-gate.sh
bash scripts/net-gate.sh
bash scripts/app-gate.sh
bash scripts/wasm-conformance.sh
bash scripts/wasm-security-gate.sh
bash scripts/wrun-gate.sh
bash scripts/wd-c-gate.sh
bash scripts/wd-wasm-gate.sh
```

## 已知限制

- `@9` parallel 未实现
- `@6`/`@7`/`@8` 与 VM 的完整语义对拍待补
- macOS TLS 运行时加载路径未验证
- 连接/读写超时、重定向策略、结构化审计仍待做
- 严格只读 preopen 需要 OS/容器层
- WASM 插件出网需要独立 host proxy

## 升级建议

- 可信 native 程序：默认行为兼容；需要收紧时使用 `WHISPER_NET_*`。
- 运行不可信 native 程序：至少 `WHISPER_NET=0`，仍建议 OS 隔离。
- 运行不可信 WASM：保持默认无网络。
