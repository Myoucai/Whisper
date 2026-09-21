# 战役 O 预注册判据（先落判据，后动手）

2026-09-21 立项。仿 `tests/n1_preregistered.md` 的做法：**先把验收判据写死，
再改代码**，避免事后按结果调整标准。

> **执行结果（2026-09-21，同日回填）：部分达成。**
> O-2 及三处编译器缺陷已修复并落地；O-1/O-3 精确定位到根因但**未修复**，
> 转入独立立项。逐条对照见文末「执行记录」。判据原文一字未改，
> 未达成的条目**不降级、不加白名单**。

## 背景

自举链（`-t c2` / `exe` / `native`）能编「引号被高阶词消费」的程序
（`@map`/`@fold` 编译期已知 → 融合成嵌套函数），也能编**单个**引号字面量
作为值。但下列形态让 `whisperc compile-full` 报
`Type mismatch: expected [T], got str`，连 C 都产不出来。

## 判据（全部为机器可验的断言）

### J1 —— 三行失败形态在两链输出一致

对下列三例，`whisper run`（VM）与 `-t c2` 产物的 stdout **逐行一致**
（归一化规则同 `tools/opcode_coverage.sh`：去 CR、去引号、去 `#t`/`#f`
布尔拼写差异）：

| 用例 | 源码 | 期望 |
|------|------|------|
| O-1 列表取引号 | `[{ 1 + }] 0 @nth 5 swap call .` | `6` |
| O-2 两个裸引号 | `{ 1 + } { 2 * } drop drop 7 .` | `7` |
| O-3 列表分发 | `[{ add } { mul } { sub }] 2 @nth 20 8 @ call .`（前置 `: add { + } ; : mul { * } ; : sub { - } ;`） | `12` |

### J2 —— 覆盖门禁纳入这三例

`tools/opcode_coverage.sh` 增补 O-1/O-2/O-3 三例，门禁结果
`PASS 不降`（当前 59 → 62），`FAIL=0` 且 `known-gap` 保持为空。
**不允许把失败用例登记进 `KNOWN_GAPS` 白名单**——那是把判据改成能过。

### J3 —— 固定点复验

`scripts/verify-fixedpoint.sh` 报 `gen2 == gen3`。因为本战役必须改
`whisperc/compiler.all.ws`（固定点基准），并重生
`whisperc/seed/compiler_seed.c`，所以这条是**必须通过**而非"顺带确认"。

### J4 —— 既有门禁零回归

以下全部维持战前水平，不得以"改了编译器"为由放宽：

    cargo test --workspace       473 / 0（若新增测试则更多）
    cargo clippy -D warnings     0
    cargo fmt --all --check      0
    syntax-gate                  26 / 0
    stdlib-gate                  58 / 0
    typecheck-gate               FP=0（TP 不罚）
    net-gate / app-gate          PASS / PASS（13 路由）
    opcode_coverage              PASS=62 FAIL=0 GAP=0

### J5 —— 种子可复现

重生后的 `whisperc/seed/compiler_seed.c` 必须**可由仓库内工具链重新生成**
（`whisper build -t c2 whisperc/compiler.all.ws` 的输出与它一致），
即种子仍是纯生成物、不含手工编辑。判据：记录重生命令与两次生成的哈希。

## 反向判据（什么算失败，提前写明）

- 为通过 J1 而把 `KNOWN_GAPS` 加白名单 → **失败**。
- 为通过 J1 而放宽 VM 侧行为使其与 C 链"一致地错" → **失败**。
- 改 `compiler.all.ws` 后固定点不再成立且无法在**本战役内**修复 → 回滚，
  如实登记为"需要更大改动"，不得留下半成品提交。

## 附：战役顺带项（做得到就做，做不到不影响判据）

同一批工作本可一并解决（都卡在同一次种子重生上）：

1. `@n` 在 classify 阶段正名为能力令牌，cgen 发射 `w_cap_call(id)`，
   取代目前 runtime.c 兜着的 `W__<n>()` 误发。
2. `-t c`（Rust 转译器）对 `CapExec` 发射空操作
   （`transpiler.rs: // cap_exec: no-op in native mode`）。

**这两项不作为本战役判据**——它们扩大改动面，而 J3 要求固定点在战役内成立。
若一并做，必须同样满足 J3/J4。

---

## 执行记录（2026-09-21）

### 逐条对照

