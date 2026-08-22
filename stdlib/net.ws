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
