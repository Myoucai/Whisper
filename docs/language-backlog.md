# 语言级后续战役（backlog）

2026-09-10 盘点确立。此处登记**已实锤、暂不实施**的语言级问题——
每项含事实、选项与预注册判据要求；开工时先落判据再动手。

## 战役 S-1：非 ASCII 字符串语义两链分叉（**2026-09-21 发现，建议优先**）

**事实**：收口战役收尾做端到端实证时发现，`"你好" .` 经 `-t c2` 编出来的 C
里字面量只有 2 字节（`E4 E5`），而不是 6 字节。完整矩阵与复现脚本见
`tests/closeout_preregistered.md` 的 S-1 节（`bash target/l1/cjk.sh` 一键复现）。

**严重性**：这不是外观问题——**Rust CLI 这条路径会静默写坏任何含非 ASCII
字面量的程序**。同一个程序用 `wd`（原生 cgen）编出来是正确的。

**根因（待确认）**：两条运行时对"字符串是字节序列还是字符序列"没有共识，
而且各自内部也不一致——VM 的 `StrLen` 是字节（`s.len()`）、`StrNth` 注释写
明 "byte semantics (§3.5)"，但 `StrChars`/`StrIter` 用 `chars()`；C 侧
`w_strchars` 拿 `w_str_len` 当计数却用 `s[i]` 做字节下标。引号转义
`escape-c-str` 正好走在这条不一致的路径上。

**为什么单独立项**：要先**定语义**（spec §3.5 说字节），再统一
`strlen`/`strnth`/`strslice`/`strchars`/`charsstr`/`striter`/`escape-c-str`
在两条运行时里的实现，并复跑覆盖门禁与全部语料。仓促改会把"两边都错得不一样"
变成"两边都错得一样"，比现状更难发现。

**预注册要点**：① 先写死"字节语义"的判定表（上表每一项的期望值）；
② 两链同时改、同时验；③ 覆盖门禁补非 ASCII 用例（现有 `str-*` 用例全是
ASCII，这正是它长期没被发现的原因）；④ 复跑固定点与六闸。

## 战役 L-1：`exit` 与 `print` 两个原语缺失（**2026-09-21 已关闭**）

**判据与执行记录**：`tests/l1_preregistered.md`（E1–E4 已预注册并回填实测）。

**状态**：已达成并收口。opcode `0x93` = `Print`、`0x94` = `Exit`，两链同语义；
`wd` 的错误路径现在有真退出码（1 = 构建/IO 错，2 = 用法错）。E1（stdout 逐字节）
4/4、E2（真退出码，含"不提前 return"的 X-3）3/3 两链全中；六闸 + 87 项覆盖 +
固定点全绿。**E4-c 只达成 `wd` 那一半**，根因在上游能力 `@5`，见下 W-5。

**事实（写 `wd` 时撞出来的，2026-09-21 实测）**

1. **C 链程序没有退出码**：`cgen` 固定发射 `return 0;`，Whisper 层没有
   `exit`/`quit` 一类的词。实测 `wd build nope.ws out` 打印了
   `wd: cannot read source: …` 却仍然 `exit=0`——**一个不能报失败的 CLI
   工具不是工具**。
2. **没有裸打印词**：打印只有 `.`（`OutputTop`，自带换行，且两链对字符串引号
   的处理本就不同）；`striter` 不是打印词而是字符串**迭代器**（推
   `[字符码, 余串]`）。于是 `wd run` 转发子进程输出末尾会多一个换行，
   **做不到逐字节转发**。

**设计决定（已写进预注册，实施时未回退）**：走**正规 opcode 路线**，
不用 `@n` 能力 ABI 凑。`@n` 的语义是"这个副作用需要显式授权"，而"退出进程"
"打印字符串"不是需要授权的事——塞进能力 ABI 会让每个用到的程序都要
`--allow-…`，那是把设计错误固化。

**规模（实测）**：新增一个 opcode 实际动了 **14 个 Rust 文件 + 1 个自举链文件
+ `runtime.c`**——预注册写的是 11+2，漏了 `lexer.rs` / `token.rs` /
`lsp/lib.rs` 三个"词形层"文件（清单修正记在预注册里）。改
`compiler.all.ws` 意味着**必须重跑固定点**（约 25 分钟）。这不是"顺手加个词"，
是一次独立战役——事后确认这个判断是对的。

