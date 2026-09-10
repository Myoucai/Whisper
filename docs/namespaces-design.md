# J-4 模块命名空间——设计定稿与验收判据（跑前写死）

日期：2026-09-04。触冻结语法（spec v1.0），按冻结声明流程执行：
双实现同步（Rust VM + 自举 C 链）、syntax-gate/stdlib-gate 补例、
spec 修订。

## 语义（最小侵入设计）

1. **词法**：`.` 进入词名字符集，仅当**前后均为词字符**（字母数字与
   现有词字符集）。独立 `.`（输出）与 `..`（OutputAll）不变。
   由此 `str.find` 是单个 Word token；`3.` / `.5` 数字词法不受影响
   （read_number 先行）；`a .` 空格分隔仍是输出。

2. **命名空间注册**：`import "path/to/mod"` 自动以**路径末段**注册
   命名空间 `mod`（无需新关键字 `as`）。模块导出词同时以两种形态
   可用：
   - 非限定 `find`（向后兼容，现行为）；
   - 限定 `str.find`（消歧义权威形态）。

3. **冲突解决**：两模块导出同名词时，现有"警告 + 后者遮蔽"保持；
   新增：**限定形态永不被遮蔽**——`str.find` 直接解析到 `std/str`
   的导出，与冲突无关。

4. **解析优先级**：限定词 `mod.word` 先查命名空间表；命中则用之，
   未命中则**回退为普通词**（错误信息统一在运行时 undefined word）。
   非限定词行为完全不变。

## 验收判据（跑前写死）

- J4-1 词法：`str.find` 单 token；`a .`、`3 .`、`..`、`3.14` 不变
  （Rust 与自举链双实现一致的语法门禁用例）；
- J4-2 限定调用：两模块同名导出（如 std/str 的 `find` 与自定义），
  非限定得到警告+遮蔽，限定 `str.find` / `my.find` 各得其所；
- J4-3 向后兼容：全量 workspace 测试 + Lexicon 套件 + examples
  输出逐字节不变（限定词不出现时零扰动）；
- J4-4 自举链：`whisperc` 侧同一词法规则；固定点 gen2==gen3 保持；
- J4-5 spec 修订 + stdlib.md / tutorial 文档。

## 明确不做

- `as` 别名语法（新关键字，冻结期避免）；
- 嵌套命名空间 `a.b.c`（点只允许一层：第一个点分隔命名空间）；
- 限定形态的类型检查特殊化（typecheck 把 `mod.word` 当普通词——
  J-3 已证 checker 需重设计，不在此处叠加）。


## 战报（2026-09-04，跑后追加，判据未动）

### 已落地（保留）

- **J4-1 词法**：Rust 侧限定量词法完成——`mmod.mw` 单 Word token；
  独立 `.`/`..`/`3.14` 不受影响；5 项 lexer 单测锁死
  （qualified_word_tests）。
- **J4-2 限定调用 + 冲突语义**：resolver 在 import 时以路径末段注册
  命名空间，模块每个 Def 克隆为 `末段.词名` 的附加 Def——限定形态
  即普通词，永不被非限定冲突遮蔽；实测验收：
  - `std/str` 与自定义模块同导出 `find`：非限定得警告+遮蔽，
    `str.find`=1（真查找）、`mymod.find`="MINE" 各得其所；
  - 双向限定（mymod.find / str.find）同时正确。
- **J4-3 向后兼容**：workspace 20 套全绿；Lexicon 套件警告数不变
  （t_ind6s7:4 为 J-3 已知基线，其余 0）；examples 0。
- **J4-5 spec/文档**：本文件 + spec.md §2 词法表更新。

### 未达成（J4-4 自举链同步）——如实归档，compiler.all.ws 已回退

自举链侧实现了同一词法规则（dot-joins?/chunk-cont?/is-wordch? +
read-chunk-acc 门控 + tokenize-loop 分支），在 **Rust VM 上执行**
语义完全正确（tokenize/structify/classify 全链验证，`mmod.mw` 曾
编译出 `W_mmod_mw();`）；但**C 编译版**（bootstrap 编译器二进制）
在含 `.` 的输入上崩溃（"expected ptr"）。定位过程中实锤的三个
自举链结构性缺口：

1. **seed 的 `over` C-gen 缺口**：classify 有 [15] 特例，但
   emit-one 无 case 15 → `// unknown list instruction` 静默垃圾；
2. **kv 只存字符串**（Rust VM 铁律在 C 链同样成立）：is-wordch?
   直存 i64 立即崩，需 ctos 中转；
3. **export 过滤**：直接 run 含 export 的文件会过滤非导出词——
   调试 driver/tokenize-loop 全部踩坑（c2 管线不经 resolver 故
   生产路径无此问题）。

修复 1+2 后 VM 侧依旧全对、C 侧依旧崩——剩余分歧在 seed C-gen
对我新增词的某个（些）opcode 发射上，未能定位。**结论与 J-3 同型**：
自举链的 C-gen 对新词法代码有未定位的发射分歧，需要 emit-one
opcode 覆盖审计（而非继续打地鼠）。Rust 侧命名空间功能完整可用，
自举链保持 HEAD（固定点 gen2==gen3 保持）。

### 使用示例（Rust VM，已可用）

```whisper
import "std/str"
import "mymod"        // 也导出 find
"abc" "b" str.find .  // 1 —— 限定永不遮蔽
find .                // "MINE" —— 非限定=现状（警告+后导出遮蔽）
```

