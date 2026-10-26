#!/bin/bash
# App gate: the Phase-D deliverable — a Whisper-written web backend,
# compiled by the self-hosted chain, serving real routes. Requires
# gcc + curl.
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
[ -n "$BIN" ] || { echo "app-gate: whisper binary not found" >&2; exit 1; }
TIMEOUT="$(command -v timeout || command -v gtimeout || true)"
APP="$ROOT/apps/webapp"
PORT="${APP_GATE_PORT:-18211}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"; [ -n "${SRV_PID:-}" ] && kill "$SRV_PID" 2>/dev/null' EXIT

if [ -n "$TIMEOUT" ]; then
    (cd "$APP" && "$TIMEOUT" 240 "$BIN" build server.ws -t c2 -o "$TMP/server.exe" >/dev/null 2>&1) || {
        echo "FAIL: c2 build failed"; exit 1; }
else
    (cd "$APP" && "$BIN" build server.ws -t c2 -o "$TMP/server.exe" >/dev/null 2>&1) || {
        echo "FAIL: c2 build failed"; exit 1; }
fi

# reset the persistent counter for a deterministic check
printf '0' > "$APP/data/visits.txt"

(cd "$APP" && WS_PORT="$PORT" WHISPER_HEAP_MB=128 WHISPER_NET=1 "$TMP/server.exe" >"$TMP/server.log" 2>&1) &
SRV_PID=$!
ready=0
i=0
while [ "$i" -lt 20 ]; do
    if curl -s --max-time 1 "http://127.0.0.1:$PORT/api" >/dev/null 2>&1; then
        ready=1; break
    fi
    sleep 0.5
    i=$((i+1))
done
if [ "$ready" -eq 0 ]; then
    echo "FAIL: app server did not become ready"
    tail -20 "$TMP/server.log" 2>/dev/null
    exit 1
fi

api="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/api" 2>/dev/null)"
idx="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/f/about.html" 2>/dev/null | head -c 30)"
nf="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/zzz" 2>/dev/null)"
greet="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/greet?name=ali+ce" 2>/dev/null)"
echo="$(curl -s --max-time 5 -d "payload-42" "http://127.0.0.1:$PORT/echo" 2>/dev/null)"
v1="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/visits" 2>/dev/null)"
v2="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/visits" 2>/dev/null)"

kvput="$(curl -s --max-time 5 -X PUT -d "kv-value" "http://127.0.0.1:$PORT/kv/greeting" 2>/dev/null)"
kvget="$(curl -s --max-time 5 "http://127.0.0.1:$PORT/kv/greeting" 2>/dev/null)"
kvmiss="$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT/kv/zzz" 2>/dev/null)"
kv405="$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" -X PATCH "http://127.0.0.1:$PORT/kv/x" 2>/dev/null)"
kvdel="$(curl -s --max-time 5 -X DELETE "http://127.0.0.1:$PORT/kv/greeting" 2>/dev/null)"
kv404="$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT/zzz" 2>/dev/null)"

ok=1
[ "$api" = '{"ok":true,"lang":"whisper","chain":"self-hosted"}' ] || ok=0
[ -n "$idx" ] || ok=0
[ "$nf" = "not found" ] || ok=0
[ "$greet" = '{"hello":"ali ce"}' ] || ok=0
[ "$echo" = 'got: payload-42' ] || ok=0
[ "$v1" = '{"visits":1}' ] || ok=0
[ "$v2" = '{"visits":2}' ] || ok=0
[ "$kvput" = '{"ok":true}' ] || ok=0
[ "$kvget" = "kv-value" ] || ok=0
[ "$kvmiss" = "404" ] || ok=0
[ "$kv405" = "405" ] || ok=0
[ "$kvdel" = '{"deleted":true}' ] || ok=0
[ "$kv404" = "404" ] || ok=0

if [ "$ok" = 1 ]; then
    echo "=== app gate: PASS (13 routes: index/JSON/static/query/POST-body/persistence/KV-CRUD/404/405) ==="
    exit 0
else
    echo "=== app gate: FAIL ==="
    echo "  api=[$api] idx=[$idx] nf=[$nf]"
    echo "  greet=[$greet] echo=[$echo] v1=[$v1] v2=[$v2]"
    tail -20 "$TMP/server.log" 2>/dev/null
    exit 1
fi
