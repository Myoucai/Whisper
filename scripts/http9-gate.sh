#!/usr/bin/env bash
# @9 parallel fan-out gate (native C chain).
set -u
unset WHISPER_NET_ALLOW
unset WHISPER_NET_AUDIT
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
export PATH="/mingw64/bin:/usr/bin:/c/msys64/clang64/bin:$PATH"
export MSYS2_ARG_CONV_EXCL='*'
BIN="${BIN:-$ROOT/target/debug/whisper}"
[ -f "$BIN" ] || BIN="$BIN.exe"
[ -f "$BIN" ] || { echo "http9-gate: missing $BIN" >&2; exit 1; }
winpath() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
PY=""
for p in python3 python; do command -v "$p" >/dev/null 2>&1 && { PY="$p"; break; }; done
[ -n "$PY" ] || { echo "http9-gate: python not found" >&2; exit 1; }
TMP="$ROOT/target/http9-gate"
rm -rf "$TMP"; mkdir -p "$TMP"
PORT="${HTTP9_GATE_PORT:-18083}"
HTTP6_PORT="$PORT" "$PY" -u "$(winpath "$ROOT/tests/security/http6_server.py")" >"$TMP/server.log" 2>&1 &
SRV=$!
stage=server-ready
cleanup() {
    rc=$?
    trap - EXIT
    kill "$SRV" 2>/dev/null
    rm -rf "$TMP"
    [ "$rc" -ne 0 ] && echo "::error::http9-gate failed stage=$stage rc=$rc" >&2
    exit "$rc"
}
trap cleanup EXIT
for _ in $(seq 1 50); do
    (exec 3<>"/dev/tcp/127.0.0.1/$PORT") 2>/dev/null && { exec 3<&- 3>&-; break; }
    sleep 0.1
done
stage=build
cat > "$TMP/app.ws" <<EOF
2 "GET" "http://127.0.0.1:$PORT/echo" "{}" "" @9 .
EOF
"$BIN" build "$(winpath "$TMP/app.ws")" -t c2 -o "$(winpath "$TMP/app.exe")" >/dev/null 2>"$TMP/build.err" \
    || { echo "http9-gate: build failed"; cat "$TMP/build.err"; exit 1; }
stage=run
WHISPER_NET=1 "$TMP/app.exe" >"$TMP/out" 2>"$TMP/err" \
    || { echo "http9-gate: run failed"; cat "$TMP/err"; exit 1; }
if [ "$(grep -o '"status":200' "$TMP/out" | wc -l | tr -d ' ')" != "2" ]; then
    echo "http9-gate: expected two status 200 results"; cat "$TMP/out"; exit 1
fi
grep -q '"body":"cookie=;xtest="' "$TMP/out" \
    || { echo "http9-gate: unexpected body"; cat "$TMP/out"; exit 1; }
stage=range
cat > "$TMP/bad.ws" <<EOF
0 "GET" "http://127.0.0.1:$PORT/echo" "{}" "" @9 .
EOF
"$BIN" build "$(winpath "$TMP/bad.ws")" -t c2 -o "$(winpath "$TMP/bad.exe")" >/dev/null 2>&1
if WHISPER_NET=1 "$TMP/bad.exe" >"$TMP/bad.out" 2>"$TMP/bad.err"; then
    echo "http9-gate: count 0 was accepted"; exit 1
fi
grep -q '@9 count must be in 1..16' "$TMP/bad.err" \
    || { echo "http9-gate: missing range error"; cat "$TMP/bad.err"; exit 1; }
echo "http9-gate: PASS (@9 fan-out x2 + range rejection)"
