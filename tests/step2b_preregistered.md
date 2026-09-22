# Step 2b 预注册判据（HTTP 能力上 C 链）

2026-09-21 立项。仿前几份预注册：**先把验收判据写死，再改代码**。

## 背景（2026-09-21 实测）

`runtime.c` 只实现了 `@0/@1/@4/@5` 四个能力 id；HTTP 家族 `@2/@3/@6/@7/@8/@9`
在 C 链上会发出 `W__<n>()` 调用，而运行时没有这些函数——gcc 报
`implicit declaration of function 'W__2'`。实测六个 id 全是这个症状。

`docs/independence-design.md` 把 Step 2b 估为"中等工程"，理由是"需要一个
HTTP + TLS 客户端……用 C 重写是独立的中等工程"。**这条估算基于一个未核实的
前提**：本轮核实发现，VM 侧的 `capability.rs::http_request` 本身就
**只支持 `http://`**，HTTPS 是显式报错：

    "HTTPS is not yet supported. Use http:// URLs or enable the 'native-tls'
     feature. Plain TCP to port 443 cannot carry TLS traffic."

且实现只有约 80 行（TcpStream::connect + 手写请求行 + 读全文 +
`split("\r\n\r\n").nth(1)` 取 body）。因此"与 VM 对齐"的 C 侧实现**不含 TLS**，
量级从"中等工程"降到"补一个 HTTP/1.1 最小客户端"。TLS 缺口是 VM 与 C 链
**共同的**现状，不是 Step 2b 引入的。

## 判据（全部为机器可验的断言）

### S1 —— `@2` / `@3` 两链对拍

对一个**本地** HTTP/1.1 服务器（测试进程内起 `TcpListener`），下列程序
VM（`whisper run --allow-http`）与 `-t c2` 产物输出**逐行一致**：

| 用例 | 源码 | 期望 |
|------|------|------|
| H-1 GET | `"http://127.0.0.1:PORT/hi" @2 .` | 服务器回的 body |
| H-2 POST | `"http://127.0.0.1:PORT/echo" "{\"a\":1}" @3 .` | 服务器回的 body |
| H-3 GET 多行 | body 含换行的路径 | 原样 |
| H-4 带路径与查询 | `"http://127.0.0.1:PORT/a/b?c=d" @2 .` | 服务器看到的 path 一致 |

判据同时要求**服务器侧观察到的请求**两链一致：请求行（method + path）、
`Host:` 头、`Connection: close`、POST 的 `Content-Type: application/json`
与 `Content-Length`。

### S2 —— HTTPS 两链同样拒绝

`"https://example.com/" @2 .`：VM 报 `HTTPS is not yet supported…`；
C 链**不得**把明文发到 443 端口，必须以同样的语义失败（错误文本可比）。
这是安全判据：静默降级成明文是**不可接受**的。

### S3 —— 未实现的 id 显式报错，且不牵连只用到已实现 id 的程序

- 调用 `@6`/`@7`/`@8`/`@9` 的程序：**构建成功**，运行到该调用时以明确信息
  失败（指明 id、能力名、"C 链未实现、仅 VM 可用"、指向本文件），
  退出码非 0，而**不是** gcc 的 `implicit declaration`。
- 仅 `import "std/http"` 而只调用 `http-get` 的程序：**构建并运行成功**。
  （因此不能用编译期 `#error`：c2 会文本内联 `std/http.ws`，`@6` 令牌必然
  出现在编译单元里，编译期报错会把只用 `@2` 的程序一起打死。）

### S4 —— 既有门禁零回归

    cargo test --workspace       ≥ 476 / 0
    cargo clippy -D warnings     0
    cargo fmt --all --check      0
    syntax / stdlib              26 / 0 · 58 / 0
    typecheck-gate               FP=0
    opcode_coverage              PASS 不降（当前 80）
    net-gate / app-gate          PASS / PASS（13 路由）

### S5 —— 固定点复验

`scripts/verify-fixedpoint.sh` 报 `gen2 == gen3`。本战役**改 `runtime.c`**
（不是 `compiler.all.ws`），但 `runtime.c` 由生成的 C 直接 `#include`，
仍须复验固定点未被牵连。

## 反向判据（什么算失败，提前写明）

- 为让 S1 通过而把 HTTPS 静默当明文发（哪怕"两链一致"） → **失败**，
  这是安全降级。
- 为让 S3 通过而在编译期对 `@6..@9` 一律 `#error` → **失败**，
  它会让只用 `@2` 的程序也编不过（见 S3 第二条）。
- 为通过判据而放宽 VM 侧行为 → **失败**。

## 明确不在本战役范围

