# 收口战役：把登记在案的缺口逐条清掉（预注册）

2026-09-21 立项。仿前几份：**判定标准先写死，再动代码**。

本轮的目标不是加新能力，是**收口**——`docs/independence-design.md`、
`docs/language-backlog.md`、`tests/step3_preregistered.md`、
`tests/step2b_preregistered.md`、`CHANGELOG.md` 里逐条登记过的"未做/残留"，
能清的清掉，清不掉的给出**为什么不clean**。

## 立项时的实况核对（先查，再改）

侦察阶段就推翻了三条既有描述，先记在这里：

| 登记的说法 | 实况 |
|---|---|
| `path_is_allowed` 拿相对路径与绝对白名单比较，相对路径**永远被拒** | **已经是错的描述**：`capability.rs:129` 已把相对路径 join CWD 再归一化，注释里写明这就是为了修它。**代码已修，文档没跟上** |
| `whisperc/compiler.ws` 等与权威文件漂移 | 属实，且**无人引用**（全仓库只有注释提到）。它是早期"手工合并版" |
| `-t c`（Rust 转译器）对 `CapExec` 发射空操作 | 属实，且 `CapCall` 也一样——**两个 opcode 都静默发射一行注释**。而 `runtime.c` 里 `W_call()` 与 `W__0..W__9` 全都在 |

## 各项判据

### W1 —— 源码行尾归一化（CRLF）

**事实（实测）**：同一个 CRLF 源文件里的跨行字符串字面量
`"a\r\nb" strlen .`：

| 路径 | 输出 |
|---|---|
| `whisper run`（Rust 前端） | **4**（保留 `\r`） |
| `whisper build -t c2` | 3 |
| `wd run` | 3 |

根因：`c2.rs::expand_imports` 用 `str::lines()`（会吃掉行尾 `\r`），
Whisper 侧 lexer 也丢掉，**只有 `Lexer::new` 直接吃原始文本**。
后者还影响**编译器自身**：`include_str!("compiler.all.ws")` 在
`core.autocrlf` 工作树上带 CRLF，于是 `-t c2` 生成的 C 里混进 3 个杂散
`\r`（正是 A3 的 3 字节残差），而且同一份源码在 LF 检出与 CRLF 检出上
产出**不同的 C**——可复现性问题。

**判据**

- **W1-a** 上述三行表格统一为 **3**（三个路径一致）。
- **W1-b** `-t c2` 对自包含程序产出的 `.c` 与 `wd` 产出的 `.c` **逐字节
  相同**（A3 的自包含半边就此收口）。
- **W1-c** 单独的 `\r`（不跟 `\n`）**仍然保留**——它不是行尾，是合法字符。
- **W1-d** 六道门禁 + 固定点不回归。

**反向判据**：为了让三处一致而把 `\r` 从**字符串内容**里全局删掉（那会
改掉用户程序的数据）→ **失败**。只折叠 CRLF 这一对。

### W2 —— `wd` 的多行定义限定名克隆（A3 的另一半）

**事实**：`wd` 的 `wd-clone-piece` 要求定义行"以 `;` 收尾"才克隆，于是
`stdlib/math.ws` 里的多行定义（如 `clamp`）没有 `math.clamp` 克隆，
生成的 C 里少了 `W_math_clamp`。实测带 import 的程序 wd 17451B vs c2
17878B（差 427B、29 行）。

**判据**

- **W2-a** 带 import 的程序，`wd` 与 `-t c2` 的 `.c` **逐字节相同**。
- **W2-b** 克隆覆盖多行定义：`import "std/math"` + 调用一个**多行**定义的
  限定名（`5 0 10 math.clamp .`）两链同值。
- **W2-c** 里程碑 A/B 的四条冒烟不回归。

### W3 —— `bootstrap.sh` 在缺 `cmp` 的机器上假报固定点破裂

