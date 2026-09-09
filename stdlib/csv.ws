// std/csv — CSV 解析（最小集：无引号转义）
//
//   csv-parse  ( str -- [[col,...],...] )   行列表，每行 = 列字符串列表
//
// 已知限制（如实）：不处理引号包裹（"a,b" 仍按逗号切）与内嵌换行；
// 需要时再加状态机版本。扫描器的录制/回放数据（简单表）够用。

import "std/str"

: csv-parse
{
  "\r" "" strreplace
  "\n" strsplit
  { "," strsplit } @map
} ;

export csv-parse
