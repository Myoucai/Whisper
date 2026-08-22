#!/bin/bash
# Net gate: compile and RUN a real HTTP server through the self-hosted
# c2 chain, then fetch it with curl. Requires gcc + curl.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${BIN:-$ROOT/target/debug/whisper.exe}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"; [ -n "${SRV_PID:-}" ] && kill "$SRV_PID" 2>/dev/null' EXIT
PORT="${NET_GATE_PORT:-18123}"

cat > "$TMP/httpd.ws" <<EOF
// gate httpd — served by the self-hosted chain
: handle {
    _ recv drop
    "gate-ok" http-respond send drop
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
import "std/net"
$PORT listen
dup 0 >= ??
    serve
| drop "listen failed" .
]
EOF

if ! timeout 240 "$BIN" build "$TMP/httpd.ws" -t c2 -o "$TMP/httpd.exe" >/dev/null 2>&1; then
    echo "FAIL: c2 build failed"
    exit 1
fi

"$TMP/httpd.exe" >/dev/null 2>&1 &
SRV_PID=$!
sleep 2

body="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/" 2>/dev/null)"
body2="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/second" 2>/dev/null)"

if [ "$body" = "gate-ok" ] && [ "$body2" = "gate-ok" ]; then
    echo "=== net gate: PASS (httpd served 2 requests via self-hosted chain) ==="
    exit 0
else
    echo "=== net gate: FAIL (got [$body] / [$body2]) ==="
    exit 1
fi