- **TLS/HTTPS**：VM 也没有。两链同样拒绝，缺口一致，另立项。
- `@6`/`@7`/`@8`/`@9` 的**实现**（会话 cookie 罐、审计日志、raw 通道、
  并行会话）：本轮只让它们**显式失败**。它们共用同一套 HTTP 核心，
  将来补实现时只需在其上加状态。
- C 链的**宿主白名单**：VM 侧 `--allow-http` 有 `allowed_hosts` 白名单，
  C 链目前对 `@0/@1/@5` 也没有白名单（`W__5` 直接 popen）。本轮不引入
  半套沙箱——要么整套做，要么不做，避免"看起来安全"。

## 执行记录

### 逐条对照

| 判据 | 结果 | 说明 |
|------|------|------|
| **S1** `@2`/`@3` 两链对拍 | ✅ | 新集成测试 `http_capabilities_match_across_chains`：进程内 `TcpListener` 起本地 HTTP/1.1 服务，4 例（GET / 带路径与查询 / POST / 再次 GET）VM 与 c2 输出相同；**服务器侧记录的请求逐字节相同**（请求行、`Host`、`Connection: close`、POST 的 `Content-Type` 与 `Content-Length: 7`） |
| **S2** HTTPS 两链同样拒绝 | ✅ | `https_is_refused_and_never_sent_as_plaintext`：两链均以 `HTTPS is not yet supported…` 失败，且**监听套接字在整个过程中没有收到任何连接**——拒绝发生在建连之前，不存在明文降级 |
| **S3** 未实现 id 显式报错且不牵连 | ✅ | 六个 id 现在**全部构建成功**；`@6`-`@9` 调用时打印 `capability @N (…) is not implemented on the C chain; it is VM-only for now…` 并退出；`import "std/http"` 且只用 `@2` 的程序照常构建运行 |
| **S4** 既有门禁零回归 | ✅ | cargo 479/0、clippy 0、fmt 0、syntax 26/0、stdlib 58/0、typecheck FP=0、coverage 80/0/0、net/app PASS |
| **S5** 固定点 gen2==gen3 | ✅ | 见 CHANGELOG |

### 顺带修掉的既有 bug：`@3` 在 VM 上根本用不了

写 S1 的 POST 用例时 VM 直接报 `http_post: expected URL and body`。根因：
`HttpPostCap` 没有覆写 `Capability::arity()`，默认值是 1，于是 `CapCall` 只弹
1 个操作数交给 `call()`，而 `call()` 要求 2 个——**怎么写在 VM 上都失败**。
`FileWriteCap`（`path content @1`）早就覆写成 2 了，HTTP POST 这一处漏了。

修法与它一致：`HttpPostCap::arity() -> 2`。这也顺带恢复了双链契约——C 侧的
`W__3` 本来就弹 2 个。

### 顺带修掉的 README 失真

README 原文把 `@2/@3` 与 `@6` 并列写成"支持 `https://`（rustls + webpki 根）"，
但 `capability.rs` 里 `@2/@3` 是**显式拒绝** HTTPS 的，真正走 rustls 的只有
`http_session.rs`（`@6` 家族）。已按令牌分档重写为可用性表格。

同时修掉 `@2/@3` 的拒绝信息——原文说 "or enable the 'native-tls' feature"，
而这个 cargo feature **不存在**，属误导。新文案指明：改用 `http://`，或改用
确实支持 TLS 的 `@6`。

### 实现要点

`whisperc/runtime.c` 新增（约 190 行）：

- `w_url_parse`：与 `capability.rs::parse_url` 同规则（端口从右切、path 恒以
  `/` 开头、默认 80/443）；
- `w_tcp_connect`：`getaddrinfo` + `connect`（运行时原本只有服务端
  listen/accept，没有客户端 connect）；
- `w_http_request`：请求行/`Host`/`Connection: close`/POST 头与 VM 逐字节一致；
- `w_http_read_all` + 首个 `\r\n\r\n` 之后取 body，对齐 VM 的
  `read_to_string` + `split("\r\n\r\n").nth(1)`；
- `W__2` / `W__3`；`W__6`-`W__9` 走 `w_cap_unimplemented()` → `w_fail()`
  （**可被 `try` 捕获**，与 VM 侧 `VmError` 的语义一致）。

**未做（如实登记）**：TLS。VM 的 `@2/@3` 也没有，两链同样拒绝；`@6` 家族的
C 侧实现（cookie 罐、审计、raw、并行）未做，调用时显式失败。C 链的宿主白名单
仍未引入——`@0/@1/@5` 本来也没有，不在这轮做半套沙箱。
