// std/conf — 置信度与不确定性（定稿表面）
//
// Whisper 的值可携带置信度：Signal(v, c)，c ∈ [0,1]。本模块定义唯一
// 官方的四个表面原语（均为语言内建，此处为文档与再导出）：
//
//   1. 标注        <value>:<conf>     ConfLabel 语法后缀
//                   例：#t:0.7   "x":0.9   42:0.5
//                   标注即包 Signal；plain 值视作 c=1.0
//
//   2. 窥视置信度  conf-of   ( x -- x f64 )   不消耗 x；plain 值得 1.0
//
//   3. 剥壳        unwrap    ( sig -- val )   去掉 Signal 包装，取裸值
//
//   4. 概率分支    p { alt1 } { alt2 } ?|     L-03 语义（显式概率）
//                   p 的来源按序判定：Signal → 解包取 c 并消耗 p；
//                   F64 ∈ [0,1] → 消耗；其他 → 运行时报错。
//                   P(alt1) = p。定种子复现：n seed!。
//
// 组合词（本模块新增，皆由上述原语构成）：
//
//   conf-mul ( a-conf b-conf -- a*b )   置信度合取（证据链衰减）
//   labeled? ( x -- b )                 x 是否为 Signal（c < 1.0）
//
// 设计约束：置信度永不进入 kv（浮点禁 kv 的既有铁律）；需要存储时
// 用 i64 千分比（c*1000 四舍五入）。

: conf-mul { * } ;
: labeled? { conf-of 1.0 < } ;

export conf-mul
export labeled?
