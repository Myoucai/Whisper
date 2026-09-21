# 语言级后续战役（backlog）

2026-09-10 盘点确立。此处登记**已实锤、暂不实施**的语言级问题——
每项含事实、选项与预注册判据要求；开工时先落判据再动手。

## 战役 N-1：bool / atan2 语义统一（两后端行为分叉）

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

**采用的修法（方向"甲"）**：新增 `lift-list` / `lift-list-iter`，把元素流里
tag 9 的定义抬到 `[6, rest]` 节点之前；`fuse-hof` 尾部增加 `head 是 [6, …]`
的分支，递归处理元素流后 splice 回去。列表可任意嵌套，每层抬出一层。
方向"乙"（`compile-one` 支持 tag 9 + cgen 补 opcode 53）未采用：53 若要发射
内联引号，只能借 gcc 嵌套函数，而嵌套函数的 trampoline 在**外层函数返回后
失效**——引号一旦被存进列表再返回出去就是悬空指针。抬升成顶层静态函数
（`[57, name]`）没有这个生命周期问题，所以保留 ref 形态是对的。

**遗留（不在本战役范围，如实登记）**：

- 条件/循环分支里的裸引号（`1 ?? { 2 } | { 3 } ]`）：分支由 `op-cond` /
  `op-loop` 各自 `compile`，同样没有把 tag 9 抬到顶层的通路。与 O′ 同源，
  未修，也未加门禁用例。
- `op-quote` / opcode 53 仍是死代码。现在无所谓（ref 形态够用），但若将来
  要支持"引号完全内联"，需要先解决上面那条生命周期问题。

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
