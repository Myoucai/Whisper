// Whisper network library — v1.0.1+ native socket opcodes.

// body ctype → full HTTP/1.0 text response
: http-respond {
    // body → full HTTP/1.0 text/plain response
    _ strlen i64tostr
    "HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\nContent-Length: " ` strcat
    "\r\n\r\n" strcat
    ` strcat
} ;

// request → method (first token of the request line)
: req-method {
    "\r\n" strsplit 0 @nth " " strsplit 0 @nth
} ;

// request → path (second token of the request line)
: req-path {
    "\r\n" strsplit 0 @nth " " strsplit 1 @nth
} ;

// request → body (after the blank line; "" when absent)
: req-body {
    dup "\r\n\r\n" strfind 0 >= ??
        "\r\n\r\n" strsplit 1 @nth
    | drop ""
    ]
} ;

// path → path without query string
: path-only {
    "?" strsplit 0 @nth
} ;

// path → query string ("" when absent)
: query-of {
    dup "?" strfind 0 > ?? dup "?" strfind 1 + 9999 strslice | drop "" ]
} ;

// formstr key → value | ""   (a=b&c=d lookup, + decoded to space)
: form-get {
    "=" strcat
    over over strfind
    dup 0 >= ??
        $1 strlen +
        ` drop
        9999 strslice
        "&" strsplit 0 @nth
        "+" " " strreplace
    | drop drop drop ""
    ]
} ;

export http-respond
export req-method
export req-path
export req-body
export path-only
export query-of
export form-get

// char → hex value | -1
: hex-val {
    dup 48 >= over 57 <= & ?? 48 - return ]
    dup 65 >= over 70 <= & ?? 55 - return ]
    dup 97 >= over 102 <= & ?? 87 - return ]
    drop -1
} ;

// url-decode loop over char codes: list acc → acc'
: udl {
    over len 0 = ?? ` drop return ]
    over 0 @nth dup 37 = ??
        drop
        over len 3 < ??
            over 0 @nth ctos strcat
            swap 1 drop-n swap udl
        |
            over 2 @nth hex-val
            $2 1 @nth hex-val
            16 * +
            ctos strcat
            swap 3 drop-n swap udl
        ]
    |
        ctos strcat
        swap 1 drop-n swap udl
    ]
} ;

// "%41li%20ce" → "Ali ce"
: url-decode { strchars "" udl } ;

export hex-val
export udl
export url-decode

// body ctype statusline → response with an arbitrary status
: respond-status {
    "HTTP/1.0 " ` strcat
    "\r\nContent-Type: " strcat
    $1 strcat
    "\r\nContent-Length: " strcat
    $2 strlen i64tostr strcat
    "\r\n\r\n" strcat
    $2 strcat
    ` drop ` drop
} ;

export respond-status

// body → 405 response with Allow header (KV route methods)
: respond-405 {
    // body -> 405 response with Allow header
    _ strlen i64tostr           // [body, len-str]
    "HTTP/1.0 405 Method Not Allowed\r\nContent-Type: text/plain\r\nContent-Length: "
    ` strcat
    "\r\nAllow: GET, PUT, POST, DELETE" strcat
    "\r\n\r\n" strcat
    ` strcat
} ;

export respond-405
