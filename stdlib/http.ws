// HTTP client via capability tokens.
// Requires --allow-http flag to bind capabilities.
//   Cap 2: @http_get  (URL → response body)
//   Cap 3: @http_post (URL body → response body)

: http-get  { @2 } ;
: http-post { @3 } ;

export http-get
export http-post