**事实**：`if ! cmp -s a b` 在 `cmp` 不存在时得到 127，取反为**真** →
打印 `FIXED POINT BROKEN` 并 `exit 1`。本机（msys2）实测 `cmp` 与 `diff`
都不存在。**这条脚本因此从来没有跑到过第 5 步**——上次记的"只做了
`bash -n`"背后其实还有一个真 bug。

**判据**

- **W3-a** 比较逻辑抽到 `scripts/lib/same-bytes.sh`，两个脚本共用；
  在该库下"相同/差一字节/长度不同"三种输入判定正确。
- **W3-b** `bash scripts/bootstrap.sh` **端到端跑完 7 步**，产物
  `target/bootstrap/whisper2` 与 `target/bootstrap/wd` 都在，且 wd 冒烟得 `3`。
- **W3-c** `bash scripts/verify-fixedpoint.sh` 仍通过（它改用同一个库）。

### W4 —— `-t c` 的能力 opcode 不再静默空操作

**事实**：`transpiler.rs` 对 `CapExec`/`CapCall` 各发一行注释，生成的程序
**静默少做一件事**。这是最坏的一类：不报错、结果错。

**判据**

- **W4-a** `CapExec` 发射 `W_call();`（runtime.c 既有），`CapCall(n)` 发射
  `W__<n>();`。
- **W4-b** `-t c` 产物经 gcc 后：`{ 42 } call .` → `42`；
  `"WHISPER_W4" "v" @4` 类能力调用与 VM 同值。
- **W4-c** `c_backend_tests` 12 例不回归；`-t c` 对**未实现**的能力 id
  仍走 `w_cap_unimplemented`（不假装成功）。

### W5 —— 历史快照与文档漂移

**判据**

- **W5-a** `whisperc/compiler.ws` 与顶层模块快照（`lexer.ws`/`parser.ws`/
  `classify.ws`/`main.ws`）：确认无人引用后**移出权威路径**并在
  `whisperc/legacy/README.md` 说明它们是什么、权威文件是哪个。
- **W5-b** `docs/independence-design.md` 里 `path_is_allowed` 那条"未动的
  可用性 bug"改为**已修**（代码早修了，文档撒谎）。
- **W5-c** `op-quote`/opcode 53 死代码：在 `compiler.all.ws` 原地加一行
  "这是死代码，原因见 …"，避免下一个人以为它可达。

### W6 —— `@5` 的 `trim()` 与逐字节转发（上一轮唯一未达标的判据）

**事实**：`@5` 的 `out`/`err` 两链都按设计 `trim()`（runtime.c 注释写明
"mirror Rust 的 `String::trim()`"）。于是 `wd run` 永远转不出子进程末尾的
换行，L-1 的 E4-c 卡在这里。

**设计决定（提前写明）**：**不动 `@5` 的语义**。`trim` 是命令替换惯用法
依赖的行为（`"cmd" @5 1 @nth strtoi64` 不需要自己剥换行），取消它会把
"修一个洞"变成"改一批调用方"。改为**新增一个不 trim 的兄弟能力**
`@10` = `exec-raw`，语义与 `@5` 完全相同，只是两个流原样返回。

**判据**

- **W6-a** VM 与 C 链都实现 `@10`，返回 `[status out err]`，**不 trim**。
- **W6-b** `"printf 'x'" @10 1 @nth strlen` → 1（`@5` 同命令给 0 或 1，
  取决于命令是否自带换行；用带换行的命令区分：`"echo x" @10 1 @nth strlen`
  → 2，`@5` → 1）。
- **W6-c** `@5` 行为**逐字节不变**（回归钉）。
- **W6-d** `wd run` 的转发段与直接运行产物**逐字节相同**（L-1 的 E4-c
  就此收口）。
- **W6-e** 未授权时 `@10` 与 `@5` 一样被拒（不能成为绕过沙箱的后门）。

## 不做（本轮明确不做，且说明理由）

- **TLS**：两链共同的现状，不是某一侧的缺口。C 链要自己实现 TLS 或引入
  依赖，与"C 只当可移植汇编、零第三方依赖"的既有契约冲突，需要独立设计。
