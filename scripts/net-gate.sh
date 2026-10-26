#!/bin/bash
# Net gate: compile and RUN a real HTTP server through the self-hosted
# c2 chain, then fetch it with curl. Requires gcc + curl.
set -u
unset WHISPER_NET_ALLOW
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${BIN:-}"
if [ -z "$BIN" ]; then
    for b in "$ROOT/target/debug/whisper" "$ROOT/target/debug/whisper.exe" \
             "$ROOT/target/release/whisper" "$ROOT/target/release/whisper.exe"; do
        [ -x "$b" ] && { BIN="$b"; break; }
    done
fi
[ -n "$BIN" ] || { echo "net-gate: whisper binary not found" >&2; exit 1; }
TIMEOUT="$(command -v timeout || command -v gtimeout || true)"
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

if [ -n "$TIMEOUT" ]; then
    "$TIMEOUT" 240 "$BIN" build "$TMP/httpd.ws" -t c2 -o "$TMP/httpd.exe" >/dev/null 2>&1 || {
        echo "FAIL: c2 build failed"; exit 1; }
else
    "$BIN" build "$TMP/httpd.ws" -t c2 -o "$TMP/httpd.exe" >/dev/null 2>&1 || {
        echo "FAIL: c2 build failed"; exit 1; }
fi

# Switch: native network must be explicitly disabled by WHISPER_NET=0.
if WHISPER_NET=0 "$TMP/httpd.exe" >"$TMP/off.out" 2>"$TMP/off.err"; then
    echo "FAIL: server ran with WHISPER_NET=0"
    exit 1
fi
grep -q 'network disabled' "$TMP/off.err" || {
    echo "FAIL: WHISPER_NET=0 did not explain the refusal"
    cat "$TMP/off.err"
    exit 1
}

WHISPER_NET=1 "$TMP/httpd.exe" >/dev/null 2>&1 &
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
