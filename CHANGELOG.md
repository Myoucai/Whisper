# Changelog

## Unreleased (2026-09-21g) — 战役 O（部分达成）：三处自举编译器缺陷 + 一个门禁盲点

目标原本是"让自举链支持引号作为一等值"，使 `call` 的动态分发两链一致。
**未完全达成**——但过程里修掉三处真实的编译器缺陷，并按预注册纪律
（`tests/o_preregistered.md`）只落地**真实用户路径验证过**的部分。

### 修复：自举编译器三处缺陷

1. **`cl-list` 缺一趟 `structify`**（`classify-nested`）。`cl-quote` 是
   `dup 1 @nth structify classify-nested`，`cl-list` 却只有
   `dup 1 @nth classify-nested`。收集器把方括号内的 token 留成裸串
   （`cbr-loop` 只归组嵌套方括号、不认花括号），少了这趟 `structify`，
   列表元素里的 `{` 就被分类成**普通词**而非引号节点——实测分类树
   `[4 "{"]`。修复后为 `[6 [[5 [[0 1] [3 16]]]]]`。
2. **`next-consumer-op` 越界**：`dup 1 @nth` 未查长度，引号恰好是最后一个
   token 时（`{ 1 + }` 单独成程序即可触发）整个编译器崩在
   `Index out of bounds`。加长度守卫。
   （附带修掉第一版守卫里多余的 `drop`——`??` 已消费条件，那个 `drop`
   吃掉的是 token 列表本身；由 `--trace` 定位。）
3. **`genq-name` 残留计数器**：非首次调用时，计数器字符串既已被消费也不再
   需要，却被多读一次 `"__QN"` 留在栈上。单引号看不出来（真分支有 `drop`），
   **每多一个引号就多一份垃圾**，把真正的操作数挤出可达范围 →
   `Type mismatch: expected [T], got str`。修复：直接用栈上那个值。

另外去掉 `fuse-hof` 的 `__FH-N` **共享 KV 临时键**（递归调用会覆盖它，
名字改为全程留在栈上）——与第 3 条同属"用全局暂存传值"的隐患。

### 未达成（如实登记，未加白名单）

- **≥3 个引号**：用户路径（`whisper build -t c2`）仍报
  `Type mismatch: expected str, got [T]`。**2 个引号已两链一致（7）。**
- **列表字面量里的引号**（原判据 O-1/O-3）：根因已定位到行 —— `op-quote`
  发射 `[53, inner_bc]`，而 `53` 在整个 `compiler.all.ws` 里**只出现在
  `op-quote` 自己那两行**，cgen 没有 53 分支（`op-quote` 实为死代码）；
  `op-list6` 用含 `fuse-hof` 的 `compile` 编译元素，提升出的
  `[9, name, body]` 落在列表节点内部，而 `compile-one` **有 tag 16 分支、
  没有 tag 9 分支** → 定义被丢弃、引用留着 → `'W___q0' undeclared`。
  转入 `docs/language-backlog.md` **战役 O′**（含两条修法方向与预注册判据）。

### 顺带发现：门禁的 c2 腿与用户路径不等价（门禁盲点）

`tools/opcode_coverage.sh` 的 c2 腿走 **NSC**（先把 `compiler.all.ws` 编成
原生二进制、再 `NSC -c < src`），而用户执行的 `whisper build -t c2` 会
**先内联 `std/core`**。三个引号的程序在 NSC 下成功、在用户路径下失败——
**NSC 通过 ≠ 用户能过**。

据此**拒绝**把 `quote-triple` 登记进门禁：它会在门禁里显示绿色，而对用户
是红色的。门禁本身改为以用户路径为准（本次只登记 2 引号一例）。

### 验证

    cargo test --workspace        474 / 0
    cargo clippy -D warnings      0
    cargo fmt --all --check       0
    syntax / stdlib               26 / 0 · 58 / 0
    typecheck-gate                FP=0 PASS
    opcode_coverage               60 / 0 / 0
    net-gate / app-gate           PASS / PASS (13 routes)
    verify-fixedpoint             gen2 == gen3

**未重生种子**：种子自 2026-09-03 冻结以来 `compiler.all.ws` 已改动多次而
固定点始终成立——种子是链的起点、不是编译器的副本，改 `.ws` 不需要重生它
（`runtime.c` 由 `#include` 现场取用，同理）。

## Unreleased (2026-09-21f) — 语言侧：`call` 补上缺失的拼写 + 两处修复

### 新增 —— `call`：动态调用引号（把一个悬空的 opcode 变得可达）

`Opcode::CapExec` 自始就在 VM 里实现（`Value::Ref(code) =>
execute_ref(code)`），但**从源码不可达**：设计原意是 `!`，而 `!` 已被
逻辑非占用（spec §3.2 冻结），词法层的 `TokenKind::Bang` 从此不再被产出，
opcode 就此悬空——文件里留着 `!` 的注释，parser 里留着
`Bang => CapExec` 的分支，但没有任何输入能走到它。

现以 `call` 词暴露（`call` 此前未被占用；`exec` 已被 std/os 的能力包装
占用故不可用）。语义：消费栈顶引号并执行，引号的输入取自调用者数据栈、
产出留在调用者数据栈——与 `@map`/`@fold` 这类提供元素槽的消费者不同。

    { 42 } call .                   # 42
    10 { 2 * } call .               # 20
    : ap { call } ; 5 { 1 + } ap .  # 6

- Rust 侧：`token.rs` 把误导性的 `Bang` 正名为 `CapExec` 并补
  `keyword_name` 往返（`call` 可作 def 名）；`lexer.rs` 加关键字；
  `checker.rs` 建模栈效应（弹引号、把其声明产出压回栈）。
- **C 链**：cgen 一直把 `call` 当普通词发射 `W_call()`，但没有任何地方
  定义该符号——于是 VM 能跑、c2 编译报 gcc 错。与 `@n` 同一模式，因此在
  `runtime.c` 里实现 `W_call`，**不动冻结的 compiler.all.ws**：引号字面量
  发射为裸函数指针（`w_push(W_PTR((void*)W_qN))`），正是高阶 helper 已在
  用的 `wfn` 形状；用 `wval_is_heap` 把代码指针与 arena 里的字符串/列表
  区分开，失败走 `w_fail` 以便被 `try` 捕获（对齐 VM 的可捕获 TypeMismatch）。
- 门禁：`opcode_coverage.sh` **56 → 59**；新增集成测试
  `call_invokes_quotation_on_both_chains`（每个程序只放一个引号字面量，
  理由见下）；spec 新增 §16 词汇附录 v1.0.9。

**发现的边界（先前就存在，与 `call` 无关）**：自举链只在程序含**单个**
引号字面量时能编含 `call` 的程序。引号作为一等值——两个及以上引号字面量、
或引号出现在列表字面量里——会让 `whisperc compile-full` 报
`Type mismatch`。`[{ 1 + }] len .` 不含任何新语法，在引入 `call` 之前就
编不过（已用 stash 在 HEAD 上对照复现）。`call` 只是让这个洞变得可见：
动态分发（把引号放进列表/map 再取出来调用）正是引号作为一等值的主要
用途。已立项为**战役 O**（`docs/language-backlog.md`），含实测对照表与
预注册判据——修它要动固定点基准并重生种子，故需要独立立项。

### 修复 —— `try` 的引号合同过严，strict 默认下误拒合法代码

`checker.rs` 对 `try` 用 `expect_ref`，即要求引号是净零 `[T] → [T]`。
但 `try` 是**捕获**体的产出的（VM 跑完弹一个值、默认 0，再压
`[flag, value]`），体有净产出完全合法。于是默认 strict 下：

    { 1 0 / } try drop "caught" .   ->  try quot: expected [T7] → [T7],
                                       got [T6] → [T5 i64 | f64]

而 VM 一直跑得好好的（`--no-strict-types` 下输出 `"caught"`）。
改用 `expect_ref_shape(stack, 0, None, ...)`——"产出不限"，与 `@each`
同一个合同。引号体内的真错误仍照抓（`{ "s" 1 + } try ...` 依旧报
`arithmetic lhs: expected i64 | f64, got str`）。