| 判据 | 结果 | 说明 |
|------|------|------|
| **J1** 三例两链一致 | **部分** | **O-2 未完全达成**：两个引号 ✅ 两链均为 7；**三个及以上引号仍失败**，**O-1/O-3 未达成**（根因见下） |
| **J2** 覆盖门禁纳入 | **部分** | 只纳入"两个引号"一例（`PASS 59 → 60`）；≥3 引号与 O-1/O-3 **未纳入**——没通过就不进门禁，也未加白名单 |
| **J3** 固定点 gen2==gen3 | ✅ | 见 CHANGELOG |
| **J4** 既有门禁零回归 | ✅ | cargo 474/0、clippy 0、fmt 0、syntax 26/0、stdlib 58/0、typecheck FP=0、net/app PASS、覆盖 60/0/0 |
| **J5** 种子可复现 | N/A | **未重生种子**。证明：种子自 2026-09-03 冻结以来，`compiler.all.ws` 已改动多次而固定点始终成立——种子是链的起点、不是编译器的副本，改 `.ws` 不需要重生它（`runtime.c` 由 `#include` 现场取用，同理）。 |

### 顺带发现：门禁的 c2 腿与用户路径不等价

`tools/opcode_coverage.sh` 的 c2 腿走 **NSC**（把 `compiler.all.ws` 先编成
原生二进制，再 `NSC -c < src`）；而用户执行的 `whisper build -t c2` 会**先
内联 `std/core`** 再喂给编译器。三个引号的程序在 NSC 路径下**成功**、在用户
路径下**失败**（`Type mismatch: expected str, got [T]`）。

这是一个**门禁盲点**：NSC 通过不等于用户能过。本次据此**拒绝**把
`quote-triple` 登记进门禁——它在门禁里会绿，但对用户是红的。
（修门禁本身——让 c2 腿也走内联后的源码——属独立事项。）


### 修复的三处编译器缺陷（均已落地）

1. **`cl-list` 缺 `structify`**（`classify-nested`）：`cl-quote` 是
   `dup 1 @nth structify classify-nested`，`cl-list` 却只有
   `dup 1 @nth classify-nested`。收集器把方括号内的 token 留成裸串
   （`cbr-loop` 只归组嵌套方括号、不认花括号），少了这一趟 `structify`，
   列表元素里的 `{` 就被分类成**普通词**而不是引号节点。
   修复后 `[{ 1 + }]` 的分类树为 `[6 [[5 [[0 1] [3 16]]]]]` ✅
2. **`next-consumer-op` 越界**：`dup 1 @nth` 没查长度。当引号恰好是最后
   一个 token 时（`{ 1 + }` 单独成程序即可触发）整个编译器崩在
   `Index out of bounds`。加了长度守卫。
   （附带修掉我第一版守卫里多余的 `drop`——`??` 已消费条件，那个 `drop`
   吃掉的是 token 列表本身；是 `--trace` 抓出来的。）
3. **`genq-name` 残留计数器**：非首次调用时，计数器字符串既没被消费也不再
   需要（它已经是分支的输入），却被多读一次 `"__QN"` 留在栈上。单引号看不
   出来（真分支有 `drop`），**每多一个引号就多一份垃圾**，把真正的操作数挤
   出可达范围 → `Type mismatch: expected [T], got str`。修复：直接用栈上
   那个值，去掉冗余重读。

另外把 `fuse-hof` 的 `__FH-N` **共享 KV 临时键**去掉——它在递归调用中会被
覆盖，名字改为全程留在栈上。这是同一类"用全局暂存传值"的隐患。

### O-1/O-3 的根因（未修，已定位）

`op-quote` 发射 `[53, inner_bc]`，但 **`53` 在整个 `compiler.all.ws` 里只出现
在 `op-quote` 自己那两行**——cgen 没有任何 53 分支，只会走到
`#error whisper-cgen: unknown bare opcode`。也就是说 `op-quote` 目前是**死
代码**：引号能工作，完全依赖 `fuse-hof` 把它提升成 `[57, name]`（cgen 支持的
ref 形态）+ 一个 `[9, name, body]` 定义。

而 `op-list6` 用 `compile`（= `fuse-hof` + `compile-loop`）编译列表元素，
所以 `[{ 1 + }]` 的元素里确实被提升出了 `[16 "__q0"]` + `[9 "__q0" body]`；
但 `compile-one` **有 tag 16 的分支、没有 tag 9 的分支**，于是定义被丢弃、
引用留着 → 生成的 C 里 `w_push(W_PTR((void*)W___q0))` 指向一个从未发射的
函数，gcc 报 `'W___q0' undeclared`。

**修法方向**（两条，任一都需要独立设计）：
- 让 `fuse-hof` 在遇到列表节点时**下潜**，把其中提升出的 Def 抬到顶层，
  使 `split-defs` 能收到（改动集中但触及最热的函数）；或
- 让 `compile-one` 支持 tag 9（把嵌套定义汇入 defs 累积器）+ 给 cgen 补
  opcode 53 的发射（内联引号需要合成函数名，是两遍式改动）。

按反向判据第三条，**未达成即如实登记，不留下"看起来做了"的状态**：
本战役落地的是三处**独立成篇、可自行验证**的缺陷修复，
O-1/O-3 转为独立战役（见 `docs/language-backlog.md` 战役 O′）。
