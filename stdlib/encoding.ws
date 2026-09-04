// std/encoding — base64 / base64url / 百分号编码（纯 Whisper，零依赖）
//
// 词表（字节语义 §3.5：strnth/strslice/charsstr 均按字节）：
//   b64-enc      RFC 4648 标准 base64（含 = 填充）
//   b64url-enc   URL 安全字母表（- _，带填充）
//   pct-enc      非保留字符 → %XX（大写十六进制）
//   pct-dec
//   b64-dec      见文件尾（b64url-dec 同理）
//
// 铁律：kv 只存字符串（E-S 源 / E-A 字母表 / E-N 组数）。

: b64-alpha     { "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/" } ;
: b64url-alpha  { "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_" } ;

// ( idx -- ch ) 0..63 → 字母表字符；越界 → "="
: enc-ch
{
  dup 0 >= over 63 <= &
  ?? "E-A" kv-get ` strnth ctos
  | drop "=" ]
} ;

// 四个六位组 → 字符
// 栈迹： dup18>>63& → swap dup12>>63& → swap dup6>>63&
//   → swap 63& →
: sextets
{
  dup 18 >> 63 bitand enc-ch
  swap dup 12 >> 63 bitand enc-ch
  swap dup 6 >> 63 bitand enc-ch
  swap 63 bitand enc-ch
  strcat strcat strcat
} ;
// 映射段栈迹： enc-ch→ rot→ enc-ch
//   →?? 检查：rot 顶三→ 得
//   ——逐字核对太脆，实测为准（见 e64test）。

// ( off i -- b ) 读 E-S 的第 off+i 字节
: sat
{
  + "E-S" kv-get ` strnth
} ;

// 一组：状态 ( off out -- off+3 out+C )
: enc-step
{
  $1 0 sat
  $2 1 sat
  $3 2 sat
  rot 65536 * rot 256 * + +
  sextets
  strcat
  swap 3 + swap
} ;

// ( out -- out' ) 尾组：rem = len mod 3；v 经 dup/swap 携带取六位组
: enc-tail
{
  "E-S" kv-get strlen 3 mod
  dup 1 =
  ??
    drop
    "E-S" kv-get dup strlen 1 - strnth 65536 *
    dup 18 >> 63 bitand enc-ch
    swap dup 12 >> 63 bitand enc-ch
    swap drop
    strcat strcat "==" strcat
  |
    dup 2 =
    ??
      drop
      "E-S" kv-get dup strlen 2 - strnth 65536 *
      "E-S" kv-get dup strlen 1 - strnth 256 * +
      dup 18 >> 63 bitand enc-ch
      swap dup 12 >> 63 bitand enc-ch
      swap dup 6 >> 63 bitand enc-ch
      swap drop
      strcat strcat strcat "=" strcat
    |
      drop
    ]
  ]
} ;

// ( s alpha -- b64 )
: b64-enc-with
{
  "E-A" ` kv-set
  "E-S" ` kv-set
  "E-S" kv-get strlen i64tostr "E-N" ` kv-set
  0
  ""
  { enc-step }
  { $1 "E-N" kv-get strtoi64 swap - 3 >= }
  #
  swap drop
  enc-tail
} ;

: b64-enc    { b64-alpha b64-enc-with } ;
: b64url-enc { b64url-alpha b64-enc-with } ;

export b64-enc
export b64url-enc
export pct-enc
export pct-dec
export b64-alpha
export b64-enc-with
export enc-step
export sextets
export enc-ch
export enc-tail
export sat
export sat

// ── 解码 ─────────────────────────────────────────────────────────
// ( ch-str -- idx ) E-A 反查；非字母表（含 =）→ 0
: dec-ch
{
  "E-A" kv-get ` strfind
  dup 0 < ?? drop 0 | ]
} ;

// ( off i -- ch-str ) E-S 第 off+i 字节的单字符串
: sget
{
  + "E-S" kv-get ` 1 strslice
} ;

// ( gi -- ) 处理第 gi 组（1 基）：4 字符 → 24 位 → 3 字节追加 E-B
: dec-group
{
  1 - 4 * i64tostr "E-O" ` kv-set
  0 "E-O" kv-get strtoi64 sget dec-ch 18 <<
  1 "E-O" kv-get strtoi64 sget dec-ch 12 << +
  2 "E-O" kv-get strtoi64 sget dec-ch 6 << +
  3 "E-O" kv-get strtoi64 sget dec-ch +
  dup 16 >> 255 bitand ctos "E-B" kv-get swap strcat "E-B" ` kv-set
  dup 8 >> 255 bitand ctos "E-B" kv-get swap strcat "E-B" ` kv-set
  255 bitand ctos "E-B" kv-get swap strcat "E-B" ` kv-set
} ;

// ( b64 alpha -- str ) 解码：全组产出后按尾部 = 裁剪
: b64-dec-with
{
  "E-A" ` kv-set
  "E-S" ` kv-set
  "E-B" "" kv-set
  "E-S" kv-get strlen 4 / range-to
  { dec-group } @each
  // pad 数 = 尾部 '=' 个数（至多 2；空串安全：strslice 钳位后失配）
  "E-S" kv-get dup strlen 1 - 1 strslice "=" streq
  "E-S" kv-get dup strlen 2 - 1 strslice "=" streq &
  ?? "E-B" kv-get strlen 2 -
  | "E-S" kv-get dup strlen 1 - 1 strslice "=" streq
    ?? "E-B" kv-get strlen 1 -
    | "E-B" kv-get strlen
    ]
  ]
  "E-B" kv-get 0 rot strslice
} ;

: b64-dec    { b64-alpha b64-dec-with } ;
: b64url-dec { b64url-alpha b64-dec-with } ;

// ── 百分号编码 ───────────────────────────────────────────────────
// ( 0-15 -- hexch )
: hex-ch
{
  dup 10 < ?? 48 + | 55 + ] ctos
} ;

: keep-set
{
  "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz-._~"
} ;

// ( b -- s ) 保留字符原样，否则 %HH
: pct-byte
{
  dup ctos "E-K" kv-get strfind 0 >=
  ?? ctos
  |
    dup 4 >> hex-ch
    swap dup 15 bitand hex-ch
    swap drop
    strcat
    "%" ` strcat
  ]
} ;

: pct-enc
{
  "E-K" keep-set kv-set
  "E-P" "" kv-set
  strchars
  { pct-byte "E-P" kv-get swap strcat "E-P" ` kv-set } @each
  "E-P" kv-get
} ;

// ( chbyte -- v ) 十六进制字符值，非法 → 0
: hex-val
{
  dup 48 >= over 57 <= & ?? 48 -
  | dup 65 >= over 70 <= & ?? 55 -
    | dup 97 >= over 102 <= & ?? 87 -
      | drop 0
      ]
    ]
  ]
} ;

: pct-dec
{
  "E-S" ` kv-set
  "E-P" "" kv-set
  0
  {
    "E-S" kv-get over strnth
    dup 37 =
    ??
      drop
      "E-S" kv-get over 1 + strnth hex-val 16 *
      "E-S" kv-get $2 2 + strnth hex-val +
      ctos "E-P" kv-get swap strcat "E-P" ` kv-set
      3 +
    |
      ctos "E-P" kv-get swap strcat "E-P" ` kv-set
      1 +
    ]
  }
  { dup "E-S" kv-get strlen < }
  #
  drop
  "E-P" kv-get
} ;

export b64-dec
export b64url-dec
export dec-ch
export sget
export dec-group
export b64-dec-with
export hex-ch
export pct-byte
export pct-enc
export hex-val
export pct-dec
export keep-set
export b64url-alpha