### 修复 —— 文件能力的相对路径永远被拒

`path_is_allowed` 把请求路径归一化后直接与白名单根比较，而根是绝对路径
（`current_dir()`），所以相对路径**永远不可能** `starts_with` 它——
`--allow-file-write` 下 `write-file "x.txt"` 根本用不了。失败关闭，不是
安全洞，但是不可用。

现在相对路径先与 CWD 拼接再归一化比较——这正是文件系统打开文件时做的事。
`..` 仍在拼接**之后**按词法解析，所以逃逸的相对路径照样被包含性检查拦下。
用 `has_root()` 而非 `is_absolute()`：后者在 Windows 上对
`/allowed/dir/file.txt`（无盘符的盘相对路径）返回 false，会把它错误地送进
CWD 拼接分支——这个 bug 是本机（Windows）跑既有安全单测时抓到的，Linux
上不会暴露。

新增 2 个单测（相对路径可用、相对路径逃逸仍拒），原有 6 个安全单测行为
不变（它们全用绝对路径）。

### 验证

    cargo test --workspace        473 / 0
    cargo clippy -D warnings      0
    cargo fmt --all --check       0
    opcode_coverage               59 / 0 / 0

## Unreleased (2026-09-21e) — 整体回顾：CI 从未运转 + 一个被它掩盖的字节冲突

对独立性工作做整体复核时发现的问题。第一条是根因，后面几条都是它掩盖的。

### 修复（P0）—— CI 工作流文件是非法 YAML，**从未运行过**

`.github/workflows/ci.yml` 里 `name: App gate (Whisper web backend:
JSON/static/404 routes)` 的冒号未加引号，YAML 解析直接失败
（`mapping values are not allowed here`）。GitHub Actions **无法加载该
工作流**，所以 `test`/`selfhost`/`c-backend`/`lint` 全部从未执行。
CHANGELOG 里历次"四 CI 门全绿"的说法来自本地手工跑，不是 CI 的实际结论。

修复：该 name 加引号。校验后 6 个 job 全部可解析
（`test` / `c-backend` / `c-backend-clang` / `selfhost` / `lint` / `release`）。

**这条解释了本轮的另外两个回归为何能存活**：递归守卫 P0（2026-09-21b）
与下述字节冲突，都会被 `test`/`selfhost`/`lint` 任一条腿拦住。

### 修复（正确性）—— `.wbin` 指令字节冲突：`Try` 与 `BytesToHex` 共用 0xC1

`BytesToHex`（2026-09-18，L-12 新增）被分配了 `0xC1`，而 `Try` 自
2026-06-07 起就占着该字节。冲突被复制到**四处**：

| 位置 | 角色 |
|------|------|
| `opcode.rs` `to_byte` | 编码 |
| `wbin.rs` `from_byte` | `.wbin` 解码 |
| `bootstrap.rs`（编码侧） | 自举管线 |
| `bootstrap.rs`（解码侧） | 自举管线 |

三张解码表都是 `0xC1 => BytesToHex` 在前，于是**每一段 `try` 写进 `.wbin`
再读回来都变成 `bytes->hex`**，运行时炸在 "expected i64, got ref"。
实测复现：同一程序直接 `run` 正常，经 `.wbin` 往返后报错。

修法：新来者让位 —— `BytesToHex` 迁到空闲字节 `0xCE`，`Try` 保留
`0xC1`。`.wbin` 里含 `try` 的程序自此恢复正常。

**防回归**：`opcode.rs` 新增 `to_byte_is_injective` —— 158 个变体逐个取
字节，断言两两不同（`from_byte(to_byte(op)) == op` 那类往返断言**抓不到**
编码侧冲突，必须直接查重）。已反向验证：把字节改回 0xC1，测试立即报
`0xC1 shared by BytesToHex and Try`。

### 清理 —— `lint` 作业的既存债（CI 复活后会立刻变红）

- **rustfmt**：132 处差异 / 34 文件（含大量本轮未触碰的文件）→
  `cargo fmt --all`，现 `--check` 干净。
- **clippy**：39 warning → **0**，`cargo clippy --workspace -- -D warnings`
  通过。其中数条是真问题而非风格：
  - `xorshift64_next` 自由函数是死代码（VM 用的是同名方法，逻辑重复）→ 删。
  - `parser.rs` 的 `mod tests` 缺 `#[cfg(test)]`，导致测试模块被编进
    lib 目标（clippy 因此报 `use super::*` 未使用）→ 补门控。
  - `vm.rs` 一个函数上叠了两个 `#[test]`（前一个测试函数被删后留下悬空
    文档注释与属性）——**这让 libtest 多注册了一个幻影测试**，所以历史
    计数 471 里的 1 是虚的，真实基数是 **470**。
  - `builtins.rs` 的 `rot` / `@filter` 各重复一条（签名相同，无害），
    `vm.rs` 的 `"list?"` 重复一条 → 删冗余。
  - 其余为 `type_complexity`（加别名）、`is_multiple_of`、
    `eq_ignore_ascii_case`、`while let`、`Default`、十六进制分组等。

### 未修（如实记录，各自独立）

- `try` 接一个"有净产出"的引号时，strict 类型检查会拒（`expected [T]→[T],
  got [T]→[T i64|f64]`）。**HEAD 上同样如此**（已用 stash 对照验证），
  属检查器对 `try` 签名的建模问题，不在本轮范围。
- `path_is_allowed` 拿相对路径与绝对白名单比较，故 `--allow-file-write`
  下相对路径永远被拒（失败关闭，非安全洞）。在安全边界上且已有 6 个
  加固单测，应单独评估。
- HTTP 能力（`@2/@3/@6`-`@9`）仅 VM 侧可用 —— Step 2b。
- 仓库根目录三个历史调试残留（`out_fire_stage.txt` / `out_l2.txt`，
  1–3 字节，非任何脚本产出）未动。

## Unreleased (2026-09-21d) — Step 6：C 降级为可替换接口

把 C 从"依赖"变成"接口"。核心不是删掉 C，而是**让任何一个 C 编译器
都能当后端**——这恰好是当初选 C 的理由，之前却因为两个隐含假设没兑现。

- **`W_TLS` 条件宏**：原来无条件 `__thread`，等于把运行时悄悄钉在
  GCC/Clang 上。现按 MSVC → `__declspec(thread)`、C11 → `_Thread_local`、
  其余回退 `__thread`。
- **移除 `-fno-strict-aliasing`（先证明再删）**：
  - 静态审计：无类型双关（`*(double*)&x` 类 0 处、无 union），位模式
    转换全部走 `memcpy`；`w_str_new` 的 `*(uint32_t*)p` 合法（arena
    存储无声明类型，首次写入确定有效类型）。
  - 动态验证：取最重工作量（`compiler.all.ws` 65 KB → 205 KB C），
    带/不带该标志各编一个自举编译器，编译同一程序输出**逐字节一致**
    （SHA256 相同）。
  - 从 **9 个构建路径**移除（CLI 的 c2/native、bootstrap、fixedpoint、
    覆盖门禁、run_core、两个测试、README 示例），并 bump `-t native`
    内容寻址缓存键（`-O3-flto-v2`）以免复用旧标志下构建的条目。
  - **决定性理由不是性能**：MSVC 根本没有这个标志，它就是"C 可替换"
    的直接阻碍。
- **规范锁定**：`docs/spec.md` 新增 §15「C 发射契约」——C99 + POSIX/Win32、
  禁止编译器扩展、不得要求非默认标志、不得依赖 UB、编译器矩阵至少
  gcc + clang 两条腿。违反即为**规范级缺陷**。
- **CI 新增 `c-backend-clang` 腿**：`WHISPER_CC=clang` + `GCC=clang`，
  跑 C 后端测试 + stdlib 门禁 + 覆盖门禁。这是 clang 的唯一验证途径
  （本机无 clang，gcc 腿为本地实测）。
- **残留风险（如实记录）**：删标志的依据是"静态审计干净 + 最重工作量
  逐字节一致"，但**通过测试不等于证明无 UB**。若将来在某编译器/优化
  级别上出现异常，正确反应是把标志加回来并记录触发条件。
