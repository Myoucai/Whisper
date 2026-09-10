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

## 战役 N-2：typecheck strict 默认（J-3 复活）

**事实**：详见 docs/typecheck-strict-design.md 战报——expect_ref
放松、?? 臂逐槽合并、两遍法签名推断三者可把 Lexicon 误报压 0，
但 std/encoding 向量链反向失败；两语料库对 checker 压力方向相反，
修补不可同时满足。

**前置**：checker 重设计——多态变体推断 + 引号栈合同 + kv 态词
effect 行。非修补可达，需独立设计文档 + 分阶段实施
（先 effect 行，后变体推断）。

## 附：已关闭项（勿重复立项）

- map 遍历顺序非确定 → M-1 已修（VM BTreeMap + C 链排序插入，
  2026-09-10）。
- `]` then-臂列表贴近闭合括号两实现分歧 → spec §2 已标注已知限制
  （VM 拒 / 自举链收），规范写法已文档化。语法冻结期内不改。
