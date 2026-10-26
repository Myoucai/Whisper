#!/usr/bin/env bash
# P1 native HTTPS gate: @6 via dynamically loaded OpenSSL.
set -u
unset WHISPER_NET_ALLOW
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
export PATH="/mingw64/bin:/usr/bin:/c/msys64/clang64/bin:$PATH"
export MSYS2_ARG_CONV_EXCL='*'
BIN="${BIN:-$ROOT/target/debug/whisper}"
[ -f "$BIN" ] || BIN="$BIN.exe"
[ -f "$BIN" ] || { echo "https-gate: missing $BIN" >&2; exit 1; }
command -v openssl >/dev/null 2>&1 || { echo "https-gate: openssl not found" >&2; exit 1; }
command -v gcc >/dev/null 2>&1 || { echo "https-gate: gcc not found" >&2; exit 1; }
TMP="$ROOT/target/https-gate"
TMPREL="target/https-gate"
rm -rf "$TMP"; mkdir -p "$TMP"
PORT="${HTTPS_GATE_PORT:-18443}"
PASS=0; FAIL=0
winpath() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
ok()  { PASS=$((PASS+1)); printf '  %-6s ✅ %s\n' "$1" "$2"; }
bad() { FAIL=$((FAIL+1)); printf '  %-6s ❌ %s\n' "$1" "$2"; }

openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$TMPREL/key.pem" -out "$TMPREL/cert.pem" -days 1 \
  -subj "/CN=127.0.0.1" -addext "subjectAltName=IP:127.0.0.1" \
  >/dev/null 2>&1 || { echo "https-gate: cert generation failed" >&2; exit 1; }
CA="$(cygpath -w "$TMP/cert.pem" 2>/dev/null || echo "$TMP/cert.pem")"
openssl s_server -accept "$PORT" -cert "$TMPREL/cert.pem" -key "$TMPREL/key.pem" -www -quiet \
  >"$TMP/server.log" 2>&1 &
SRV=$!
trap 'kill "$SRV" 2>/dev/null; rm -rf "$TMP"' EXIT
sleep 1

cat > "$TMP/get.ws" <<EOF
"GET" "https://127.0.0.1:$PORT/" "{}" "" @6 drop drop .
EOF
"$BIN" build "$(winpath "$TMP/get.ws")" -t c2 -o "$(winpath "$TMP/get.exe")" >/dev/null 2>"$TMP/build.err" || {
    echo "https-gate: c2 build failed"; cat "$TMP/build.err"; exit 1; }

if WHISPER_NET=1 WHISPER_TLS_CA="$CA" "$TMP/get.exe" >"$TMP/ok.out" 2>"$TMP/ok.err" \
   && [ "$(tr -d '\r' < "$TMP/ok.out")" = "200" ]; then
    ok H-1 "自签 CA：状态 200"
else
    bad H-1 "自签 CA HTTPS 失败"; cat "$TMP/ok.err"
fi

if WHISPER_NET=1 "$TMP/get.exe" >"$TMP/noca.out" 2>"$TMP/noca.err"; then
    bad H-2 "未信任 CA 仍成功"
else
    ok H-2 "未信任 CA 被拒绝"
fi

cat > "$TMP/badhost.ws" <<EOF
"GET" "https://localhost:$PORT/" "{}" "" @6 drop drop .
EOF
"$BIN" build "$(winpath "$TMP/badhost.ws")" -t c2 -o "$(winpath "$TMP/badhost.exe")" >/dev/null 2>&1
if WHISPER_NET=1 WHISPER_TLS_CA="$CA" "$TMP/badhost.exe" >"$TMP/bad.out" 2>"$TMP/bad.err"; then
    bad H-3 "hostname mismatch 仍成功"
else
    ok H-3 "hostname mismatch 被拒绝"
fi

echo "https-gate: $PASS/$((PASS+FAIL))"
[ "$FAIL" -eq 0 ]