- **未纳入**：tcc / msvc 腿。MSVC 需要的不止是标志（`unistd.h`、`popen`
  命名等），属另立项。

## Unreleased (2026-09-21c) — Step 2a：C 链能力 ABI 接线（`@n` 令牌）

### 背景：`exec` 早就存在，缺的是整条能力 ABI

`exec` 不是缺词——它是能力令牌 `@5`（`stdlib/os.ws`），VM 侧一直可用。
真正的洞是 **C 链不支持 `@n` 令牌**：cgen 把 `@n` 当普通词引用，一律发射
`W__<n>()`（`compiler.all.ws:1293`），而这些函数从不生成，gcc 直接报
`implicit declaration of function 'W__4'`。后果远超"缺 exec"：
`import "std/os"` 经 c2 **根本编不过**。

之所以长期未发现：**c2 的六道门禁没有任何一个用例碰过能力令牌**——
stdlib-gate 测 math/list/str，net/app 走原生 socket 操作码。覆盖盲区，
不是偶发疏漏。

### 新增：runtime.c 实现 cgen 既成的能力 ABI

`@0`（file read）、`@1`（file write）、`@4`（env）、`@5`（exec）四个
shim，语义逐条镜像 `capability.rs`。**不动 `compiler.all.ws`**——那是
固定点基准，改它必须连带重生种子并重跑整条自举链；ABI 是 cgen 既成的
契约，实现它比修改它便宜得多。

- `@5` exec 用 popen + stderr 重定向到临时文件（popen 只能捕获单流），
  命令包在 `( ... )` 组里让重定向覆盖整条命令，stdin 接空设备以对齐
  Rust `Command::output()`——否则读 stdin 的命令会永久挂住 VM。
  状态码走 `WEXITSTATUS`，两流按 VM 的 `String::trim()` 修剪。
- 未覆盖 **2/3/6/7/8/9（HTTP 家族）**：需要 C 侧 HTTP+TLS 客户端，
  是独立中等工程（Step 2b）。用到它们的程序仍然编不过——这是诚实的
  结果，好过产出错误的二进制。

### 修复：VM 侧两处能力缺陷（双链对拍时暴露）

- **`Opcode::CapCall` 恒弹一个操作数**，而 `FileWriteCap`/`HttpPostCap`
  需要两个（`path content`）——所以 **`@1` 在 VM 里从来不可用**，而 C 链
  （按 shim 实际需要弹）与它分歧。新增 `Capability::arity()`（默认 1，
  `FileWriteCap` 覆写为 2），`CapCall` 按其弹取并反序成声明序。
  HTTP 能力的 arity 保持默认，行为不变（属 Step 2b）。
- **`FileWriteCap` 写完不压任何结果**：既静默丢弃失败，又与原生
  `write-file` opcode、C 链 `w_write_file`、以及类型模型三者都不一致。
  现返回字节数。`@1` 此前完全不可用，无兼容负担。

### 修复：`@n` 令牌缺类型签名（strict 默认下误拒合法代码）

`checker.rs` 把 `CapCall` 归入 `_ => {}`（既不吃操作数也不产结果），
于是 `"cmd" exec 1 @nth .` 在**默认 strict 模式**下被拒
（"@nth list: expected [T0], got str"）。现为 0/1/4/5 四个 id 建模栈效应
（`@5` 的异构结果用语言既有的 `Type::Union`）。与 J-3 修过的 `http-*`
内建签名缺失同属一类。HTTP 家族保持不建模（Step 2b）。

### 验证

- **双链对拍**：`@0/@1/@4/@5` 五组用例 VM 与 c2 输出逐行一致
  （含非零退出码 7、独立 stderr 捕获）。新增 Rust 集成测试
  `capability_abi_matches_across_chains`（覆盖文件对，因 VM 的文件能力
  白名单锚定 CWD，shell 里构造跨平台绝对路径过于脆弱）。
- `tools/opcode_coverage.sh` 新增 4 例（cap-env / cap-exec-out /
  cap-exec-st / cap-exec-err）；VM 侧运行行补上能力绑定标志。
- 回归：cargo workspace **470/0**；typecheck 棘轮 FP=0 / TP=23 不变。

## Unreleased (2026-09-21b) — P0 回归修复（递归守卫阈值）+ Step 1：`-t exe` 去 Rust 化

### 修复（P0 —— 上一提交自带的回归）

- **词调用深度守卫阈值过低，c2 链对真实程序全面不可用**。上一提交
  （eec695f）为 `: f { f } ; f` 的 30 GB 分配问题加了 2048 帧上限，
  但**自举编译器是"用递归代替循环"遍历 token 的**，所需深度 ≈ token
  数，随输入规模线性增长。实测：

  | 输入 | 体积 | 所需帧数 |
  |------|------|---------|
  | `stdlib/math.ws` | 1.9 KB | 2 049 – 4 096 |
  | `whisperc/compiler.all.ws` | 65 KB | 16 385 – 32 768 |

  后果：`whisper build -t c2` 对**任何 import stdlib 模块的程序**直接报
  `RecursionLimit`，连 `compiler.all.ws` 自己都编不过——而
  `tools/opcode_coverage.sh` 正是这样构建自举编译器的。`stdlib-gate` /
  `syntax-gate` / `app-gate` / `net-gate` / 覆盖门禁**全部受影响**。
  上一提交的回归记录只写了 cargo 与固定点（固定点走 C 种子链，绕开了
  Rust VM 的守卫），所以未被发现。
- **修法**：默认上限 2048 → **262 144**。对已知最大工作量（32 768 帧）
  留 8× 余量；按 48 字节/帧计最坏约 25 MB，而原始 bug 是 30 GB。
  另新增 `WHISPER_MAX_CALL_DEPTH` 运行时逃生口（诊断手段，非安全边界——
  内存已由数据栈守卫约束）。
- **回归测试 4 项**：`c2_chain_compiles_stdlib_imports`（std/math 经
  c2 链编译并运行，端到端复现原故障）、`deep_user_recursion_beyond_old_ceiling_runs`
  （5000 帧用户递归可运行）、`test_deep_but_legitimate_recursion_is_allowed`
  （上限不变量：>32768 且 ≤1M）、`test_word_recursion_depth_guard`
  （改为断言当前阈值而非硬编码 2048）。
- **复验**：std/math 0.8s、webapp server 2.0s、compiler.all.ws 31.2s
  全部恢复可编译；syntax 26/0、stdlib 58/0、typecheck FP=0 全绿；
  cargo workspace **469/0**。

### 变更（Step 1：`-t exe` 去 Rust 化）

- **`-t exe` 改为自举链原生产物**。此前它把 **Rust CLI 二进制本身**复制
  一份再追加 wbin 负载（`build.rs` 的 `current_exe()` + `WSHPEXE1`
  trailer），于是"自包含可执行文件"里装的其实是整个 Rust VM（还捎带
  LSP/REPL/类型检查器/包管理器）。现改走 c2 链产出普通机器码，
  **产物零 Rust**。
- **`-t bundle` 保留旧行为**（解释器打包：构建期不需要任何工具链），
  帮助文本与文档如实标注 `embeds the Rust VM`。两种分发模型不再共用
  一个名字。选择保留而非删除，是因为"没有 C 编译器也能出单文件"这条
  工作流是真实的，只是它必然带 Rust，应该按名字说清楚。
- **产物路径回显修复**：mingw gcc 会把 `-o app` 自作主张变成 `app.exe`，
  而 CLI 此前回显的是**请求路径**而非真实产物路径。现解析实际存在的
  文件再报告（影响 c2 / native / exe 三个目标）。
- **测试**：新增 `exe_target_is_native_and_carries_no_rust`——断言产物
  可运行、**不含** `WSHPEXE1` trailer、且确实经过 C 链（同级 `.c` 存在）；
  原 `bundled_exe_*` 两个测试迁移到 `-t bundle` 并更名 `bundle_target_*`。

### 环境发现

- 本机 gcc 一直存在于 `C:\msys64\mingw64\bin`（未入 PATH），因此此前
  "无 C 编译器即跳过"的 c2/C 后端测试**从未在本机真正执行过**。纳入
  PATH 后 12 项 c_backend 与全部 c2 链门禁首次在本机跑通——上述 P0
  回归正是这样暴露的。