- **`@6`-`@9` 的 C 侧实现**：与 TLS 强耦合（`@6` 是 HTTPS 会话）；在 TLS
  决定之前做 `@6` 只能做成"半套"。`@7`/`@8`/`@9` 单独做的收益不足以支撑
  一次 ABI 扩张。**保持现状：调用时显式报错**。
- **C 链 host 白名单**：设计文档已有立场——"不引入半套沙箱"。独立 C 二进制
  没有命令行，白名单只能走环境变量，那与 VM 的 `--allow-http` 不是同一种
  保证。保持现状并**在文档里写明这是刻意的**，而不是漏掉的。
- **Step 4（把 fmt/包管理器/REPL/LSP 迁到 Whisper）**：独立战役，不在收口范围。

## 执行记录

2026-09-21 实施完成。**W1–W6 全部达成**，其中 W5 的范围在核对后被改写
（见下）。

### 侦察阶段推翻的三条既有描述

| 登记的说法 | 实况 |
|---|---|
| `path_is_allowed` 相对路径永远被拒 | **代码早已修好**（`capability.rs:129` join CWD 再归一化）。是文档没跟上，不是代码欠账 |
| `whisperc/compiler.ws` 等"漂移快照" | 只有一部分是死的：`compiler.ws`/`src/cgen.ws`/`src/classify.ws`/`src/lexer.ws` 零引用；而 `lexer.ws`/`classify.ws`/`parser.ws`/`main.ws`/`src/compile.ws` **是活的**（`whisper bootstrap` 用 `include_str!` 拼） |
| `-t c` 对 `CapExec` 发射空操作 | 属实，且 **`CapCall` 也一样**——两个 opcode 都静默发注释 |

### 逐条对照

| 判据 | 结果 | 实测 |
|------|------|------|
| **W1-a** 三条路径对 CRLF 源一致 | ✅ | `"a\r\nb" strlen .`：修前 VM=4 / c2=3 / wd=3；修后 **三者皆 3** |
| **W1-b** `-t c2` 与 `wd` 的自包含 `.c` 逐字节相同 | ✅ | 1601 == 1601 |
| **W1-c** 单独的 `\r` 仍保留 | ✅ | 四条 lexer 单测钉住（裸 `\r`、引号前 `\r`、`\r` 转义不受影响、行号不变） |
| **W1-d** 门禁与固定点不回归 | ✅ | 见下方"全量验收" |
| **W2-a** 带 import 的程序 `.c` 逐字节相同 | ✅ | 17875==17875、17228==17228、22569==22569 |
| **W2-b** 多行定义的限定名两链同值 | ✅ | `5 0 10 math.clamp .` 两链皆 5；`-3 0 10 math.clamp .` 两链皆 0 |
| **W2-c** 里程碑 A/B 四条冒烟不回归 | ✅ | 3 / 25 / 3 / 25 全过 |
| **W3-a** 共享比较库判定正确 | ✅ | 相同→equal、差一字节→differ、长度不同→differ；两脚本 `bash -n` 通过 |
| **W3-b** `bootstrap.sh` 端到端 7 步 | ✅ | 见下方"全量验收"——**这是它第一次真正跑完 7 步** |
| **W3-c** `verify-fixedpoint.sh` 仍通过 | ✅ | `gen2 == gen3`（已改用同一个库） |
| **W4-a/b** `-t c` 真实发射能力 opcode | ✅ | 产物中不再有 `no-op in native mode`；`{ 42 } call .`→42、`10 { 2 * } call .`→20（gcc 编译后实跑） |
| **W4-c** `c_backend_tests` 12 例不回归 | ✅ | 12/0 |
| **W5-a** 零引用快照移出 | ✅ | 删 4 个（引用数是机器数出来的），新增 `whisperc/README.md` 写清权威文件与软自举模块 |
| **W5-b** `path_is_allowed` 文档改正 | ✅ | 设计文档两处过期描述改为"实为已修，勿再立项" |
| **W5-c** `op-quote` 死代码标注 | ✅ | 原地加注：53 在本文件只出现于该词，cgen 无 53 分支；引号可用性靠 `fuse-hof`，与它无关 |
| **W6-a/b** `@10` 两链实现且不 trim | ✅ | `"echo x" @10 1 @nth strlen` → **3**（两链同值）；`@5` 同命令 → 1 |
| **W6-c** `@5` 逐字节不变 | ✅ | 仍是 1；既有 `cap-*` 覆盖用例全过 |
| **W6-d** `wd run` 转发逐字节相同 | ✅ | 转发段 `33 0A` == 直接运行产物 `33 0A`；"产物输出作为后缀出现在 wd run 里 = yes" |
| **W6-e** 未授权时 `@10` 被拒 | ✅ | `Error: Runtime error: Capability not bound: @10` |

