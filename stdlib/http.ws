// HTTP client via capability tokens.
// Requires --allow-http flag to bind capabilities.
//   Cap 2: @http_get  (URL → response body)
//   Cap 3: @http_post (URL body → response body)
//   Cap 6: http-req   (L-06 会话族：method url headers-json body
//                       -- status headers-json body；cookie 罐 + 审计）
//   Cap 7: http-audit (-- 审计行)
//   Cap 8: http-req-raw（L-12：同 @6，但 body 为 hex 串 → 原始字节发送，
//                       multipart/二进制载荷通道；响应仍文本）
//   Cap 9: http-req-parallel（L-11：count method url headers body --
//          results-json；同一请求并发 count 份，按请求序聚合；竞态/
//          latch 探测通道，VM 侧单次调用不破坏确定性）

: http-get  { @2 } ;
: http-post { @3 } ;
: http-req  { @6 } ;
: http-audit { "" @7 } ;
: http-req-raw { @8 } ;
: http-req-parallel { @9 } ;

export http-get
export http-post
export http-req
export http-audit
export http-req-raw
export http-req-parallel