## Unreleased (2026-09-21) — 版本号与文档口径对齐（纯文档，零行为变更）

- **版本号统一为 1.0.8**：`docs/spec.md` §13/§14 的词汇附录已在
  v1.0.8 / v1.0.8b（FFI + 字节语义补全），`ffi.rs` 与 `runtime.c` 亦以
  v1.0.8 标注；而 `Cargo.toml`、CLI `VERSION`、`package.ws` 仍停在
  1.0.0，README 徽章写 1.0.7——四处口径互不一致，本次统一到 1.0.8。
  已核：无任何测试断言版本字符串（`--version` 用例仅断言退出码）。
- **README 失真修正**（逐条实测后重写）：
  - 徽章：version 1.0.7→**1.0.8**、tests 451+5 gates→**465+7 gates**、
    stdlib 14→**20 modules**；
  - "手写 C 种子"→**冻结 C 种子**：`seed/compiler_seed.c` 头部自述
    2026-09-03 起为"100% generated C"（由自举链自身产出，旧手写种子
    在 git 历史），自举章节文字与流程图同步修正；
  - 功能表：`137 opcodes`→**154**（`opcode.rs` 枚举逐行实测，
    154 个变体全部出现在 `to_byte`/`from_byte`）；"静态类型推导
    （当前仅警告）⚠️"→**strict 默认 ✅**（N-2 已翻转，README 落后于
    代码）；"标准库 13 模块, 60+ 函数"→**20 模块**（20 个 `stdlib/*.ws`
    逐个 `import` 实测通过）；
  - c2 段：`v1.0.3-1.0.7`→**v1.0.3-1.0.8**（FFI 已在该链上跑通，
    stdlib-gate 含 p8-ffi 用例）；值标签 3 个→**5 个**
    （WT_I64/WT_F64/WT_PTR/WT_SIG/WT_BOOL，`runtime.c:32`）；
    残留分歧两条经复核**仍成立**（C 链串打印不带引号；
    `w_val_eq` 仅比标签+位，非深比较），保留并补注出处；
  - 测试章节：451→**465**、54→**58** 双链断言，补 opcode 覆盖门禁与
    typecheck 棘轮两道（门禁计数 5→7）；覆盖清单按 crate 实测重写
    （CLI 139 / core 132 / codegen 138 / parser 34 / typecheck 15 /
    LSP 5 / package 2 = 465）；
  - 项目结构：补 `apps/wheels`、`adapters`、`tests/conformance`、`tools`。
- **docs/stdlib.md**：模块计数 14→**20**（本文详列 15 个，其余
  `std/core`、`std/net`、`std/random`、`std/time` 点名，`std/ffi`
  指向 spec §13/§14）。
- **明确未动**：`registry.json`（包注册表 schema 版本与 std 包自身
  版本，语义不同）、`vscode-extension/package.json` 0.2.0（插件独立
  版本线）、`docs/spec.md` 头部 "v1.0 (FROZEN)"（语法合同版本）。
- **遗留待办（非本次范围）**：仓库内仍有与权威文件漂移的历史快照
  （`whisperc/compiler.ws` 手工合并版、顶层 `whisperc/*.ws` 与
  `whisperc/src/*.ws` 日期错位），以及 `-t c`（Rust 转译器）与
  `-t c2`（自举链）两条通往同一目的的 C 生成路径——清理需单独评估
  影响面，未纳入本轮零风险对齐。

## Unreleased (2026-09-18) — 六阶段计划 Phase 4 收尾：VM 递归防护 + 死字段清除

- **词调用深度守卫（P4.3）**：`Opcode::Call` 在 call_stack 深度
  ≥2048 时返回 `RecursionLimit`。此前解释器虽是迭代帧（无原生栈
  溢出），但 `: f { f } ; f` 会无限增长 call_stack 直至分配器
  abort 整进程（实测 30GB 分配失败）。ref 块守卫（512）此前已有，
  本次补齐词调用路径。错误文案泛化（word-call or quotation nesting）。
  回归：`: f { f } ; f` → 干净 RecursionLimit；VM 单测锁定。
- **CallFrame.base 死字段清除（P4.6）**：全仓库零读取（仅 4 处
  构造赋值），连字段带构造一并移除。
- **Phase 4 盘点确认**（计划快照滞后，实测已完成项）：Value::equals
  List/Map 深比较 ✓、StrSlice 字节语义+边界 ✓、Take/DropN/Nth 负数
  NegativeValue ✓、i64 溢出安全（实测 wrap 不 panic）✓、
  `Rc<[Opcode]>` word_dict ✓、capability 词法归一（ParentDir pop +
  sibling 前缀攻击测试，6 单测）✓、cgen transpiler W_qN 引号池 +
  runtime.c 高阶 helper 装线 + TODO=0 + gcc 端到端测试 ✓。
- **回归**：cargo workspace 465/0；固定点验证 gen2==gen3 OK
  （scripts/verify-fixedpoint.sh 全程 ~25min，当前树实测）。
- （同期 L 系列战果见 1512ca1/635c814/cda7db3：#48 keyword 冲突
  警告、L-12 bytes->hex + http-req-raw、L-11 http-req-parallel。）

## Unreleased (2026-09-10o) — N-3：WT_BOOL 第五标签（布尔全对齐）+ Type::Map 保险层

- **WT_BOOL**（runtime.c 值模型第五标签 + cgen 全量发射改造）：
  原 N-1 判定的两格"永久分歧"闭合——`#t .`→#t、`1 2 < .`→#t、
  `#t json-stringify`→true、`"[true]" json-parse 0 @nth .`→#t、
  `{ 42 } try . .`→42/#t、`#t:0.5 unwrap .`→#t。布尔字面量/比较
  （= < > != <= >=）/逻辑（& | !）/streq/map-has/谓词（list? i64?
  str? bool?）全部 W_BOOL；w_val_eq 对齐 VM equals（仅 Bool↔I64
  跨标签数值比）；bool? 精确判定（I64 1 非 bool——旧 0/1 启发式
  修正）；数值位协变（w_as_i64/w_num_f64 收 BOOL）。12 项探针
  双侧一致；spec §4 撤两行永久分歧。stdlib 门禁 3 例期望值更新
  为 VM 真值（#t）。
- **Type::Map**（typecheck 保险层，替代 kv effect 行）：借位计数
  丢失的 9 条真阳性经调查全部为 map 类型形状（map-get 收 i64）而
  非 kv 键序——A-SK effect 行以证据裁决不做；对症加 Type::Map +
  map-* 五词参数精确化：**TP 6→7、FP 保持 0**。
- **回归**：cargo 460/0；覆盖门禁 52/0/0；syntax 26/0、stdlib 58/0、
  net、app 四门全绿；typecheck-gate FP=0/TP=7；固定点
  gen2==gen3 OK。

## Unreleased (2026-09-10n) — N-2 Phase 2.5+4 收官：**strict 默认达成**（J-3 战役关闭）

J-3 归档为"不可修补"的目标经分阶段重设计达成：**type 错误默认致命，
`--no-strict-types` 降级逃逸**。三语料假阳性全零。

- **?? 臂逐槽合并**：两臂按槽位对齐合并（unify/union），不再只取
  last()——J-3 当年撤回是因被坏签名放大，Phase 2 freshen 后放大源
  已除，重试成功（t_mk4 双 FP 清零）。
- **内建签名实例化**：builtins 表的 TypeVar(99)/(0) 是固定 id，
  所有调用点共享同一变量、首点绑定污染全局（t_base 最后一条 FP
  的根）——与词签名同走 freshen。
- **借位计数**：infer_sig 模式下词体欠压操作（体首 `dup`/`drop`
  等）合成 fresh 输入并计数，签名输入数=借位数——J-3 "体首即消费
  给 0 输入"（tp-key 族）的修正。真阳性 15→6（部分真 bug 转为
  静默，如实记录）。
- **异构列表 union**：`[1 2 [3 4]]` 运行时合法——元素型升级 union
  而非报错（conformance 2→0）。
- **翻转**：main.rs 三处 flag 改 `!get_flag(--no-strict-types)`；
  帮助文本更新；覆盖门禁 VM 侧加逃逸口（行为对拍用例含刻意栈残留）。
