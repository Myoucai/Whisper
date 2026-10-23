#!/usr/bin/env bash
# @6 HTTP gate: cookie jar + user headers + @7 audit, no TLS required.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
export PATH="/mingw64/bin:/usr/bin:/c/msys64/clang64/bin:$PATH"
export MSYS2_ARG_CONV_EXCL='*'
BIN="${BIN:-$ROOT/target/debug/whisper}"
[ -f "$BIN" ] || BIN="$BIN.exe"
[ -f "$BIN" ] || { echo "http6-gate: missing $BIN" >&2; exit 1; }
winpath() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
PY=""
for p in python3 python; do command -v "$p" >/dev/null 2>&1 && { PY="$p"; break; }; done
[ -n "$PY" ] || { echo "http6-gate: python not found" >&2; exit 1; }
TMP="$ROOT/target/http6-gate"
rm -rf "$TMP"; mkdir -p "$TMP"
PORT="${HTTP6_GATE_PORT:-18081}"
"$PY" "$(winpath "$ROOT/tests/security/http6_server.py")" >"$TMP/server.log" 2>&1 &
SRV=$!
trap 'kill "$SRV" 2>/dev/null; rm -rf "$TMP"' EXIT
sleep 1
cat > "$TMP/app.ws" <<EOF
"GET" "http://127.0.0.1:$PORT/set" "{}" "" @6 drop drop drop
"GET" "http://127.0.0.1:$PORT/echo" "{\\"X-Test\\":\\"yes\\"}" "" @6 . drop drop
@7 .
EOF
"$BIN" build "$(winpath "$TMP/app.ws")" -t c2 -o "$(winpath "$TMP/app.exe")" >/dev/null 2>"$TMP/build.err" || {
    echo "http6-gate: build failed"; cat "$TMP/build.err"; exit 1; }
WHISPER_NET=1 "$TMP/app.exe" >"$TMP/out" 2>"$TMP/err" || {
    echo "http6-gate: run failed"; cat "$TMP/err"; exit 1; }
body="$(head -1 "$TMP/out" | tr -d '\r')"
if [ "$body" != "cookie=sid=abc;xtest=yes" ]; then
    echo "http6-gate: bad body [$body]"; cat "$TMP/out"; exit 1
fi
if ! grep -q 'GET http://127.0.0.1:.*/set 200' "$TMP/out"; then
    echo "http6-gate: missing audit line"; cat "$TMP/out"; exit 1
fi
echo "http6-gate: PASS (cookie + user header + audit)"