**为什么值得做**：它是 `wd` 从「能用」变成「能当工具用」的前置；也是任何
Whisper 写的 CLI 的前置。

**顺带修掉的真实双链分歧**：C 链在 Windows 上把 stdout 上每个 `\n` 改写成
CRLF，而 VM 从不翻译——这条分歧长期靠 Rust 测试里的 `.replace(['\r','"'], "")`
抹平。`print` 要求逐字节，绕不过去，已在 `w_init` 里把 stdout 设为二进制模式
一次性解决（细节见预注册执行记录：惰性切换是错的，因为文本翻译发生在 flush
时，会让输出取决于后面有没有调用 `print`）。

## W-5：能力 `@5` 吞掉子进程输出的尾部空白（**2026-09-21 已关闭**）

**状态**：收口战役 W6 解决。**没有改 `@5`**——`trim` 是命令替换惯用法依赖的
行为（`"cmd" @5 1 @nth strtoi64` 不必自己剥换行），取消它会把"修一个洞"
变成"改一批调用方"。改为**新增 `@10` = `exec-raw`**：同一份实现、同一份
授权（CLI 绑 `@5` 时一起绑 `@10`），只是两个流原样返回。

判据见 `tests/closeout_preregistered.md` 的 W6（a–e 全部满足）：
`"echo x" @10 1 @nth strlen` → 3、`@5` 同命令 → 1，两链一致；未授权时
`@10` 与 `@5` 一样被拒（`Capability not bound: @10`），不是绕过沙箱的后门；
`wd run` 的转发段与直接运行产物逐字节相同（L-1 的 E4-c 就此收口）。

**顺带修掉两处被 trim 掩盖的 C 链缺陷**（`@10` 一上就把它们照出来了）：

1. 命令包裹写成 `( cmd )`，`cmd.exe` 的 `echo` 会吞掉行尾——`echo x` 实际
   产出 `x `（多一个空格）。改成 `(cmd)`。
2. `_popen` 的读端是文本模式，会把子进程的 CRLF 折叠成 LF。加了
   `_setmode(_fileno(p), _O_BINARY)`。

（原登记内容保留在下面，供对照。）

**事实（L-1 实测）**：`@5` 返回的 `[status out err]` 里，`out`/`err` 都经过
`trim()`——VM 侧 `capability.rs` 的 `String::trim()`，C 链 `runtime.c` 的
`w_trim_inplace`，两边注释都写明是刻意对齐（"mirror Rust 的 `String::trim()`"）。
于是 `"echo hi" @5 1 @nth strlen` 得 2 而不是 3。

**影响**：任何"原样转发子进程输出"的工具都做不到逐字节——`wd run` 就是第一个
撞上的（L-1 的 E4-c）。

## 战役 N-1：bool / atan2 语义统一（**2026-09-21 核实为已闭合**）

**状态**：2026-09-21 收尾核查时实测，预案里的三个咽喉点两链**已全一致**——
`#t 1 +` → 2（VM 与 C 链同值）、`#f 5 -` → -5、`0 1 atan2` → 0、`#t 1 =` → `#t`。
即"VM 放松（bool 按 1/0 参与算术、atan2 接受 i64）"这条推荐路线已经落地。
覆盖门禁的 `bool-arith`/`bool-sub`/`bool-bit`/`bool-f64`/`bool-atan2` 用例
一直在这条契约上（90 例 0 fail）。

**唯一的残留不是 bool 语义**：`#t json-stringify .` 两链输出仍有差异
（VM `"true"` 带引号、C 链 `true` 不带）——那是 C 链 `.` **打印字符串不带引号**
这条已知分歧（README 有记），与 bool 无关。

（原登记内容保留在下面，供对照。）

**事实（2026-09-10 实测）**

- `#t 1 +`：Rust VM 报 `Type error: arithmetic lhs: expected
  i64 | f64, got bool`；C 链输出 `2`（bool 即 i64，无独立标签）。
  同族：`#t json-stringify` → VM `"true"` / C `"1"`（spec §4 已记录）。
- `0 1 atan2`：VM 报错（严格 f64）；C 链数值提升输出 `0`
  （stdlib 门禁契约，spec §4 已记录）。

根因是类型模型不对称：VM 有独立 `Value::Bool`，C 链 wval 只有
I64/F64/PTR/SIG 四标签、**无法表达独立 bool**。

**选项**