- **验收**：cargo 460/0、syntax 26/0、stdlib 58/0、net、app、
  覆盖门禁 52/0/0、typecheck-gate 三语料 FP=0（TP 6 不罚）。
- 调试辅助保留：WHISPER_TC_TRACE=1 打印语句级类型栈。

## Unreleased (2026-09-10m) — N-2 Phase 2：多态实例化 + 假阳性普查（FP 仅剩 3）

- **调用点实例化**（checker.rs freshen）：WordRef 每次调用深拷贝
  签名并把 TypeVar 整体换新——消灭签名 var-id 被首个调用点绑定后
  全程序污染的一类。
- **决定性普查**：剩余 18 诊断 × 运行时成败交叉表——**假阳性仅 3**
  （t_base:1 + t_mk4:2，均为共享闭括号多 `??` 链的臂槽建模类，
  J-3 撤回项真身）；**真阳性 15**——检查器抓到的真 bug（t_q*
  全家 + t_ind6/s7，运行时确实错；早先手工 rc 读数有误，由门禁
  修正）。检查器的价值首次被定量。
- **门禁升级**：typecheck-gate 改为 FP 感知棘轮——运行时成功文件
  上的诊断才计 FP（CEIL_FP=3）；真阳性不惩罚。
- **回归**：cargo 460/0；覆盖门禁 52/0/0；encoding 0 /
  conformance 2 基线不动。
- **后续**：Phase 2.5（?? 臂逐槽建模）清 FP 3→0 即达 strict 默认
  前置。

## Unreleased (2026-09-10l) — N-2 Phase 1：引号栈合同——lexicon 诊断 32→18

- **消费者合同重设计**（checker.rs）：expect_ref 的 t→t 净零形状
  （J-3 战败根因之一）改为按消费者 arity——@map/@filter 至少 1
  产出、@each 产出不限（体合法地追加多输出）、@fold 2 提供槽 +
  至少 1 产出、@times 不设形状；产出检查带消费者视角换算
  （`o + (提供 − 声称)`，@fold 的 `{ + }` 正确过）。
- **引号体入口栈参数化**：从固定单元素栈改为（环境 Σ×4 + 元素
  槽×2）起查，`$N` 越过元素落到真槽；Ref 的 arity 按**低水位触底**
  构造（自产自弃不计触底），产出按入口边界切片。
- **关键语义发现**：VM Each/Fold 清栈单元素执行（父栈停靠）——
  引号根本够不到环境栈；入元数触底审计在该粒度下是噪声（t_q9
  实证），撤入检查、留给 Phase 2 按消费者入口重查。
- **战果**：typecheck-gate lexicon **32→18**（净清 14：t_ind10/
  t_ind10c/t_ind3/t_ind4/t_ind7/t_ind7a/t_ind9/t_dump/t_q9 全清，
  t_ind6/t_ind6s7 各 4→1）；conformance 2 / encoding 0 维持基线。
  剩余 18 全为槽污染类（t_q1-qc/t_mk4/t_base）——Phase 2 多态
  实例化的靶子。
- **回归**：cargo 460/0；覆盖门禁 52/0/0；stdlib 58/0；syntax 26/0；
  棘轮降档 18 并 PASS。

## Unreleased (2026-09-10k) — N-1 bool/atan2 语义统一（VM 协变）+ N-2 Phase 0 判据 harness

- **N-1（预注册判据全部兑现，tests/n1_preregistered.md）**：
  `#t 1 +`→2、`"ab" #t strnth`→98、`0 1 atan2`→0、`#t bitnot`→-2、
  `#t 0.5 +`→1.5——VM 三咽喉点 Bool 数值协变（binary_num_op 归一 /
  pop_i64 / pop_f64）+ equals 跨标签按数值比，与 C 链（bool 即
  i64）全量对齐。16 格行为矩阵实测兑现；永久分歧仅剩
  `#t .`（#t vs 1）与 `#t json-stringify`（"true" vs 1，wval 无
  bool 标签），spec §4 记录。
- **typecheck 数值协变**：infer.rs unify 加 Bool↔I64↔F64 两两可合
  六臂——静态假阳性清零优先，反向泄漏（f64 进 i64 位）由运行时
  动态检查兜底。诊断零扰动实证（N-1 前后双二进制 32/2/0 完全一致）。
- **N-2 Phase 0**：scripts/typecheck-gate.sh 判据棘轮——std/encoding
  向量链 0 / conformance 2 / **lexicon 32**（实测真基线；J-3 报告
  的 6 是当时语料规模，后续新增测试带入存量近似）。只许降不许升。
- **回归**：cargo 460/0；覆盖门禁 **52/0/0**（+bool 四用例）；
  Lexicon 双跑 0 diff/50。

## Unreleased (2026-09-10j) — M: map 顺序确定化 + ] 歧义文档化 + 后续战役登记

- **map 遍历顺序非确定（语言级修复）**：VM `Value::Map` 底层
  HashMap→BTreeMap——此前同一程序三次运行 map-keys 输出
  `["c" "a" "b"]`/`["a" "b" "c"]`/`["c" "a" "b"]`（RandomState），
  且与 C 链插入序不一致；现两实现统一为 bytewise 排序（C 链
  w_map_set 改排序插入）。覆盖门禁新增 map-order 双用例锁死。
- **`]` then-臂列表贴近闭合括号的两实现分歧（文档化已知限制）**：
  `#t ?? [1 2 3] ] ...` VM 解析器拒绝（Unexpected RBracket）、自举
  链接受——spec §2 补已知限制与规范写法（臂内列表后至少跟一词，
  或列表前置构造）。
- **后续战役登记（docs/language-backlog.md）**：N-1 bool/atan2
  语义统一（两后端行为分叉，`#t 1 +` VM 报错/C 链=2；含 16 格
  预注册判据矩阵要求）；N-2 typecheck strict（J-3 复活，前置为
  checker 重设计：多态变体推断+引号栈合同+kv effect 行）。
- **回归**：覆盖门禁 48/0/0；四 CI 门全绿；cargo 460 全绿；
  固定点 gen2==gen3 OK；Lexicon 50 测试双二进制对照输出
  **字节级一致**（BTreeMap 零行为扰动实证）。

## Unreleased (2026-09-10i) — L: c2 债务清偿——try/json/conf 全链落地 + 三笔 K 前旧债

覆盖门禁白名单清空：**46 pass / 0 fail / 0 known-gap**；CI 四门
syntax 26/0、stdlib 58/0、net、app 全绿。

