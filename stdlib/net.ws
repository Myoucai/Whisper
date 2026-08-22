// Whisper network library — v1.0.1 native socket opcodes.

// body → full HTTP/1.0 text response
: http-respond {
    _ strlen i64tostr
    "HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\nContent-Length: "
    ` strcat
    "\r\n\r\n" strcat
    ` strcat
} ;

export http-respond
