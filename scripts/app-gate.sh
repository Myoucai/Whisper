#!/bin/bash
# App gate: the Phase-D deliverable — a Whisper-written web backend,
# compiled by the self-hosted chain, serving real routes. Requires
# gcc + curl.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${BIN:-$ROOT/target/debug/whisper.exe}"
APP="$ROOT/apps/webapp"
PORT="${APP_GATE_PORT:-18211}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"; [ -n "${SRV_PID:-}" ] && kill "$SRV_PID" 2>/dev/null' EXIT

if ! (cd "$APP" && timeout 240 "$BIN" build server.ws -t c2 -o "$TMP/server.exe" >/dev/null 2>&1); then
    echo "FAIL: c2 build failed"
    exit 1
fi

# reset the persistent counter for a deterministic check
printf '0' > "$APP/data/visits.txt"

(cd "$APP" && WS_PORT="$PORT" "$TMP/server.exe" >/dev/null 2>&1) &
SRV_PID=$!
sleep 2

api="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/api" 2>/dev/null)"
idx="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/f/about.html" 2>/dev/null | head -c 30)"
nf="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/zzz" 2>/dev/null)"
greet="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/greet?name=ali+ce" 2>/dev/null)"
echo="$(curl -s --max-time 5 -d "payload-42" "http://127.0.0.1:$PORT/echo" 2>/dev/null)"
v1="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/visits" 2>/dev/null)"
v2="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/visits" 2>/dev/null)"

ok=1
[ "$api" = '{"ok":true,"lang":"whisper","chain":"self-hosted"}' ] || ok=0
[ -n "$idx" ] || ok=0
[ "$nf" = "not found" ] || ok=0
[ "$greet" = '{"hello":"ali ce"}' ] || ok=0
[ "$echo" = 'got: payload-42' ] || ok=0
[ "$v1" = '{"visits":1}' ] || ok=0
[ "$v2" = '{"visits":2}' ] || ok=0

if [ "$ok" = 1 ]; then
    echo "=== app gate: PASS (routes: index/JSON/static/404/query/POST-body/persistence) ==="
    exit 0
else
    echo "=== app gate: FAIL ==="
    echo "  api=[$api] idx=[$idx] nf=[$nf]"
    echo "  greet=[$greet] echo=[$echo] v1=[$v1] v2=[$v2]"
    exit 1
fi