- **try（L-1）**：runtime.c 引入 w_fail + setjmp/longjmp——值级错误
  （类型错、除零、栈下溢、JSON 错、listen 失败）回卷到最近 `{..} try`
  并压 [#f, 消息]；成功形状 [#t, 值] 与 VM Try 逐槽对齐（含空体默认
  0 与栈回卷深度）；cgen 发射 `w_try_call(_qb);`（嵌套 try、
  try-内-json 错误捕获实测通过）。资源级错误保持致命。
- **json（L-2）**：W_json_parse_cstr / W_json_stringify 全实现——
  递归下降（对象/数组/字符串/数/true/false/null，\uXXXX BMP）；
  紧凑序列化、对象键 bytewise 排序（serde_json BTreeMap 对齐）、
  f64 文本轮转对齐（4.0 保留 .0、1e30 无 +）；map 升级为标记前缀
  对表（magic pair 首元素）——map 与形似列表在 C 链可区分，L-08
  启发式腐坏不会复发；`.` 打印 {"k": v} 对齐 VM Debug。
- **置信度（L-3）**：`#t:0.75` 三处落地——tokenizer ':' 并入
  （累积为 "#t"/"#f" 时越过 is-ws 停止判定）、classify [14]/[12]、
  cgen 发射 w_sig 包装；WT_SIG 第四标签 + w_as_*/w_num_f64/
  wval_is_str/is_list/dump 透明解包；conf-of peek 不弹、unwrap 剥壳。
- **裸 HOP + PushRef（旧债 1，K 前即缺）**：`: map { @map } ;` 式
  包装词要求裸 @map/@each/@fold/@times/@filter/try(67/68/69/83/5/193)
  发射（引号运行时到栈）——补六个 emit-bare case 接 runtime 现成
  w_map_over 等；**引号作参数**（`{ 1 + } map`）的 PushRef(53) 发射
  从未存在——fuse-hof 的"无消费者引号"分支提升为顶层合成词 __qN
  （[16,名] 压引用 token + [9,名,体] 合成 def，split-defs 全流提取），
  新指令 [57] 发射 `w_push(W_PTR(W___qN))`。
- **K-1 克隆注释 bug（旧债 2）**：行尾 "// 注释" 让单行 def 判定
  失守，下一行 def 被原样吞进克隆体（stdlib/list 的 range-to 注释
  导致 `: filter` 双定义、gcc redefinition）——判定与体拷贝改按剥
  注释后代码。
- **杂项旧债 3**：atan2 弹栈序反了 + 改数值提升（VM 严格 f64、C 链
  提升并按门禁契约 0 1 atan2 → 0）；ffi-sym 该回 I64 地址（VM 语义，
  ffi-call 按 i64 收；一修双愈 sym-zero/crc32）；kv-keys 漏 W_PTR
  包装（server.ws 编不过）。
- **堆提交 bug（顺带修复）**：list arena 提交从基地址覆盖全跨度
  （旧 4GB 实为全量急提交，16GB 下 CI 必挂）——改字符串底部/列表
  顶部各自 64KB 切片懒提交。
- **方言纪律战报**：本轮五个自举栈序 bug 全部被"VM 先跑、原生对拍"
  抓住——kv 存值序（值在键下）、$N pick 压栈后索引漂移（$2 $2）、
  prepend 参数序（elem 在底 list 在顶）、over 在单元素栈越界（用
  `_`）、?? 包装漏闭合 ]（括号恰好重排进内层臂，语义静默错乱）。
- **教训（如实记录）**：K-6 验收清单漏跑了 stdlib/app 门禁——
  PushRef/裸 HOF/克隆注释三笔 K 前旧债因此带病入库（fa947de 的
  CI selfhost 应为红）。L 阶段补齐：本地 gates 复验入验收清单。
- **回归**：覆盖门禁 46/0/0；四 CI 门全绿；cargo test --workspace
  全绿；固定点 gen2==gen3 保持（L 变更后复验）。

## Unreleased (2026-09-09h) — K: J4-4 自举链同步收官（限定量词法全链落地）

- **自举链限定量词法（J4-4）**：`mmod.mw` 在 C 链词法器并入单词，
  独立 `.`/`..`/`3.14` 不受影响——与 Rust VM 语义一致。
  `: mmod.mw { 42 } ; mmod.mw .` 全链输出 42；`3.14 .` 双侧 3.14。
  compiler.all.ws 新增 is-wordch?/dot-joins?/chunk-cont?/chunk-or-delim
  四词（保守子集纪律：无 #t/#f 字面量、kv 只存字符串）。
- **两个根因（战报 docs/namespaces-design.md）**：
  1. seed 编译器对函数内 >2 层 ?? 嵌套产出错字节码（Rust 编译器
     容忍 → VM 侧全对、原生侧坏）——原 is-wordch? 5 层嵌套重写为
     &/| 扁平布尔链（零嵌套）；
  2. 自举词法逐字符 strslice 全剩余串 → 累计分配 O(源²)：50KB
     compiler.all.ws 自编译越过 2GB 字符串 arena（冻结种子在 60KB
     合成输入同样爆——非新词引入，链本就贴崖）。runtime.c
     HEAP_SIZE 4GB→16GB（VirtualAlloc/mmap 懒提交，实际内存不变）；
     生成 C 以 `#include "runtime.c"` 引用，gen2==gen3 文本比对不受
     影响，seed 文件保持冻结。
- **c2 管线命名空间注入（K-1）**：expand_imports 内联模块后追加
  限定克隆 `: 末段.词名 { ... } ;`——c2 目标编译含 import 源时
  限定调用可用。
- **操作码覆盖门禁（K-2）**：tools/opcode_coverage.sh——36 特性
  用例双跑对拍（VM vs 自举链 -c→gcc→run），归一化已记录打印口径
  （#t/#f→1/0、去引号）；NSC 环境变量注入预构建自举编译器；
  已知缺口（json/try/conf）白名单大声记账。现况 36 pass / 0 fail /
  3 known-gap。
- **静默兜底→硬失败（K-3）**：seed cgen 未知 token/裸 opcode 发
  `#error whisper-cgen: ...`——C 编译响亮失败，不再静默垃圾。
- **数值提升修复（K-4）**：seed cgen 的 `<=`/`>=` 用 w_num_f64
  （提升，对齐 VM binary_num_op）替代 w_as_i64（F64 硬失败）；
  浮点算术/比较/打印两实现一致（spec §4 浮点差异行删除）。
- **回归**：cargo test --workspace 460 全绿；固定点
  scripts/verify-fixedpoint.sh 通过；runtime.c 死代码 fn-trace
  调试脚手架移除。

## Unreleased (2026-09-04g) — J-4 模块命名空间（Rust 侧完整落地）

- 词法：`.` 在前后均为词字符时并入词（`mmod.mw` 单 token）；独立
  `.`/`..`/`3.14` 语义不变；5 项 lexer 单测。
- resolver 命名空间注入：import 时以路径末段注册，模块每个 Def
  克隆为 `末段.词名` 附加 Def——限定形态即普通词，永不被非限定
  冲突遮蔽；冲突语义（警告+后导出遮蔽）对非限定形态保持不变。
- 实测：std/str 与自定义模块同导出 find——str.find=1、
  mymod.find="MINE"、非限定 find=遮蔽者；双向限定同时正确。
- 向后兼容：workspace 20 套全绿；Lexicon 警告基线不变；examples 0。
- 自举链同步（J4-4）未达成：VM 侧语义全对，C 编译版崩——三个
  结构性缺口实锤（seed over 的 C-gen 静默垃圾、kv 只存字符串、
  export 过滤），修复后仍有未定位发射分歧；compiler.all.ws 回退
  保固定点。战报 docs/namespaces-design.md。spec §2 词法表补
  限定量行（标注自举链未同步）。

## Unreleased (2026-09-04f) — J-3 typecheck strict 默认：未达成，如实归档

- 保留（纯增量）：http-get/post/req/audit 内建签名入 builtins 表
  （此前未知词按 0 消费 1 产出 Any 建模，http-req 4→3 栈形全断）；
  docs/examples/api_client.ws 真 bug（`.` 在 `..` 清栈后调用）。
- 试过并回退（战报详见 docs/typecheck-strict-design.md）：
  expect_ref 放松、?? 臂逐槽合并、两遍法签名输入推断——三者能把
  Lexicon 误报压到 0，但 std/encoding 向量链在同一近似下反向失败；
  两个语料库对 checker 的压力方向相反，修补不可同时满足。
- 结论：strict 默认的前置（全套零误报）需要 checker 重设计
  （多态变体推断 + 引号栈合同 + kv 态词 effect 行），已立项存档。
- `--strict-types` opt-in 维持现状（6cffb62 起可用）。

## Unreleased (2026-09-04e) — 成熟化第一批：错误恢复栈卫生 + 堆按需提交

