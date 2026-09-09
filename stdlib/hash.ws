// std/hash — MD5（纯 Whisper，RFC 1321；32 位算术以 0xFFFFFFFF 截断）
//
//   md5  ( msg -- digest-hex )   32 个小写十六进制字符
//
// 字节管线：消息 → strchars 字节列表（任意 UTF-8 安全）→ 填充（列表
// 追加）→ 64 字节分块 → 每块 16 个 32 位小端字入 kv（M-W0..M-W15）→
// 64 轮（寄存器/常数全 kv 字符串化 i64）→ 状态字小端十六进制拼接。

import "std/str"

: m32 { 4294967295 bitand } ;

// ( x s -- xr ) 32 位循环左移
: md5-rot
{
  over over 32 swap - >> rot rot << bitor m32
} ;

// ( 0-15 -- hexch ) 小写
: hex-l { dup 10 < ?? 48 + | 87 + ] ctos } ;

// ( b -- 2hex ) 小写
: hex-b { dup 4 >> hex-l ` 15 bitand hex-l strcat } ;

: md5-k
{
  "3614090360,3905402710,606105819,3250441966,4118548399,1200080426,2821735955,4249261313,1770035416,2336552879,4294925233,2304563134,1804603682,4254626195,2792965006,1236535329,4129170786,3225465664,643717713,3921069994,3593408605,38016083,3634488961,3889429448,568446438,3275163606,4107603335,1163531501,2850285829,4243563512,1735328473,2368359562,4294588738,2272392833,1839030562,4259657740,2763975236,1272893353,4139469664,3200236656,681279174,3936430074,3572445317,76029189,3654602809,3873151461,530742520,3299628645,4096336452,1126891415,2878612391,4237533241,1700485571,2399980690,4293915773,2240044497,1873313359,4264355552,2734768916,1309151649,4149444226,3174756917,718787259,3951481745"
} ;

: md5-s
{
  "7,12,17,22,7,12,17,22,7,12,17,22,7,12,17,22,5,9,14,20,5,9,14,20,5,9,14,20,5,9,14,20,4,11,16,23,4,11,16,23,4,11,16,23,4,11,16,23,6,10,15,21,6,10,15,21,6,10,15,21,6,10,15,21"
} ;

// ( r -- f ) 轮函数（四分之一分段；?? 嵌套仅 else 臂）
: md5-f
{
  dup 16 <
  ??
    drop
    "M-B" kv-get strtoi64 "M-C" kv-get strtoi64 bitand
    "M-B" kv-get strtoi64 4294967295 bitxor "M-D" kv-get strtoi64 bitand bitor m32
  |
    dup 32 <
    ??
      drop
      "M-D" kv-get strtoi64 "M-B" kv-get strtoi64 bitand
      "M-D" kv-get strtoi64 4294967295 bitxor "M-C" kv-get strtoi64 bitand bitor m32
    |
      dup 48 <
      ??
        drop
        "M-B" kv-get strtoi64 "M-C" kv-get strtoi64 bitxor
        "M-D" kv-get strtoi64 bitxor m32
      |
        drop
        "M-B" kv-get strtoi64
        "M-D" kv-get strtoi64 4294967295 bitxor bitor
        "M-C" kv-get strtoi64 swap bitxor m32
      ]
    ]
  ]
} ;

// ( r -- g ) 本轮字下标
: md5-g
{
  dup 16 <
  ??
  | dup 32 <
    ?? 5 * 1 + 16 mod
    | dup 48 <
      ?? 3 * 5 + 16 mod
      | 7 * 16 mod
      ]
    ]
  ]
} ;

// ( r -- ) 一轮；r 在 kv M-R，块字在 M-W0..M-W15
: md5-round
{
  "M-R" kv-get strtoi64 dup dup dup
  md5-f i64tostr "M-F" ` kv-set
  md5-g i64tostr "M-G" ` kv-set
  md5-k "," strsplit swap @nth strtoi64 i64tostr "M-KC" ` kv-set
  md5-s "," strsplit swap @nth strtoi64 i64tostr "M-SH" ` kv-set
  // T = A + f + K[r] + W[g]  (m32)
  "M-A" kv-get strtoi64 "M-F" kv-get strtoi64 + m32
  "M-KC" kv-get strtoi64 + m32
  "M-W" "M-G" kv-get strcat kv-get strtoi64 + m32
  i64tostr "M-T" ` kv-set
  // A:=D  D:=C  C:=B  B:=B+rot(T, s[r])
  "M-A" "M-D" kv-get kv-set
  "M-D" "M-C" kv-get kv-set
  "M-C" "M-B" kv-get kv-set
  "M-B" kv-get strtoi64
  "M-T" kv-get strtoi64
  "M-SH" kv-get strtoi64
  md5-rot + m32 i64tostr "M-B" ` kv-set
} ;

// ( msg -- bytes-list ) 填充：0x80、零至 ≡56 (mod 64)、8 字节小端位长
: md5-pad
{
  strchars
  dup len 8 * i64tostr "M-BL" ` kv-set
  128 append
  { 0 append }
  { dup len 64 mod 56 = #f = }
  #
  "M-K8" "0" kv-set
  {
    "M-BL" kv-get strtoi64 "M-K8" kv-get strtoi64 8 * >> 255 bitand append
    "M-K8" kv-get strtoi64 1 + i64tostr "M-K8" ` kv-set
  }
  { "M-K8" kv-get strtoi64 8 < }
  #
  "M-K8" kv-del
  "M-BL" kv-del
} ;

// ( B g -- B ) 块 B 第 g 个 32 位小端字 → kv M-W{g}；B 保留
: bw
{
  dup i64tostr "M-GI" ` kv-set
  4 * i64tostr "M-O" ` kv-set
  dup "M-O" kv-get strtoi64 @nth
  $1 "M-O" kv-get strtoi64 1 + @nth 256 * +
  $1 "M-O" kv-get strtoi64 2 + @nth 65536 * +
  $1 "M-O" kv-get strtoi64 3 + @nth 16777216 * +
  i64tostr
  "M-W" "M-GI" kv-get strcat `
  kv-set
  "M-O" kv-del
  "M-GI" kv-del
} ;

// ( block -- ) 处理一个 64 字节块
: md5-block
{
  0 bw 1 bw 2 bw 3 bw 4 bw 5 bw 6 bw 7 bw
  8 bw 9 bw 10 bw 11 bw 12 bw 13 bw 14 bw 15 bw
  drop
  "M-A0" "M-A" kv-get kv-set
  "M-B0" "M-B" kv-get kv-set
  "M-C0" "M-C" kv-get kv-set
  "M-D0" "M-D" kv-get kv-set
  "M-NR" kv-get strlen 0 =
  ?? "64" | "M-NR" kv-get ]
  strtoi64 range-to
  { 1 - i64tostr "M-R" ` kv-set md5-round } @each
  "M-A" kv-get strtoi64 "M-A0" kv-get strtoi64 + m32 i64tostr "M-A" ` kv-set
  "M-B" kv-get strtoi64 "M-B0" kv-get strtoi64 + m32 i64tostr "M-B" ` kv-set
  "M-C" kv-get strtoi64 "M-C0" kv-get strtoi64 + m32 i64tostr "M-C" ` kv-set
  "M-D" kv-get strtoi64 "M-D0" kv-get strtoi64 + m32 i64tostr "M-D" ` kv-set
} ;

// ( w -- hex8 ) 状态字 → 小端 4 字节 8 个小写十六进制字符
: md5-word-hex
{
  i64tostr "M-X" ` kv-set
  "M-H" "" kv-set
  4 range-to
  { 1 - 8 * "M-X" kv-get strtoi64 swap >> 255 bitand hex-b "M-H" kv-get swap strcat "M-H" ` kv-set }
  @each
  "M-H" kv-get
} ;

: md5
{
  md5-pad
  "M-A" "1732584193" kv-set
  "M-B" "4023233417" kv-set
  "M-C" "2562383102" kv-set
  "M-D" "271733878" kv-set
  {
    dup 64 take md5-block
    64 drop-n
  }
  { dup len 0 > }
  #
  drop
  "M-A" kv-get strtoi64 md5-word-hex
  "M-B" kv-get strtoi64 md5-word-hex strcat
  "M-C" kv-get strtoi64 md5-word-hex strcat
  "M-D" kv-get strtoi64 md5-word-hex strcat
} ;

export md5
export m32
export md5-rot
export hex-l
export hex-b
export md5-k
export md5-s
export md5-f
export md5-g
export md5-round
export md5-pad
export bw
export md5-block
export md5-word-hex
