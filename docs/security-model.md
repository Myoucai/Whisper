# Whisper 安全模型（2026-10-20）

本文档定义 Whisper 的**威胁模型、信任边界、当前执行缺口与整改不变量**。
它是所有“能力沙箱 / 零信任 / 原生安全”表述的权威依据；README、spec、
stdlib 文档里的安全承诺以本文为准。

> **一句话结论**：当前只有 Rust VM 有 `CapabilityTable` 的**框架**，但
> `read-file`/`write-file`/`getenv`/`ffi-*` 等 ambient opcode 绕过它；
> 原生 C 产物完全没有语言级授权。**WASM/WASI 是已选定的原生安全目标**，
> 见 `docs/wasm-wasi-plan.md`。WASM 已到 M2 部分（安全门禁 20/20），
> 但 host HTTP 代理未完成，**尚不能作为生产边界**。

---

## 1. 威胁模型

| 编号 | 攻击者/场景 | 目标 |
|------|-------------|------|
| T1 | 不可信的 `.ws` 源码 | 不能越权读写、联网、执行命令、加载 native 库 |
| T2 | 不可信的 `.wbin` / 字节码 | 同上；且不能靠畸形字节码破坏宿主 |
| T3 | 被篡改/替换的 native 产物 | 即使 binary 被控制，也出不了 OS 沙箱 |
| T4 | 恶意包 / 恶意 native 轮子 | 安装/加载阶段不能获得未授权能力 |
| T5 | 编译器/工具链供应链 | 产出可复现、可审计；构建不静默执行用户代码 |
| — | 已攻破的 OS/内核/虚拟化层、侧信道 | **不在语言安全模型内**（由 OS/硬件负责） |

“原生安全”在本项目里指 **T1 + T3 的组合**：源码不能越权，binary 被攻破
也出不了宿主沙箱。**仅靠 C runtime 只能做到 T1 的一部分，做不到 T3。**

---

## 2. 信任边界（现状）

| 执行方式 | 产物 | 能力授权 | 当前是否可作为不可信代码边界 |
|----------|------|----------|------------------------------|
| `whisper run` | Rust VM 解释字节码 | `CapabilityTable`（`@n`） | **否**：ambient opcode/FFI 绕过 |
| `-t bundle` | Rust VM + `.wbin` 单文件 | 同上 | **否**：同上；且内含完整 VM |
| `-t c` / `-t c2` / `-t exe` / `-t native` | C → 原生 exe | **无** | **否**：只有 OS 用户权限 |
| `wd build/run` | C → 原生 exe | **无**；构建期还需 gcc/执行 | **否**：构建/运行都不是沙箱 |
| wasm M2（`wd wasm` / `scripts/build-wasm.sh`） | `.wasm` + WAMR | WASI preopen + import allowlist + ro snapshot | **尚未**：文件/import/ro 负向通过；host 代理未完成 |

**原生产物是 trusted-only**：它适合“自己写的、已审阅的程序”，不适合
“用户上传的、LLM 生成的、来源不明的程序”。在 WASM v1 的负向门禁完成前，
不可信代码不应走 `exe`/`c2`/`native`/`wd`。

---

## 3. 效果分类与执行缺口

“ambient opcode”指不经过 `CapabilityTable`、直接调用宿主的词。它们和
`@n` 是两套不同的路径。

| 效果 | 词/opcode | Rust VM 当前 | C 链当前 |
|------|-----------|--------------|----------|
| 文件读 `@0` | `@0` | 有表；未绑定拒绝 | 直接 `fopen`，无检查 |
| 文件写 `@1` | `@1` | 有表；未绑定拒绝 | 直接 `fopen`，无检查 |
| HTTP GET/POST `@2/@3` | `@2/@3` | 有表；未绑定拒绝 | 直接 socket，无检查 |
| 环境变量 `@4` | `@4` | `--allow-env` 才绑定 | 直接 `getenv` |
| 执行 `@5` | `@5` | `--allow-exec` 才绑定 | 直接 `popen` |
| 执行（原样流）`@10` | `@10` | 与 `@5` 同绑 | 直接 `popen` |
| 会话 HTTP `@6`-`@9` | `@6`-`@9` | 有表 | 显式“未实现”退出 |
| 文件读（builtin） | `read-file` | **直接读，绕过表** | 直接 `fopen` |
| 文件写（builtin） | `write-file` | **直接写，绕过表** | 直接 `fopen` |
| 环境变量（builtin） | `getenv` | **直接读，绕过表** | 直接 `getenv` |
| 目录/存在性 | `listdir`/`mkdir`/`file-exists` | **直接调用** | 直接调用 |
| 监听/接受/收发 | `listen`/`accept`/`recv`/`send`/`serve-concurrent` | **直接调用** | 直接调用 |
| FFI | `ffi-open`/`ffi-sym`/`ffi-call` | **直接 `dlopen`/调用** | 直接 `dlopen`/调用 |
| 标准输入 | `read-line`/`,` | 直接 stdin | 直接 stdin |
| 时间/随机 | `now`/`sleep`/`random`/`seed!` | 直接调用 | 直接/部分 |
| 进程内 KV | `kv-*` | 进程内 | 进程内 |
| 输出/退出/参数 | `.`/`print`/`exit`/`args` | 不涉及宿主授权 | 同 |

