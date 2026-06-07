// Whisper HTTP Server
// Usage: whisper serve server.ws
// Test: curl http://localhost:8080

import "std/str"

: handler {
    // request = [method, path, body] — pick the path
    1 @nth
    _ "/" streq
    ??["200 OK" "text/html" "<h1>Whisper v1.0</h1>"]
    |["404" "text/plain" "Not Found"] ]
} ;

export handler