1. VM 放宽（bool 参与算术按 1/0 提升；atan2 接受 i64）——与 C 链
   对齐，语言语义变宽；需过 typecheck/builtins 表全量回归。
2. C 链收紧——需要第五值标签 WT_BOOL，牵动全部 w_as_*/打印/eq，
   发射体积上涨；语义最严。
3. 维持现状 + spec §4 记录（当前状态）。

**判据要求（预注册）**：选定方向后，先在 tests/ 落判据文件——
bool×{算术,比较,json,打印}×{VM,C 链} 的 16 格行为矩阵 + Lexicon
两语料库误报数不升。

**实施方案（2026-09-10 规划，推荐 A——VM 放松）**

VM 侧三咽喉点各加一行 Bool 协变，即可全量对齐 C 链（bool 即 1/0）：

1. `binary_num_op`（vm.rs ~1846）：match 加两臂
   `(Bool(a), I64/F64(b))`/`(I64/F64, Bool)` → 按 1/0 提升；
2. `pop_i64` / `pop_f64`（~1826/~1836）：`Value::Bool(b) => Ok(if b
   {1} else {0})`——atan2/位运算/索引等全部 pop_* 站点自动受益；
3. `Value::equals`（value.rs ~70）：`(Bool, I64)`/`(I64, Bool)`
   跨标签按数值比（`#t 1 =` → #t，与 C 链 1 一致）。

**明确不动**（永久分歧，维持 spec §4 记录）：`#t .`（#t vs 1）、
`#t json-stringify`（"true" vs "1"）——修复需 C 链第五标签
WT_BOOL（收益低、触面大，另立项才做）。

**typecheck 侧**（与 N-2 的热身）：checker 的 expect/协变助手中加
规则 `Bool <: I64 <: F64`（数值位接受 Bool）——builtins 表签名不用改。

**步骤**：① 预注册 16 格矩阵（tests/n1_preregistered.md，含
Lexicon 双跑差异预告——曾依赖报错的测试列出白名单）→ ② VM 三点
→ ③ typecheck 协变 → ④ 覆盖门禁补 bool 算术用例 + Lexicon 双跑
复核（预期 diff 仅白名单内）→ ⑤ spec §4 行更新。约半个战役日。

## 战役 N-2：typecheck strict 默认（J-3 复活）

**事实**：详见 docs/typecheck-strict-design.md 战报——expect_ref
放松、?? 臂逐槽合并、两遍法签名推断三者可把 Lexicon 误报压 0，
但 std/encoding 向量链反向失败；两语料库对 checker 压力方向相反，
修补不可同时满足。

**前置**：checker 重设计——多态变体推断 + 引号栈合同 + kv 态词
effect 行。非修补可达，需独立设计文档 + 分阶段实施
（先 effect 行，后变体推断）。

**实施方案（2026-09-10 规划：四阶段，每阶段独立验收可停）**

现状锚点：checker.rs 1185 行（全局约束求解 + Union-Find，
Type 已有 TypeVar/Ref(in,out)/Union）；基线 = Lexicon 6 警告
（t_ind6s7:4 / t_ind10c:1 / t_ind10:1，**全部引号栈近似类**）+
std/encoding 255-ctos 向量链 0 错误——两语料反向拉扯是战败根因。

- **Phase 0 判据 harness（先于一切）**：scripts/typecheck-gate.sh——
  strict 模式跑 Lexicon 50 + std/encoding 链，机器断言
  「Lexicon 错误=0 且警告≤6 且编码链错误=0」。此门为后续每阶段
  的通过条件，防"压一头翘一头"。
- **Phase 1 引号栈合同（2026-09-10 完成，7f1f59e）**：消费者合同
  按 arity（@each 产出不限等）+ 引号体（环境×4 + 元素槽×2）参数化
  + 低水位触底 Ref 构造。实测 lexicon 诊断 **32→18**（棘轮同步
  降档）。战果注：VM Each/Fold 清栈单元素执行——入元数触底审计
  在检查器粒度是噪声，撤、留 Phase 2 按消费者入口重查。
- **Phase 2 多态实例化（2026-09-10 完成，基础设施+普查）**：调用
  点 freshen（深拷贝签名换新 TypeVar）落地——修掉 var-id 共享污染
  一类；**决定性普查**：剩余 18 诊断交叉运行时成败后 FP 仅 3
  （t_base:1/t_mk4:2，均为共享闭括号多 ?? 链的臂槽建模类），
  真阳性 15（检查器抓到真 bug——t_q* 全家 + t_ind6/s7 运行时真错）。
  门禁升级为 FP 感知棘轮（CEIL_FP=3）。