### W6 顺带照出来的两处 C 链缺陷（此前被 `@5` 的 trim 掩盖）

`@10` 一做出来，C 链与 VM 的原始字节就**不一样**（`78 20 0A` vs `78 0D 0A`），
于是暴露了：

1. 命令包裹写成 `( cmd )`，`cmd.exe` 的 `echo` 吞掉行尾 → 实际产出 `x `
   （多一个空格）。改 `(cmd)`。
2. `_popen` 的读端是文本模式，会把子进程的 CRLF 折叠成 LF。加
   `_setmode(_fileno(p), _O_BINARY)`。

修完两链原始字节一致。**这两处都不是本轮引入的**，是 trim 一直在掩盖它们。

### 过程中踩到的两个栈语义坑（已写进 wd.ws 注释）

1. **`over` 会读到调用者的栈**。`wd-clone-of` 里闭合判定一度排在 `line` 已被
   `drop`/切片消耗之后，于是 `over` 拿到的是调用者的 state 列表，`strfind`
   拿列表当字符串 → `expected ptr`。**同一个坑这个文件栽过两次**，现在注释
   就写在出事那一行上面。
2. **KV 只存字符串**。cgen 对键和值都做 `w_as_ptr`，把布尔存进 KV 就是
   `expected ptr`。标志位一律走 `"1"`/`"0"`。

### 全量验收（2026-09-21 本机实测，gcc = msys64 mingw 16.1.0）

| 项 | 基线（收口前） | 收口后 |
|---|---|---|
| `cargo test --workspace` | 482 / 0 | **490 / 0**（+4 parser、+4 CLI 集成） |
| syntax-gate | 26 / 0 | 26 / 0 |
| stdlib-gate | 58 / 0 | 58 / 0 |
| typecheck-gate | FP=0 / TP=23 | FP=0 / TP=23 |
| net-gate | PASS | PASS |
| app-gate | PASS（13 路由） | PASS（13 路由） |
| `opcode_coverage.sh` | 87 / 0 / 0 | **90 / 0 / 0**（+3 条 `execraw-*`） |
| 固定点 `verify-fixedpoint.sh` | gen2 == gen3 | **gen2 == gen3** |
| `build-wd.sh`（零 Rust） | 通过 | **通过** |
| `bootstrap.sh`（零 Rust，7 步） | **从未跑完过**（`cmp` 假失败） | **7/7 完成，EXIT=0** |

`bootstrap.sh` 的 7 步：种子 → whisper1 → whisper2 → 固定点自检 → stdlib 冒烟
→ 用 whisper2 编出 `wd` → `wd` 自己编并跑一个程序（输出 3）。全程只有 gcc。
这是它第一次真正跑完——此前第 4 步会因为本机没有 `cmp` 而假报固定点破裂。

`wd` 与 `-t c2` 的 C 输出逐字节一致（四个程序，含多行定义的限定名克隆），
这条断言现在由集成测试 `w2_wd_output_matches_c2_byte_for_byte` 常驻把关，
且跑的是**零 Rust 链产出的** `target/wd/wd`。


