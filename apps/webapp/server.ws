// Whisper web backend — runs on the self-hosted chain (gcc only).
//   build+run: bash apps/webapp/run.sh
//   routes:    /        HTML index
//              /api     JSON status
//              /f/NAME  static file from public/
//              else     404
import "std/net"

: respond-as {
    // body ctype → full HTTP response
    `
    "HTTP/1.0 200 OK\r\nContent-Type: " rot strcat
    "\r\nContent-Length: " strcat
    $1 strlen i64tostr strcat
    "\r\n\r\n" strcat
    ` strcat
} ;

: respond-404 {
    drop
    "not found" "text/plain" respond-as
} ;

: req-path {
    // request → path (second token of the first line)
    "\r\n" strsplit 0 @nth
    " " strsplit 1 @nth
} ;

: static-file {
    // "/f/name" → file body | ""
    3 9999 strslice
    "public/" swap strcat
    read-file
} ;

: route {
    // path → response
    dup "/" streq ??
        drop
        "<html><body><h1>Whisper Web</h1><p>Served by the self-hosted chain.</p></body></html>"
        "text/html" respond-as
        return
    ]
    dup "/api" strfind 0 = ??
        drop
        "{\"ok\":true,\"lang\":\"whisper\",\"chain\":\"self-hosted\"}"
        "application/json" respond-as
        return
    ]
    dup "/f/" strfind 0 = ??
        static-file
        dup strlen 0 > ??
            "text/html" respond-as
            return
        ]
        drop
        respond-404
        return
    ]
    respond-404
} ;

: handle {
    _ recv req-path route send drop
} ;

: serve {
    _ accept
    dup 0 >= ??
        dup handle
        close
        serve
    | drop
        serve
    ]
} ;

"WS_PORT" getenv
dup strlen 0 = ?? drop "8097" | ]
strtoi64
listen
dup 0 >= ??
    serve
| drop "listen failed" .
]