- **Phase 2.5（2026-09-10 完成）**：?? 臂逐槽合并（J-3 撤回项，
  freshen 基础设施后放大源已除，重试成功）+ 内建签名实例化
  （TypeVar(99) 固定 id 共享污染）+ 借位计数（词体欠压操作合成
  fresh 输入——"体首 dup 给 0 输入"修正，tp-key 族）+ 异构列表
  union（[1 2 [3 4]] 运行时合法）。**FP 3→0**。
- **Phase 4（2026-09-10 完成）**：strict 默认翻转——type 错误默认
  致命，`--no-strict-types` 逃逸。全量验收：cargo 460/0、四门
  全绿、覆盖门禁 52/0/0、三语料 FP=0。**N-2 战役关闭**：J-3 归档
  为"不可修补"的目标经分阶段重设计达成。
- **Phase 3 kv effect 行**（最难，先独立设计注再动手）：
  kv-set/kv-get 以 A-SK 风格键序 effect 建模（符号键→类型），
  "`"swap 模式词的检查器栈序与运行时对齐。若 Phase 1+2 后
  两语料已零误报，本阶段可降级为纯保险层（可选）。
- **Phase 4 默认翻转**：strict 默认 + `--no-strict-types` 逃逸口 +
  spec/tutorial 更新 + Lexicon 全语料复跑定档。

工作量：Phase 0+1 ≈ 一个战役日；Phase 2 ≈ 一个战役日；
Phase 3 视 Phase 1/2 战果伸缩；Phase 0.5 日。每阶段独立提交，
预注册判据未动。

## 战役 O：自举链的「引号作为值」限制（一等引号）（**2026-09-21 已关闭**）

**状态**：已达成并收口，判据见 `tests/o_preregistered.md`（J1–J5 全部满足）。

**立项时的实测事实**：自举链能编"引号被高阶词消费"的程序
（`@map`/`@fold` 等，编译期已知 → 融合成嵌套函数），也能编**单个**
引号字面量作为值（`{ 42 } call .` 两链一致）。但下列形态会让
`whisperc compile-full` 直接报错，连 C 都产不出来：

| 程序 | VM | c2（修前） | c2（现状） |
|------|----|----|----|
| `{ 42 } call .` | 42 | ✅ 42 | ✅ 42 |
| `10 { 2 * } call .` | 20 | ✅ 20 | ✅ 20 |
| `: ap { call } ; 5 { 1 + } ap .` | 6 | ✅ 6 | ✅ 6 |
| `{ 1 + } { 2 * } drop drop 7 .` | 7 | ❌ `Type mismatch` | ✅ 7 |
| `[{ 1 + }] len .` | 1 | ❌ 同上 | ✅ 1 |
| `[{ 1 + }] 0 @nth 5 swap call .` | 6 | ❌ `'W___q0' undeclared` | ✅ 6 |

**重要**：这与 `call`（战役见 spec §16）无关——`[{ 1 + }] len .` 不含
任何新语法，在引入 `call` 之前就编不过（已用 stash 对照在 HEAD 上复现）。
`call` 只是让这个先前的洞变得可见：动态分发（把引号放进列表/map 再取出来
调用）正是引号作为一等值的主要用途。

**修掉的四处缺陷**（三处在 `fuse-hof` 的引号展开链上，一处在收集器）：

1. `cl-list` 缺一趟 `structify`，列表元素里的 `{` 被分类成普通词；
2. `next-consumer-op` 的 `dup 1 @nth` 未查长度，引号在末尾即越界崩溃；
3. `genq-name` 非首次调用时把计数器字符串留在栈上（每个引号一份垃圾）；
4. `try-def-colon` / `try-loop-inner` 对可能不是字符串的 token 调裸
   `streq`——**VM 报类型错、C 链静默返回 false 的真实双链分歧**，
   改用带守卫的 `str-eq?`。

列表字面量里的引号（上表末两行）由后继的 **战役 O′** 修复。

**附带项（未做，独立立项）**：把 `@n` 在 classify 阶段正名为能力令牌
（现在靠 runtime.c 实现 cgen 误发的 `W__<n>()` 兜着，见
`docs/independence-design.md` Step 2a），以及 `-t c`（Rust 转译器）
对 `CapExec` 发射空操作的问题（`transpiler.rs` 里写的
`// cap_exec: no-op in native mode`）。

