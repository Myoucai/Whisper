// HTTP client via capability tokens.
// Requires --allow-http flag to bind capabilities.
//   Cap 2: @http_get  (URL → response body)
//   Cap 3: @http_post (URL body → response body)
//   Cap 6: http-req   (L-06 会话族：method url headers-json body
//                       -- status headers-json body；cookie 罐 + 审计）
//   Cap 7: http-audit (-- 审计行)

: http-get  { @2 } ;
: http-post { @3 } ;
: http-req  { @6 } ;
: http-audit { @7 } ;

export http-get
export http-post
export http-req
export http-audit