## 战报（2026-09-09，K 阶段——J4-4 收官，判据未动）

上节"未达成"的两个悬念（未定位的发射分歧）在本阶段全部定位并修复，
compiler.all.ws 重新携带限定量词法，固定点恢复。

### 根因一：seed 对 >2 层 ?? 嵌套的函数产出错字节码

J4-4 战败的"剩余分歧"实锤：is-wordch? 初版用了 5 层嵌套 ??。
Rust 编译器容忍任意嵌套 → VM 侧语义全对；seed 编译器对深嵌套
函数产出损坏字节码 → C 侧错（独立 `.` 输入下 chunk-cont? 往栈上
泄漏 I64 255，w_as_ptr 崩）。修复：is-wordch? 重写为 &/| 扁平
布尔链（零 ??）——这是自举子集的硬纪律，此后新增词一律扁平化。

### 根因二：O(源²) 词法分配撞 2GB 字符串 arena（链本就贴崖）

扁平化后六项探针全过，但全源自编译（gen2）爆
`string heap exhausted (pos≈2^31)`。逐级二分定位：

- 冻结种子在 60KB 合成输入同样爆——**非新词引入**；
- 自举词法逐字符 `strslice` 全剩余串 → 累计分配 O(源²)；
  50KB 源 ≈ 2GB，链数月来一直贴着旧限（4GB 总/2GB 每半）运行，
  新词的每字符 kv/ctos 常数把 w1 顶过崖；
- 修复：runtime.c HEAP_SIZE 4GB→16GB（VirtualAlloc/mmap 懒提交，
  实际提交内存不变 ≈2.1GB）。生成 C 以 `#include "runtime.c"`
  按引用包含 → gen2==gen3 文本比对不受影响；seed 文件保持冻结
  （它经 `#include "../runtime.c"` 同享该修复）。

### 落地清单

- **词法四词**：is-wordch?（扁平）/dot-joins?/chunk-cont?/
  chunk-or-delim + read-chunk-acc 门控 + tokenize-loop 分支；
- **K-1 c2 注入**：expand_imports 内联后追加 `: 末段.词名` 克隆
  （单行定义体不重拷，真实换行转义修复）；
- **K-3 硬失败**：未知 token/裸 opcode 发 `#error`；
- **K-4 数值提升**：`<=`/`>=` 改 w_num_f64，浮点对拍全绿；
- **K-2 覆盖门禁** tools/opcode_coverage.sh：36 用例双跑对拍，
  现况 **36 pass / 0 fail / 3 known-gap**（json/conf：C 引用不
  存在的运行时助手；try：换行存根——均为 c2 目标债务，白名单
  大声记账，不遮不藏）。

### 验收

- `: mmod.mw { 42 } ; mmod.mw .` → 42（VM=原生）；
- `3.14 .` → 3.14（双侧）；`5 .` → 5；
- 固定点 gen2==gen3（scripts/verify-fixedpoint.sh）；
- cargo test --workspace 全绿；Lexicon 套件不受影响（语言侧无
  破坏性变更）。

## 战报（2026-09-10，L 阶段——c2 债务清偿）

覆盖门禁白名单清空（46/0/0），CI 四门（syntax/stdlib/net/app）全绿。

### 三笔登记债务的清偿

- **try**：runtime.c w_fail + setjmp/longjmp；值级错误回卷压
  [#f,消息]，成功 [#t,值]（含空体默认 0、栈回卷深度）；嵌套 try、
  try 捕 json 错实测。资源级错误保持致命。
- **json**：递归下降解析 + 紧凑序列化；对象键排序对齐 serde_json
  BTreeMap；f64 轮转对齐 ryu（4.0/1e30）。map 改标记前缀对表
  （与形似列表可区分——L-08 启发式腐坏不会复发）。
- **置信度**：`#t:0.75` 词法（':' 越过 is-ws 停止判定）+ classify
  [14]/[12] + WT_SIG 透明包装；conf-of peek、unwrap 剥壳。

### 挖出的三笔 K 前旧债（如实记录）

1. **裸 HOF + PushRef 发射从未存在**：`: map { @map } ;` 式包装词
   与引号作参数（`{ 1 + } map`）在 C 链从未工作——stdlib-gate 的
   list-* 全灭根因。补六个裸码 case + fuse-hof 引号提升为合成词
   __qN（[16] 压引用 + [9] 合成 def，新指令 [57]）。
2. **K-1 克隆注释 bug**：行尾注释让单行 def 判定失守，`: filter`
   被原样双定义。判定改按剥注释代码。
3. **杂项**：atan2 弹栈序反+改提升；ffi-sym 应回 I64 地址；
   kv-keys 漏 W_PTR（server.ws 编不过）。

**教训**：K-6 验收漏跑 stdlib/app 门禁，三笔旧债带病入库
（fa947de 的 CI selfhost 应为红）。本地 gates 复验已入验收清单。
另发现：Windows 下 cargo test 带 gcc 时 native_target 测试的
CRLF 断言写反（一直靠 Linux CI 掩盖）——已修。

### 自举方言战报（本轮五个栈序 bug，全部被"VM 先跑+原生对拍"抓住）

kv 存值序（值在键下）、$N pick 压栈后索引漂移、prepend 参数序
（elem 底 list 顶）、over 单元素栈越界（用 `_`）、?? 包装漏闭括号
（括号恰好重排进内层臂——语义静默错乱最难查，靠 bisect 定位）。