### try 错误恢复的两处栈卫生 bug（J-1）
- 引用块失败时数据栈回卷到进入前深度：此前半成品值残留污染恢复路径
  （`{ 1 2 3 0 / } try` 留下 [1,2]）；现只剩 [#f, 消息]；
- execute_ref_inner 错误出块时弹净本块调用帧：此前残留帧被外层主循环
  继续执行——try 捕获后引用块余下指令泄漏执行（`{ 1 0 / 5 }` 的 5
  仍会跑）；3 项回归测试（含 quote_double 空帧守卫）。

### C 后端堆 commit-on-demand（J-2 第一步）
- VirtualAlloc 从 MEM_COMMIT|MEM_RESERVE（一次性提交 4GB 提交额）改为
  MEM_RESERVE + 64KB 粒度按需提交两个 bump 方向触及的区间；Linux/macOS
  mmap 本就按需换页；soak：2M 次迭代 ~400MB 字符串 2.7s 跑完 rc=0，
  自举固定点不受影响。

### CI macOS 矩阵（J-5 可远程部分）
- test job 增加 ubuntu/macos 双矩阵（fail-fast 关闭）——三平台中唯一
  无法本地验证的平台由 CI 覆盖。

### J-3（strict 默认）与 J-4（模块命名空间）
- 体量大且 J-4 触及冻结语法（需双实现+门禁+重调模型），列入下轮专项；
- Rust 侧（whisper run）为 Rc 生命周期，内存本就回收——J-2 剩余
  （arena 回收策略/阻塞 TLS 并发化）随 daemon 化需求再议。

## Unreleased (2026-09-04d) — 说明文档对齐现状

- README：tests 徽章 392→451、stdlib 徽章 13→14 模块；能力沙箱行补
  HTTPS/host 白名单；置信度行更新为定稿四原语；
- docs/stdlib.md：std/http 表补 http-req(@6)/http-audit(@7) 与
  TLS/chunked/超时语义；
- docs/tutorial.md：HTTP 节补 https 与会话请求示例；新增 Confidence
  小节（std/conf 四原语）；
- docs/packages.md：路线勾掉 Lexicon 拉动包（五件已交付），补 sha256
  为下一项。

## Unreleased (2026-09-04c) — Lexicon 拉动包：jwt/html/csv + resolver 顺序 bug

### resolver 顺序 bug（语言级，I-2 过程中发现）
- 模块的 export 过滤此前把**传递依赖的词也滤掉**（jwt 递归装入
  encoding 后又被 jwt 自己的 export 名单丢弃），且 visited 已标记导致
  消费方再 import 时被跳过——import 顺序决定成败。重构为"传递词原样
  + 自有词按自身 export 过滤"两轨，顺序无关（双向测试锁死）。

### std/encoding 修复
- b64-dec 无填充余组（len%4 ∈ {2,3}）此前丢字节——b64url 无填充
  常态下载荷截断；补余组处理 + 无填充向量入测试。

### 新包（Lexicon 拉动）
- std/jwt：jwt-header/jwt-payload/jwt-claim（数字 claim 转字符串，
  缺键 → ""），VulnOA 真靶令牌验证；Lexicon tests/t_jwt.ws 冒烟闭环；
- std/html：html-text（剥标签 + 六个命名实体解码）/ html-attr
  （仅双引号属性，miss → ""）；
- std/csv：csv-parse（无引号最小集，如实文档化限制）。

### std/hash（md5）：入库（前一轮遗留已解）
- 纯 Whisper 全管线，RFC 向量 + 多块消息（200×a）全对。两处根因：
  I 轮函数应为 C ^ (B | **~D**)（此前漏了取反）；长度字节第 k 位
  移位丢了 8× 系数且顺序倒置（空串全零所以先"蒙对"）。

## Unreleased (2026-09-04b) — 包生态地基 + std/encoding

### 包管理端到端验收（此前从未跑通）
- import 解析补 `$WHISPER_HOME/packages/` 搜索路径（注释与实现不符，
  已装包此前不可解析）；
- `whisper install ./本地目录`：包作者开发流（不下载不删源），manifest
  从本地读，lock 记录来源路径；
- `install` 参数序修复（-y 被当包名）；本地包 → install → import →
  运行 / list / uninstall / 重装全链路冒烟通过；
- docs/packages.md：包标准（manifest/模块约定/B2 合同门/能力审查/
  native 库/路线）。

### std/encoding（样板包，纯 Whisper 零依赖）
- b64-enc/b64-dec/b64url-enc/b64url-dec（RFC 4648 含填充与 URL 安全
  字母表）、pct-enc/pct-dec（保留集 + %XX 大写十六进制）；
- RFC 向量 + 往返 + 字母表分歧用例锁进集成测试
  （encoding_stdlib_vectors）；docs/stdlib.md 补 std/encoding 与
  std/conf 条目。

### 修复过程中沉淀的栈编排铁律（新增三条，均踩实）
- 比较词会消费操作数：`dup 0 >= swap x <=` 吃掉被比较值——须
  `dup 0 >= over x <= &`；
- `$N` pick 从栈顶 0 基计数：`[a b c]` 中 a 是 `$2` 不是 `$0/$1`；
- kv 累积字符串时 `值 kv-get strcat` 是前插（值+旧），`kv-get swap
  strcat` 才是追加。

## Unreleased (2026-09-04) — `]` 歧义解禁、LSP stdlib 感知补全

### `]` 语法歧义（spec §3.3 已知限制解禁）
- 实测解析器已是括号深度感知：列表字面量在条件分支内任意位置
  （分支尾、elif 链、嵌套列表）Rust 链与自举链均正确解析。
  加 7 项回归测试锁定（parser::list_in_cond_tests），spec.md 更新。

### LSP 补全增强
- import 路径补全：光标在 `import "..."` 串内时列出 std/* 模块与
  同目录 .ws（纯函数化 completion_items，可单测）；
- 模块感知：文档 import 了 std/X 后，补全提供该模块导出词
  （解析 stdlib 源的 export 列表）；
- 内建表补 30+ 缺失词（kv-*、map-*、字符串/列表补遗、位运算、
  时间/随机、置信度定稿原语 conf-of/unwrap、http-req）；
  5 项补全测试。

### 词级热编译：立项决策文档化
- docs/hot-word-design.md：机制、四项前置条件、风险、重启触发器；
  维持不动工（当前无 VM 计算瓶颈负载，-t native 缓存已覆盖主要收益）。

## Unreleased (2026-09-03b) — typecheck 语义修复、HTTP TLS+chunked、置信度定稿

### typecheck
- `@nth` 元素类型传播（List(T) 索引结果 = T；此前丢失致 `@nth len` 漏报）；
  `append` 元素与列表元素类型统一（`[1 2 3] "x" append` 现在报错；结果类型保持）。
- occurs-check 核实已在 unifier 层（`t := List(t)` 拒绝）并补直测；递归调用
  的 fresh-变参宽容策略不变（防误报的既定设计）。
- 新增 `--strict-types`：类型错误致命化（默认仍为警告打印并继续）。

### HTTP TLS + chunked
- `https://` 全链路支持（rustls 0.23 ring 后端 + webpki 根 + ALPN http/1.1），
  此前 @2/@3 明拒、@6 对 443 端口发明文（静默错误）。
- `Transfer-Encoding: chunked` 自动解码（@2/@3/@6 三路径，截断容错）；
  TLS 侧显式驱动握手（write 在握手期返回 Ok(0) 的自旋陷阱）+ 对端缺
  close_notify 容忍。
- 封闭集成测试：rcgen 自签服务端 + 注入根的客户端 + chunked 响应断言。

### std/conf 定稿
- 官方表面：`<value>:<conf>` 标注、`conf-of`（窥视，不消耗）、`unwrap`（剥壳）、
  `p {a} {b} ?|`（显式概率分支，L-03 语义）+ 组合词 `conf-mul`/`labeled?`；
  文档化"浮点不进 kv，持久化用千分比"铁律。

## Unreleased (2026-09-03) — C 后端语义统一 + native/exe 分发目标

### C 运行时值带类型标签（三后端算术逐位一致）
- `wval` 从裸 uint64 改为 `{v, t}` 结构（WT_I64/WT_F64/WT_PTR），构造子
  W_I64/W_F64/W_PTR + 访问器；`+ - * / % < >` 发射为 tag 分派的
  `w_op_*()`（任一侧 f64 即按 VM binary_num_op 提升），此前
  `1.5 2.5 +` 在原生侧静默输出位模式垃圾（I64_MIN 级）而 VM 输出 4。
- whisperc/runtime.c、Rust transpiler、自举发射器（cgen.ws +
  compiler.all.ws 全部 opcode 片段）三处同步转换；字符串排序改为
  字节序（与 VM 一致，此前按指针位排序——陈旧错误）。
- 已知残留（文档化）：字符串打印无引号、`=`/dedup 值位等同而非深度
  相等、f64 打印 %g 与 Rust Display 在极端量级上有格式差。

### 种子重生为纯生成 C
- `whisperc/seed/compiler_seed.c` 此前是 1071 行手工移植的 C 前端；
  现为 `whisper build -t c2 compiler.all.ws` 自编译产物的冻结快照
  （188KB，100% 生成代码），自举固定点 gen2==gen3 保持。
  scripts/bootstrap.sh 补 MINGW 的 -lws2_32。

### 新分发目标
- **`-t native`（安装期硬件特化）**：c2 链 + 宿主 `cc -O3 -march=native
  -flto`，内容寻址缓存（键=展开源+编译器+runtime+cc+march 指纹）；
  同机重建 16ms 缓存命中，源/编译器/CPU 任一变化自动重编。
  `WHISPER_NATIVE_MARCH` 可覆盖（如 x86-64 出通用包）。
- **`-t exe`（自包含单文件）**：SBCL/Starkit 式——当前二进制副本 +
  尾部 wbin 负载 + 8 字节 magic trailer；启动时自检尾部即运行内嵌
  字节码。零手写 ELF/PE/Mach-O，Windows/Linux 实测通过
  （macOS 需重签名）。import 在构建期烘入，无需工具链。

### 测试
- 新增 `-t exe`（运行对拍 VM + trailer 布局）与 `-t native`
  （缓存命中 + 浮点对拍）集成测试；全套 whisper-cli 98 过；
  workspace 20 套全绿；bootstrap.sh 五阶段 + verify-fixedpoint.sh 绿。

## Unreleased (2026-08-21) — 修复与收敛

### 修复（P0 级）
- `whisper run` 在 HEAD 上完全不可用（`over` 关键字化与核心词表冲突）。
  `TokenKind::keyword_name` 成为关键字↔def 名往返的单一事实源；`rot`/`return`/
  `split-cond`/`list?`/`i64?`/`str?`/`bool?` 一并关键字化。
- whisperc 管线恢复：6 月版 parser.ws 的 elif 链缺 `]`、main.ws 被掏空丢失
  `compile`、PushList/字节映射缺失、op-quote 双发射——全部修复，31 个自举测试
  由 11/31 → 31/31。
- 优化器删除指令后不再产生错位跳转：每个 pass 携带新旧索引映射，
  relocate 统一修正 Cond/Jump/Loop 偏移（此前任何带循环/分支的程序经
  build 产出错位字节码）。
- VM：`@map`/`@fold` 等高阶操作不再每元素深拷贝整栈（O(元素×栈深) → O(1)）；
  引号嵌套 512 层上限防宿主栈溢出；StrSlice 拒绝非 UTF-8 边界（原为 panic）；
  负数索引/长度显式报错；`[1 2] [1 2] =` 现在为 `#t`（结构化相等）。
- 能力白名单：词法规范化路径后 `..` 穿越与前缀兄弟目录被拒绝（首个单测覆盖）。

### 移除
- ELF 直编后端（native_gen.rs ~3200 行，产物无法运行）及其前身 src/native/。
- c-whisper / c-native 衍生目标、不可编译的 compiler.c、漂移的快照文件。
  净删约 5700 行。`whisper build` 目标：`wbin` / `c` / `c2`。

### 新增
- **c2 目标**：`whisper build file.ws -t c2 -o prog` —— import 递归内联 →
  自举编译器（compiler.all.ws）在 Rust VM 上运行生成 C → gcc 一键产出原生
  可执行文件。stdlib/math（浮点/位运算）全量走通。
- Rust parser 采纳种子条件语法：平铺切分 + 子区间递归，同时接受严格嵌套与
  共享闭括号 elif 链（compiler.all.ws 首次可在 Rust parser 下完整解析）。
- 浮点字面量进入自举链（classify→C 双字面量插值）；`<<`/`>>` 符号等价 shl/shr。
- 自举链结构守卫：以 `{`/`[` 开头的程序不再令 classify 崩溃。
- C 转译器（-t c）：quote 一等化为 C 函数指针，高阶操作接线 runtime.c，
  TODO 项改为显式 `#error`；12 个端到端 gcc 测试。
- CI：whisper-cli 集成测试不再被排除；新增 selfhost（种子编译 + 22 用例双模式
  + 固定点字节级复验）与 c-backend 两个 job。
- 固定点在上述改动后复验成立（gen2 == gen3）。

### 已知限制（诚实口径）
- 自举链（c2）尚不支持高阶函数（cgen 缺 quote 发射，输出 `// unknown`）；
  Rust C 转译器（-t c）已支持。
- C 运行时值无类型标签：浮点以位模式流转，`.` 打印为整数；混合类型算术
  经显式 f-opcode（i64tof64/fsqrt/f64toi64）工作。
- 两后端 `.` 的字符串格式不同（VM 带引号，C 链裸文本——自举链 driver 依赖之）。
- 类型检查器仍为警告通道（放水项未修，见计划「明确不做」）。

# Changelog

## v1.0.0 (2026-06-06) — First Stable Release

### Language Syntax (Frozen)

| Category | Operators |
|----------|-----------|
| Stack | `_` dup, `` ` `` swap, `drop`, `@` rot, `$n` pick |
| Arithmetic | `+` `-` `*` `/` `%` (mod) |
| Comparison | `=` `<` `>` `!=` `<=` `>=` |
| Logic | `&` and, `\|` or, `!` not |
| Control | `??...\|...]` if/else, `#` loop, `?->` single-branch |
| Lists | `[ ... ]` literal, `@nth` `append` `len` `@map` `@each` `@fold` |
| Words | `: name { body } ;` define, `import` `export` |
| Capability | `@n` call, `!` exec |
| Confidence | `:0.xx` label, `?\|` probabilistic choice |
| IO | `.` output top, `..` output all, `,` read input |

### Core Features

- **VM**: 56+ opcodes, stack-based, call/return frames, recursion
- **Parser**: Recursive descent lexer + Pratt parser for full .ws syntax
- **Type Checker**: Stack-effect validation at compile time
- **Compiler**: Two-pass bytecode generation with constant folding & peephole optimization
- **.wbin Format**: LEB128-encoded binary with magic header
- **WASM Target**: Direct compilation to browser-executable .wasm (90 bytes for hello world)
- **Capability Sandbox**: File I/O, HTTP GET/POST with host/path whitelists

### Self-Hosting

- `whisperc/main.ws`: Whisper compiler written in Whisper (10 lines)
- `whisper bootstrap`: Rust Lexer → Whisper Compiler → VM Execute pipeline
- hello.ws self-compilation verified

### CLI Commands

```
whisper run       file.ws    Execute
whisper build     file.ws    Compile to .wbin/.wasm
whisper check     file.ws    Type check
whisper serve     file.ws    HTTP server
whisper repl                 Interactive REPL
whisper fmt       file.ws    Format
whisper install   pkg        Package manager
whisper bootstrap file.ws    Self-hosting pipeline
```

### Standard Library

| Module | Functions |
|--------|-----------|
| `std/math` | sq, cube, abs, factorial, fib, even, odd |
| `std/str` | strlen, strcat, strdup |
| `std/list` | length, push, map, each, fold, sum, product, reverse |
| `std/io` | read-file, write-file, println |
| `std/json` | json-parse, json-stringify |
| `std/test` | assert-true, assert-false, assert-eq |

### Tooling

- VS Code extension: syntax highlighting, bracket matching, code folding
- Web Playground: interactive editor with built-in examples
- CI/CD: GitHub Actions (test + lint + release)
- Package registry: Git-based with capability review

### Testing

- 63 unit/integration tests
- 2500+ fuzz tests (random program generation)
- 6 working examples: hello, factorial, fib, sum, map, http_get

### Performance

- Constant folding: `3 4 +` → `7` at compile time
- Peephole optimization: `dup drop` → (nothing), `0 add` → (nothing)
- Strength reduction: negative add → subtract
- Direct WASM compilation: 90 bytes vs 2800+ with embedded interpreter

## v0.2.0

- Breaking: `%` changed to Modulo, `drop` keyword added for stack discard
- Self-hosting compiler (whisperc), bootstrap pipeline
- Package manager (whisper install), VS Code extension
- WASM end-to-end verification, conditional branching fixes
- List ordering fix, recursive functions, standard library expansion

## v0.1.0

- Initial release: core VM, parser, compiler, .wbin format, basic CLI