结论：

- **`@n` 不是唯一入口**。只给 `@0` 加检查而不管 `read-file`，等于没加。
- **FFI 是最高危的洞**：它可以绕开所有路径/host 检查，直接调用
  `system`、`CreateProcess`、`dlopen` 等。
- **C 链连 `@n` 的检查都没有**；`@n` 只是“直接调用同名 C 函数”。

---

## 4. 实测证据（2026-10-20）

### 4.1 VM：`@0` 被拒，但 ambient 与 FFI 不拒

```whisper
"README.md" @0 strlen .
# → Runtime error: Capability not bound: @0

"README.md" read-file strlen .
# → 21510            （无任何 allow-* flag）

"PATH" getenv strlen .
# → 621              （无 --allow-env）

"msvcrt.dll" ffi-open .
# → 140734333911040  （无任何 allow-ffi 开关）
```

### 4.2 C 产物：直接读写/执行

```whisper
"README.md" @0 strlen .        # 编成 c2 后 → 21510
"echo cap-ok" @5 1 @nth .     # 编成 c2 后 → cap-ok
```

对应实现：`whisperc/runtime.c` 的 `W__0`…`W__5` 直接
`fopen`/`popen`/`getenv`，没有 policy 参数，也没有 grant 表。

### 4.3 文档与实现不一致（整改前）

- `docs/spec.md` §4 原写“C 链能力 `@0-@5` = no-op”，实际是**无检查直通**；
- `docs/stdlib.md` 的 `std/io` 原写“`read-file`/`write-file` 需要
  `--allow-file-read/write`”，实际这两个是 native opcode，flag 不拦；
- `README.md` 的“能力沙箱”没有限定“仅 VM/部分”，容易被理解为所有产物都安全。

这些已在 2026-10-20 的文档整改中修正（见对应文件的“安全”章节）。

---

## 5. 关键发现与严重度

| 编号 | 发现 | 严重度 | 备注 |
|------|------|--------|------|
| F-1 | `ffi-*` 在 VM 与 C 链均无授权门 | **Critical** | 可加载任意 native 库并调用任意函数 |
| F-2 | `read-file`/`write-file`/`getenv` 等 ambient opcode 绕过 `CapabilityTable` | **High** | VM 的“默认无 IO”不成立 |
| F-3 | C 链 `@0`-`@5`/`@10` 无任何检查 | **High** | 原生产物=当前用户权限 |
| F-4 | `listen`/`accept`/`serve-concurrent` 等网络 opcode 无逐主机授权 | **High** | native 可用 `WHISPER_NET=0` 整体关闭；`WHISPER_NET_ALLOW` 只覆盖 HTTP/HTTPS；WASM 恒无网络 |
| F-5 | 没有“负向安全测试”；现有能力测试只测正向/ABI | **Medium** | 漏洞不会被 CI 发现 |
| F-6 | spec/stdlib/README 与实现不一致 | **Medium** | 会误导安全评估 |
| F-7 | `wd` 构建期调用 gcc 并执行产物 | **Medium** | 编译不可信源码本身是特权操作 |
| F-8 | C 产物无 OS 沙箱、无 CFI/加固 | **High** | 内存破坏可越权 |
| F-9 | WAMR preopen 会跟随 symlink 逃逸到目录外 | **High** | 裸 `iwasm --map-dir` 不可用于不可信代码；`wrun` 拒绝含 symlink 的 preopen（S-17） |
| F-10 | 未知/非 WASI import 只会等到调用时才失败 | **Medium** | `wrun` 加载前检查 import 表，只允许 `wasi_snapshot_preview1`（S-18） |
| F-11 | WAMR 的 WASI socket 扩展也挂在 `wasi_snapshot_preview1` 下 | **High** | import 检查同时限制函数名，`sock_*` 不在白名单（S-20） |

---

## 6. 目标不变量（整改验收标准）

未来任何执行目标都必须满足：

- **I-1 默认拒绝**：不可信/WASM profile 没有显式 policy 时，文件/网络/
  环境/执行/FFI 全部拒绝。trusted native 保持历史默认开启，但可用
  `WHISPER_NET=0` 整体关闭网络。
