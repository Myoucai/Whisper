// std/html — 轻量 HTML 文本提取（字节扫描，无 DOM）
//
//   html-text  ( html -- text )   剥 <...> 标签 + 解码五个命名实体
//   html-attr  ( html tag attr -- value|"" )   首个 <tag ... attr="..." 的值
//
// 服务扫描器证据提取：DVWA 类 HTML 响应取可见文本/属性，不建树。

import "std/str"

// ( -- ) 内部：H-S 源、H-P 累积、H-T 标签态（"0"/"1"）
: html-skip-loop
{
  0
  {
    "H-S" kv-get over strnth
    dup 60 =                         // '<'
    ??  drop "H-T" "1" kv-set
    |   dup 62 =                     // '>'
        ?? drop "H-T" "0" kv-set
        |   "H-T" kv-get "0" =
            ?? ctos "H-P" kv-get swap strcat "H-P" ` kv-set
            | drop
            ]
        ]
    ]
    1 +
  }
  { dup "H-S" kv-get strlen < }
  #
  drop
} ;

: ent-dec
{
  "&amp;" "&" strreplace
  "&lt;" "<" strreplace
  "&gt;" ">" strreplace
  "&quot;" 34 ctos strreplace
  "&#39;" "'" strreplace
  "&nbsp;" " " strreplace
} ;

: html-text
{
  "H-S" ` kv-set
  "H-P" "" kv-set
  "H-T" "0" kv-set
  html-skip-loop
  "H-P" kv-get ent-dec
} ;

// ( -- str ) H-S 自 H-B 偏移起的余部
: hrest
{
  "H-S" kv-get "H-B" kv-get strtoi64
  "H-S" kv-get strlen "H-B" kv-get strtoi64 -
  strslice
} ;

// ( html tag attr -- value ) 首个 <tag ... attr="..." 的值；缺 → ""
: html-attr
{
  "H-A" ` kv-set
  "H-K" ` kv-set
  "H-S" ` kv-set
  ""
  "H-S" kv-get "<" "H-K" kv-get strcat strfind
  dup 0 >=
  ??
    "H-K" kv-get strlen + 1 + i64tostr "H-B" ` kv-set
    hrest "H-A" kv-get "=\"" strcat strfind
    dup 0 >=
    ??
      "H-A" kv-get strlen + 2 + "H-B" kv-get strtoi64 + i64tostr "H-B" ` kv-set
      hrest 34 ctos strfind
      dup 0 >=
      ??
        "H-S" kv-get "H-B" kv-get strtoi64 rot strslice
        swap drop
      |
        drop
      ]
    |
      drop
    ]
  |
    drop
  ]
} ;

export html-text
export html-attr
export ent-dec
export hrest
