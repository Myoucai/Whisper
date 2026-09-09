// std/jwt — JWT 解码（不验签；扫描器的载荷侧消费）
//
// 依赖：std/encoding（b64url 无填充解码）
//
//   jwt-header   ( token -- header-json-str )
//   jwt-payload  ( token -- payload-json-str )
//   jwt-claim    ( token claim -- value-str )   载荷取键；缺 → ""
//
// 形态：header.payload.signature，三段均 b64url（无填充）。
// 用法（Lexicon 打 JWT 靶场：看 iat/exp/role）：
//   token "iat" jwt-claim .    // "1788407200"

import "std/encoding"
import "std/str"

// ( token idx -- part ) 取第 idx 段（0=header 1=payload 2=sig）
: jwt-part
{
  swap "." strsplit swap @nth
} ;

: jwt-header
{
  0 jwt-part b64url-dec
} ;

: jwt-payload
{
  1 jwt-part b64url-dec
} ;

: jwt-claim
{
  ` jwt-payload json-parse swap map-get
  dup i64? ?? i64tostr | ]
} ;

export jwt-part
export jwt-header
export jwt-payload
export jwt-claim
