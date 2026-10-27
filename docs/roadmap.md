# Whisper 后续路线图

2026-10-22。基线：CI 全绿；`v1.1.0` 已发布；`main` 为 **1.2.0（未发布）**，
包含 native `WHISPER_NET` 开关。本文件把“还没做的事”按可验收的阶段固定下来，
避免继续零散救火。

## 0. 两条产品路线

Whisper 同时有两类使用者，优先级不同的地方必须明确：

| 路线 | 使用者 | 主目标 |
|---|---|---|
| **A. 作者路线** | 开发者写自己的程序 | native 功能完整、工具链好用 |
| **B. 平台路线** | 平台运行别人的代码 | WASM 隔离、权限、配额 |

**不要把两条路线的需求混在一个版本里。**

## P0 — 1.2.0 发布收口

目标：把已完成的 native 网络开关稳定发布。

- [x] `docs/release-notes.md` 定稿（1.2.0）
- [x] `WHISPER_NET=0` 文档、help、测试一致
- [x] 全量验收：cargo test / integration / app / net / wasm gates
- [x] release workflow 使用 1.2.0 release notes
- [x] 打 tag `v1.2.0` 并确认 GitHub Release

验收：`WHISPER_NET=0` 显式拒绝所有网络 opcode；默认行为不变；
native/HTTP server 测试全绿；WASM 无网络不变。

## P1 — native HTTPS（作者路线最高优先）

问题：`@2/@3` 只支持 `http://`；`@6` 的 HTTPS 只在 VM，C/native 未实现。
真实应用几乎都需要 HTTPS。

设计见 [`native-https-plan.md`](native-https-plan.md)。

- [x] 选型文档：推荐 vendored mbedTLS；OpenSSL 仅开发备选
- [x] 最小实现：C 链 `@6` HTTP/HTTPS（动态 OpenSSL、chunked、CA 校验）
- [x] cookie jar / 用户 headers / 基础审计（`@7`）
- [x] `@8` raw hex body
- [x] `@6/@7/@8` 输出 + `@9` status/body/header + 302/Location 对拍
  - C 链 headers 只暴露 Location/Content-Type，与 VM 对齐
- [x] `@9` parallel（1..16 并发、按请求序聚合）
- [x] macOS HTTPS gate（Homebrew OpenSSL + CI runner）
- [ ] cookie/audit 与 VM 的完整语义对拍
- [ ] `@2/@3` 是否升级为 HTTPS，或继续显式拒绝
- [ ] VM/C 链行为对拍
- [ ] 负向测试：证书错误、过期、hostname mismatch、TLS 降级

验收：native 程序能访问受信任 HTTPS API；失败可定位；不静默降级到明文。

## P2 — native 网络权限细化

当前 `WHISPER_NET=0` 是整体开关，没有按目标授权。

- [x] `WHISPER_NET_ALLOW=host1,*.example.com` 域名白名单（HTTP/HTTPS 建连前）
- [x] allowlist 下默认拒绝私网/loopback/link-local/云元数据；
  `WHISPER_NET_ALLOW_PRIVATE=1` 显式放行本地测试
- [x] 方法、端口、响应大小限制（`WHISPER_NET_METHODS`/`PORTS`/`MAX_RESPONSE`）
- [x] 连接/读写超时（`WHISPER_NET_TIMEOUT`，默认 30s）
- [x] 重定向策略（manual / deny / follow；同源最多 5 次）
- [x] 结构化审计日志（`WHISPER_NET_AUDIT=json`，host/status/bytes）
- [x] 与 `@2/@3/@6` HTTP 策略对齐；raw `listen/accept/recv/send` 增加
  `WHISPER_NET_RAW`/`WHISPER_NET_LISTEN_PORTS`

验收：未授权目标在连接前拒绝；DNS rebinding/重定向不能逃逸；日志可追踪。

## P3 — WASM host HTTP proxy（仅平台路线）

只有“用户上传的插件需要联网”时才做；不要为了爬虫给 raw socket。

- [ ] `whisper:net` ABI：命名服务，不传 URL
- [ ] 独立 broker 进程 / 独立 UID / netns
- [ ] 域名/方法/path/大小/超时/重定向策略
- [ ] secrets 注入，guest 不可见
- [ ] SSRF/DNS rebinding/重定向/header 注入测试
- [ ] `wrun --net` 开关（默认 off）

验收：guest 不能开 raw socket；只能调用授权服务；越权请求在 broker 拒绝。

## P4 — 严格沙箱与资源边界

- [x] `--dir-ro-strict`：Linux 私有 user/mount namespace + ro bind mount
- [ ] 容器/EROFS/无特权环境 fallback 的严格只读方案
- [x] native 内存/HTTP 请求/body/header/@9 连接配额
- [x] WASM heap/stack/threads/wall-clock 超时配额
- [ ] WASM 文件数/连接数配额
- [ ] WAMR 版本升级策略与 CVE 跟踪
- [ ] 多租户隔离评审

验收：`--dir-ro` 不再只是快照；资源耗尽只影响单个 guest。

## P5 — 工具链与生态（作者路线）

- [ ] 包管理器（依赖、锁文件、发布）
- [ ] REPL / `check` / LSP 的 Whisper 化或明确边界
- [ ] 安装器 / 跨平台分发
- [ ] 标准库补全与真实项目样例
- [ ] 调试信息/错误定位

验收：新用户不依赖 Rust 工具即可完成“安装 → 写 → 跑 → 打包”。

## P6 — 质量与性能

- [ ] WASM AOT/wasm2c 性能路线评估
- [ ] HTTP/WASM/preopen fuzz
- [ ] 安全审计与依赖 SBOM
- [ ] CI 零 Rust `wd` 全链 nightly 稳定（已有 workflow）
- [ ] macOS/Linux/Windows 发布矩阵

## 明确不做

- 给不可信 WASM guest 开 raw socket
- 把 `--dir-ro` 快照宣传成 EROFS
- 为了“功能完整”把 host proxy 塞进 v1
- 在 native 上假装多租户安全
- 同时铺开 A/B 两条路线

## 决策门

1. 产品主要是“作者路线”还是“平台路线”？
2. 平台是否真的需要 guest 出网？
3. 是否接受引入 TLS 依赖（体积/构建复杂度）？

答案决定 P1/P3 的顺序；在回答之前，默认先做 **P0 + P1**。