## 战役 O′：引号在列表字面量内（**2026-09-21 已关闭**）

**状态**：已达成并收口。`[{ 1 + }] len .` → `1`、
`[{ 1 + }] 0 @nth 5 swap call .` → `6`、
`[{ 1 + } { 2 * } { 3 * }] 1 @nth 5 swap call .` → `10`、
`[[{ 1 + }]] 0 @nth 0 @nth 5 swap call .` → `6`，VM 与 `-t c2` 两链同值；
`opcode_coverage.sh` 登记 `qlist-*` 八例；固定点复验通过。

**根因（曾定位到行，供后来者对照）**：

1. `op-quote` 发射 `[53, inner_bc]`，但 **`53` 在整个 `compiler.all.ws`
   里只出现在 `op-quote` 自己那两行**——cgen 没有 53 分支，只会走到
   `#error whisper-cgen: unknown bare opcode`。即 `op-quote` 是**死代码**：
   引号能工作完全依赖 `fuse-hof` 提升成 `[57, name]` + `[9, name, body]`。
2. `op-list6` 用 `compile`（含 `fuse-hof`）编译列表元素，于是元素里的引号
   确实被提升出 `[16 "__q0"]` + `[9 "__q0" body]`；
3. 但 `compile-one` **有 tag 16 分支、没有 tag 9 分支**，嵌套定义被丢弃、
   引用留着 → `w_push(W_PTR((void*)W___q0))` 指向从未发射的函数。

**采用的修法（方向"甲"）**：把元素流里 tag 9 的定义抬到 `[6, rest]` 节点
之前；`fuse-hof` 尾部增加"head 是容器节点"的分支，递归处理字段流后 splice
回去。方向"乙"（`compile-one` 支持 tag 9 + cgen 补 opcode 53）未采用：53 若
要发射内联引号，只能借 gcc 嵌套函数，而嵌套函数的 trampoline 在**外层函数
返回后失效**——引号一旦被存进列表再返回出去就是悬空指针。抬升成顶层静态函数
（`[57, name]`）没有这个生命周期问题，所以保留 ref 形态是对的。

**后续（同源，已成战役 O₂）**：本战役只覆盖了列表一种容器，条件/循环/词定义/
高阶融合体四种当时仍失守；已由 `tests/o2_preregistered.md` 一般化修复
（`part` / `hs`，按 tag 区间 6..9 与 249..254 统一处理），O′ 的
`lift-list` 已被其吸收删除。

**状态（2026-09-21 收口战役 W5-c）**：`op-quote` / opcode 53 仍是死代码，
但已在 `compiler.all.ws` **就地标注**（说明 53 在本文件只出现于该词、cgen
没有 53 分支、引号可用性靠 `fuse-hof` 抬成顶层静态函数，与它无关），
避免下一个人以为它可达。**保留不删**：将来若要支持"引号完全内联"，入口在
这里，但那条路要先把上面那条嵌套函数 trampoline 的生命周期问题解决。

## 附：已关闭项（勿重复立项）
- map 遍历顺序非确定 → M-1 已修（VM BTreeMap + C 链排序插入，
  2026-09-10）。
- `]` then-臂列表贴近闭合括号两实现分歧 → spec §2 已标注已知限制
  （VM 拒 / 自举链收），规范写法已文档化。语法冻结期内不改。
- **WT_BOOL 第五标签 → 2026-09-10 完成（N-3）**：原 N-1 记录的两格
  "永久分歧"（`#t .`、`#t json-stringify`）已闭合——C 链值模型现为
  I64/F64/PTR/SIG/BOOL 五标签；比较/逻辑/谓词/try/json-parse 布尔
  全对齐 VM（spec §4 两行永久分歧行已撤）。bool? 精确判定。
- **kv effect 行 → 2026-09-10 以证据裁决：不做 A-SK 键序**。借位
  计数丢失的 9 条真阳性经调查全部是 **map 类型形状**（map-get 收
  i64，运行时 "expected map"）而非 kv 键序形状——对症保险层改为
  **Type::Map**（检查器新类型 + map-* 五词参数精确化）：TP 6→7、
  FP 保持 0。更深找回需要流敏感精度，收益/成本不成立，结案。
