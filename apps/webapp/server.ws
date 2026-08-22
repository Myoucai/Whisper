// Whisper web backend — runs on the self-hosted chain (gcc only).
//   build+run: bash apps/webapp/run.sh
//   GET  /            HTML index
//   GET  /api         JSON status
//   GET  /greet?name= JSON greeting (query param, form-decoded)
//   POST /echo        echoes the request body (text/plain)
//   GET  /visits      persistent hit counter (data/visits.txt)
//   GET  /f/<name>    static file from public/ (404 on miss)
//   else              404
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

: load-count { "data/visits.txt" read-file dup strlen 0 = ?? drop 0 | strtoi64 ] } ;
: save-count { "data/visits.txt" swap i64tostr write-file drop } ;

: route {
    // request → response (consumes exactly one, produces exactly one)
    dup req-path path-only
    // stack: [req, path]

    dup "/" streq ??
        drop drop
        "<html><body><h1>Whisper Web</h1><p>Served by the self-hosted chain.</p></body></html>"
        "text/html" respond-as
        return
    ]

    dup "/api" streq ??
        drop drop
        "{\"ok\":true,\"lang\":\"whisper\",\"chain\":\"self-hosted\"}"
        "application/json" respond-as
        return
    ]

    dup "/greet" streq ??
        $1 req-path query-of "name" form-get
        ` drop ` drop
        dup strlen 0 > ??
            "{\"hello\":\"" ` strcat "\"}" strcat
        | drop "{\"hello\":\"world\"}"
        ]
        "application/json" respond-as
        return
    ]

    dup "/echo" streq ??
        $1 req-body
        ` drop ` drop
        "got: " swap strcat
        "text/plain" respond-as
        return
    ]

    dup "/visits" streq ??
        drop drop
        load-count 1 + dup save-count
        i64tostr
        "{\"visits\":" ` strcat "}" strcat
        "application/json" respond-as
        return
    ]

    dup "/f/" strfind 0 = ??
        $1 req-path path-only 3 9999 strslice
        ` drop ` drop
        "public/" swap strcat read-file
        dup strlen 0 > ??
            "text/html" respond-as
            return
        ]
        drop
        "not found" "text/plain" respond-as
        return
    ]

    drop drop
    "not found" "text/plain" respond-as
} ;

: handle {
    _ recv route send drop
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
