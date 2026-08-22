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
        $1 req-path query-of "name" form-get url-decode
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
        "not found" "text/plain" "404 Not Found" respond-status
        return
    ]

    drop drop
    "not found" "text/plain" "404 Not Found" respond-status
} ;

: route-kv {
    // entry [req] -> exits [response]
    _ req-path path-only         // [req path]
    _ 4 9999 strslice            // [req path key]
    $2 req-method                // [req path key meth]

    dup "PUT" streq over "POST" streq | ??
        drop
        $2 req-body kv-set drop drop
        "{\"ok\":true}" "application/json" respond-as
        return
    ]
    dup "DELETE" streq ??
        drop kv-del drop drop
        "{\"deleted\":true}" "application/json" respond-as
        return
    ]
    dup "GET" streq $3 "/kv" streq & ??
        drop drop drop drop
        kv-keys "" { ` " " strcat ` strcat } @fold
        "text/plain" respond-as
        return
    ]
    dup "GET" streq ??
        drop
        kv-get
        swap drop swap drop
        dup strlen 0 > ??
            "text/plain" respond-as
            return
        ]
        drop
        "missing" "text/plain" "404 Not Found" respond-status
        return
    ]
    drop drop drop drop
    "method not allowed" respond-405
} ;

: handle {
    _ recv
    dup req-path path-only
    dup "/kv" strfind 0 >= ??
        drop route-kv
    | drop route
    ]
    send drop
} ;



"WS_PORT" getenv
dup strlen 0 = ?? drop "8097" | ]
strtoi64
"handle" serve-concurrent