- **I-2 无 ambient authority**：所有宿主效果都经过同一个 host ABI；
  不允许“某个 builtin 直接 `fopen`”这种旁路。
- **I-3 最小权限**：文件用 preopen/句柄，网络用 host/proxy，执行用 argv
  白名单且不走 shell，FFI 不可用于不可信 profile。
- **I-4 策略在 guest 之外**：policy 由启动器/宿主持有，不能由 `.wasm`
  或 `.ws` 自己声明后放宽。
- **I-5 负向测试**：每个效果至少一条“无授权必须失败”的测试，且进 CI。
- **I-6 文档即边界**：README/spec/stdlib 的每个安全承诺都能对应到实现和测试。
- **I-7 原生 trusted-only**：`exe/c2/native/wd` 不承诺语言级沙箱；
  不可信代码只允许 WASM（目标）或修复后的 VM。
- **I-8 路径规范化**：拒绝绝对路径、`..`、symlink 逃逸；WASI 用 preopen，
  native 用 `openat`/`RESOLVE_BENEATH`（若做 native policy）。
  WAMR 会跟随 symlink；`wrun` 拒绝含 symlink 的 preopen（S-17），
  裸 `iwasm` 不满足此不变量。
- **I-9 可审计**：策略、拒绝原因、允许的 host 调用可记录；默认不记录敏感内容。
- **I-10 显式 unsupported**：WASM 目标不支持的词必须编译期或运行期显式报错，
  不能静默变成 no-op。
- **I-11 import 白名单**：guest 只能 import `wasi_snapshot_preview1` 的
  显式函数白名单；未知模块/函数（含 WAMR `sock_*` 扩展）在加载前拒绝。
- **I-13 native host allowlist**：`WHISPER_NET_ALLOW` 在 HTTP/HTTPS
  建连前按 host 拒绝；配置 allowlist 后默认拒绝私网/loopback/link-local，
  `WHISPER_NET_ALLOW_PRIVATE=1` 可显式放行本地测试。未配置 allowlist 时
  保持兼容全放行。它不覆盖 raw listen/accept；方法/端口/响应大小由
  `WHISPER_NET_METHODS`/`WHISPER_NET_PORTS`/`WHISPER_NET_MAX_RESPONSE` 控制。

---

## 7. 执行 profile（目标）

| profile | 载体 | 文件 | 网络 | 执行 | FFI | 环境 |
|---------|------|------|------|------|-----|------|
| `untrusted` | WASM + `scripts/wrun.sh` | preopen（v1 只有 rw；ro 待实现） | 无 / host 代理白名单 | 无 | 无 | 白名单 |
| `trusted-native` | C/exe | OS 用户权限 | OS 用户权限 | OS 用户权限 | 允许（显式） | 全部 |
| `ci` | WASM 或 VM | 临时目录只读/读写 | 无 | 无 | 无 | 最小 |
| `build` | native | 源码目录 + 输出目录 | 无 | gcc/clang | 无 | 最小 |

`trusted-native` 不承诺沙箱；它只是“用户自己承担风险”的模式。

---

## 8. 整改顺序

1. **文档真相化**（本文件 + spec/README/stdlib 修正）——本次完成。
2. **负向安全测试基线**：先让当前实现暴露失败（标记为 expected-fail 或
   security-audit gate），再决定修 VM 还是直接转 WASM。
3. **WASM/WASI 目标**：见 `docs/wasm-wasi-plan.md`；这是原生安全的
   主路线，Rust 代码不改。
4. **过渡期**：native 产物标注 trusted-only；不可信代码禁用；`wd` 构建
   不可信源码需在隔离环境中进行。
5. **可选**：修 VM 的 FFI/ambient 门（需要 Rust 改动；在“Rust 侧不改”
   约束下暂不做，但应保留为 TODO）。

---

## 9. 审计清单（每个 effect 都要有）

- [ ] 无 policy 时调用 → 拒绝，错误信息可定位
- [ ] policy 允许 → 成功，且只允许授权范围
- [ ] 路径 `../` / 绝对路径 / symlink → 拒绝
- [ ] host 子域匹配规则 → 正确（不允许 `evil-example.com` 匹配 `example.com`）
- [ ] 命令注入：exec 不走 shell，argv 逐项校验
- [ ] FFI：不可信 profile 下编译期/运行期拒绝
- [ ] 网络：无 raw socket；host 代理白名单
- [ ] 错误不泄漏敏感路径/环境
- [ ] 退出码、stdout/stderr 行为与策略一致
- [ ] 文档、测试、实现三处一致
