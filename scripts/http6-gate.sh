#!/usr/bin/env bash
# @6 HTTP gate: cookie jar + user headers + @7 audit, no TLS required.
set -u
unset WHISPER_NET_ALLOW
stage=setup
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
"$PY" -u "$(winpath "$ROOT/tests/security/http6_server.py")" >"$TMP/server.log" 2>&1 &
SRV=$!
cleanup() {
    rc=$?
    trap - EXIT
    kill "$SRV" 2>/dev/null
    rm -rf "$TMP"
    if [ "$rc" -ne 0 ]; then
        echo "::error::http6-gate failed stage=$stage rc=$rc at [$BASH_COMMAND]" >&2
    fi
    exit "$rc"
}
trap cleanup EXIT
stage=server-ready
for _ in $(seq 1 50); do
    if (exec 3<>"/dev/tcp/127.0.0.1/$PORT") 2>/dev/null; then
        exec 3<&- 3>&-
        break
    fi
    sleep 0.1
done
RUN=""
if command -v timeout >/dev/null 2>&1; then RUN="timeout 20"; fi
stage=build
cat > "$TMP/app.ws" <<EOF
"GET" "http://127.0.0.1:$PORT/set" "{}" "" @6 drop drop drop
"GET" "http://127.0.0.1:$PORT/echo" "{\\"X-Test\\":\\"yes\\"}" "" @6 . drop drop
@7 .
EOF
"$BIN" build "$(winpath "$TMP/app.ws")" -t c2 -o "$(winpath "$TMP/app.exe")" >/dev/null 2>"$TMP/build.err" || {
    echo "http6-gate: build failed"; cat "$TMP/build.err"; exit 1; }
stage=run
WHISPER_NET=1 $RUN "$TMP/app.exe" >"$TMP/out" 2>"$TMP/err" || {
    echo "::error::http6 run failed: $(tr '\n' ' ' < "$TMP/err") server=$(tr '\n' ' ' < "$TMP/server.log")"
    echo "http6-gate: run failed"; cat "$TMP/err"; echo "--- server ---"; cat "$TMP/server.log"; exit 1; }
stage=body
body="$(head -1 "$TMP/out" | tr -d '\r')"
if [ "$body" != "cookie=sid=abc;xtest=yes" ]; then
    echo "http6-gate: bad body [$body]"; cat "$TMP/out"; exit 1
fi
stage=audit
if ! grep -q 'GET http://127.0.0.1:.*/set 200' "$TMP/out"; then
    echo "http6-gate: missing audit line"; cat "$TMP/out"; exit 1
fi
stage=audit-json
WHISPER_NET=1 WHISPER_NET_AUDIT=json $RUN "$TMP/app.exe" >"$TMP/audit-json.out" 2>"$TMP/audit-json.err" || {
    echo "http6-gate: json audit run failed"; cat "$TMP/audit-json.err"; exit 1; }
grep -q '"method":"GET".*"host":"127.0.0.1".*"status":200.*"bytes":' "$TMP/audit-json.out" \
    || { echo "http6-gate: bad JSON audit"; cat "$TMP/audit-json.out"; exit 1; }
stage=max-response
if WHISPER_NET=1 WHISPER_NET_MAX_RESPONSE=2 $RUN "$TMP/app.exe" >"$TMP/max.out" 2>"$TMP/max.err"; then
    echo "http6-gate: response cap ignored"; exit 1
fi
grep -q 'response exceeds WHISPER_NET_MAX_RESPONSE' "$TMP/max.err" \
    || { echo "http6-gate: missing response cap error"; cat "$TMP/max.err"; exit 1; }
stage=quota-headers
if WHISPER_NET=1 WHISPER_NET_MAX_HEADERS=1 $RUN "$TMP/app.exe" >"$TMP/qh.out" 2>"$TMP/qh.err"; then
    echo "http6-gate: header quota ignored"; exit 1
fi
grep -q 'response header quota exceeded' "$TMP/qh.err" \
    || { echo "http6-gate: missing header quota error"; cat "$TMP/qh.err"; exit 1; }
stage=quota-requests
if WHISPER_NET=1 WHISPER_NET_MAX_REQUESTS=1 $RUN "$TMP/app.exe" >"$TMP/qr.out" 2>"$TMP/qr.err"; then
    echo "http6-gate: request quota ignored"; exit 1
fi
grep -q 'request quota exceeded' "$TMP/qr.err" \
    || { echo "http6-gate: missing request quota error"; cat "$TMP/qr.err"; exit 1; }
stage=raw-build
cat > "$TMP/raw.ws" <<EOF
"POST" "http://127.0.0.1:$PORT/echo" "{\\"Content-Type\\":\\"application/octet-stream\\"}" "6869" @8 . drop drop
EOF
"$BIN" build "$(winpath "$TMP/raw.ws")" -t c2 -o "$(winpath "$TMP/raw.exe")" >/dev/null 2>"$TMP/raw-build.err" || {
    echo "http6-gate: raw build failed"; cat "$TMP/raw-build.err"; exit 1; }
