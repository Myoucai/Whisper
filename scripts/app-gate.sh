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

(cd "$APP" && WS_PORT="$PORT" "$TMP/server.exe" >/dev/null 2>&1) &
SRV_PID=$!
sleep 2

api="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/api" 2>/dev/null)"
idx="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/f/about.html" 2>/dev/null | head -c 30)"
nf="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/zzz" 2>/dev/null)"

ok=1
[ "$api" = '{"ok":true,"lang":"whisper","chain":"self-hosted"}' ] || ok=0
[ -n "$idx" ] || ok=0
[ "$nf" = "not found" ] || ok=0

if [ "$ok" = 1 ]; then
    echo "=== app gate: PASS (web backend: JSON + static + 404 routes) ==="
    exit 0
else
    echo "=== app gate: FAIL (api=[$api] idx=[$idx] nf=[$nf]) ==="
    exit 1
fi
