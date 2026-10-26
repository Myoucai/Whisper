#!/usr/bin/env bash
# native WHISPER_NET_ALLOW gate (host allowlist before connect).
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
export PATH="/mingw64/bin:/usr/bin:/c/msys64/clang64/bin:$PATH"
BIN="${BIN:-$ROOT/target/debug/whisper}"
[ -f "$BIN" ] || BIN="$BIN.exe"
[ -f "$BIN" ] || { echo "net-allow-gate: missing $BIN" >&2; exit 1; }
winpath() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
TMP="$ROOT/target/net-allow-gate"
rm -rf "$TMP"; mkdir -p "$TMP"
cat > "$TMP/app.ws" <<'EOF'
"http://127.0.0.1:1/x" @2 drop
EOF
"$BIN" build "$(winpath "$TMP/app.ws")" -t c2 -o "$(winpath "$TMP/app.exe")" >/dev/null 2>&1 \
    || { echo "net-allow-gate: build failed"; exit 1; }

if WHISPER_NET_ALLOW=example.com "$TMP/app.exe" >"$TMP/deny.out" 2>"$TMP/deny.err"; then
    echo "net-allow-gate: allowlist did not reject 127.0.0.1"; exit 1
fi
grep -q 'not allowed by WHISPER_NET_ALLOW' "$TMP/deny.err" \
    || { echo "net-allow-gate: missing policy error"; cat "$TMP/deny.err"; exit 1; }

if WHISPER_NET_ALLOW=127.0.0.1 "$TMP/app.exe" >"$TMP/allow.out" 2>"$TMP/allow.err"; then
    echo "net-allow-gate: connection unexpectedly succeeded"; exit 1
fi
grep -q 'Connect to 127.0.0.1:1 failed' "$TMP/allow.err" \
    || { echo "net-allow-gate: allowed host did not reach connect"; cat "$TMP/allow.err"; exit 1; }

echo "net-allow-gate: PASS (deny before connect; allow reaches connect)"