stage=raw-run
WHISPER_NET=1 $RUN "$TMP/raw.exe" >"$TMP/raw.out" 2>"$TMP/raw.err" || {
    echo "http6-gate: raw run failed"; cat "$TMP/raw.err"; echo "--- server ---"; cat "$TMP/server.log"; exit 1; }
rawbody="$(head -1 "$TMP/raw.out" | tr -d '\r')"
if [ "$rawbody" != "len=2 hex=6869" ]; then
    echo "http6-gate: bad raw body [$rawbody]"; cat "$TMP/raw.out"; exit 1
fi
stage=timeout-build
cat > "$TMP/slow.ws" <<EOF
"GET" "http://127.0.0.1:$PORT/slow" "{}" "" @6 .
EOF
"$BIN" build "$(winpath "$TMP/slow.ws")" -t c2 -o "$(winpath "$TMP/slow.exe")" >/dev/null 2>"$TMP/slow-build.err" || {
    echo "http6-gate: slow build failed"; cat "$TMP/slow-build.err"; exit 1; }
stage=timeout-run
if WHISPER_NET=1 WHISPER_NET_TIMEOUT=1 $RUN "$TMP/slow.exe" >"$TMP/slow.out" 2>"$TMP/slow.err"; then
    echo "http6-gate: timeout was not enforced"; exit 1
fi
grep -q 'network read timeout' "$TMP/slow.err" \
    || { echo "http6-gate: missing timeout error"; cat "$TMP/slow.err"; exit 1; }
stage=redirect-build
cat > "$TMP/redir.ws" <<EOF
"GET" "http://127.0.0.1:$PORT/redirect" "{}" "" @6 drop drop .
EOF
"$BIN" build "$(winpath "$TMP/redir.ws")" -t c2 -o "$(winpath "$TMP/redir.exe")" >/dev/null 2>"$TMP/redir-build.err" || {
    echo "http6-gate: redirect build failed"; cat "$TMP/redir-build.err"; exit 1; }
stage=redirect-manual
WHISPER_NET=1 $RUN "$TMP/redir.exe" >"$TMP/redir.out" 2>"$TMP/redir.err" \
    || { echo "http6-gate: manual redirect failed"; cat "$TMP/redir.err"; exit 1; }
stage=redirect-deny
if WHISPER_NET=1 WHISPER_NET_REDIRECT=deny $RUN "$TMP/redir.exe" >"$TMP/deny.out" 2>"$TMP/deny.err"; then
    echo "http6-gate: redirect deny ignored"; exit 1
fi
grep -q 'redirect denied by WHISPER_NET_REDIRECT=deny' "$TMP/deny.err" \
    || { echo "http6-gate: missing redirect deny error"; cat "$TMP/deny.err"; exit 1; }
stage=redirect-follow
WHISPER_NET=1 WHISPER_NET_REDIRECT=follow $RUN "$TMP/redir.exe" >"$TMP/follow.out" 2>"$TMP/follow.err" \
    || { echo "http6-gate: redirect follow failed"; cat "$TMP/follow.err"; exit 1; }
[ "$(tr -d '\r' < "$TMP/follow.out")" = "200" ] \
    || { echo "http6-gate: follow did not reach /set"; cat "$TMP/follow.out"; exit 1; }

cat > "$TMP/loop.ws" <<EOF
"GET" "http://127.0.0.1:$PORT/loop" "{}" "" @6 drop drop .
EOF
"$BIN" build "$(winpath "$TMP/loop.ws")" -t c2 -o "$(winpath "$TMP/loop.exe")" >/dev/null 2>&1
stage=redirect-loop
if WHISPER_NET=1 WHISPER_NET_REDIRECT=follow $RUN "$TMP/loop.exe" >"$TMP/loop.out" 2>"$TMP/loop.err"; then
    echo "http6-gate: redirect loop was not limited"; exit 1
fi
grep -q 'redirect limit exceeded' "$TMP/loop.err" \
    || { echo "http6-gate: missing redirect limit"; cat "$TMP/loop.err"; exit 1; }

cat > "$TMP/cross.ws" <<EOF
"GET" "http://127.0.0.1:$PORT/cross" "{}" "" @6 drop drop .
EOF
"$BIN" build "$(winpath "$TMP/cross.ws")" -t c2 -o "$(winpath "$TMP/cross.exe")" >/dev/null 2>&1
stage=redirect-cross
WHISPER_NET=1 WHISPER_NET_REDIRECT=follow $RUN "$TMP/cross.exe" >"$TMP/cross.out" 2>"$TMP/cross.err" \
    || { echo "http6-gate: cross-origin redirect failed"; cat "$TMP/cross.err"; exit 1; }
[ "$(tr -d '\r' < "$TMP/cross.out")" = "302" ] \
    || { echo "http6-gate: cross-origin redirect was followed"; cat "$TMP/cross.out"; exit 1; }
echo "http6-gate: PASS (cookie/header/audit-json/raw/timeout/redirect follow)"
