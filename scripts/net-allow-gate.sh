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

if WHISPER_NET_ALLOW=127.0.0.1 "$TMP/app.exe" >"$TMP/private.out" 2>"$TMP/private.err"; then
    echo "net-allow-gate: private destination unexpectedly allowed"; exit 1
fi
grep -q 'local/private\|private/link-local' "$TMP/private.err" \
    || { echo "net-allow-gate: missing private-IP rejection"; cat "$TMP/private.err"; exit 1; }

if WHISPER_NET_ALLOW=127.0.0.1 WHISPER_NET_ALLOW_PRIVATE=1 \
   "$TMP/app.exe" >"$TMP/allow.out" 2>"$TMP/allow.err"; then
    echo "net-allow-gate: connection unexpectedly succeeded"; exit 1
fi
grep -q 'Connect to 127.0.0.1:1 failed' "$TMP/allow.err" \
    || { echo "net-allow-gate: allowed host did not reach connect"; cat "$TMP/allow.err"; exit 1; }

if WHISPER_NET_METHODS=POST "$TMP/app.exe" >"$TMP/method.out" 2>"$TMP/method.err"; then
    echo "net-allow-gate: disallowed method succeeded"; exit 1
fi
grep -q 'method not allowed by WHISPER_NET_METHODS' "$TMP/method.err" \
    || { echo "net-allow-gate: missing method rejection"; cat "$TMP/method.err"; exit 1; }

if WHISPER_NET_PORTS=443 "$TMP/app.exe" >"$TMP/port.out" 2>"$TMP/port.err"; then
    echo "net-allow-gate: disallowed port succeeded"; exit 1
fi
grep -q 'port not allowed by WHISPER_NET_PORTS' "$TMP/port.err" \
    || { echo "net-allow-gate: missing port rejection"; cat "$TMP/port.err"; exit 1; }

cat > "$TMP/raw.ws" <<'EOF'
0 listen drop
EOF
"$BIN" build "$(winpath "$TMP/raw.ws")" -t c2 -o "$(winpath "$TMP/raw.exe")" >/dev/null 2>&1 \
    || { echo "net-allow-gate: raw build failed"; exit 1; }
if WHISPER_NET_ALLOW=127.0.0.1 "$TMP/raw.exe" >"$TMP/raw.out" 2>"$TMP/raw.err"; then
    echo "net-allow-gate: raw listen bypassed allowlist"; exit 1
fi
grep -q 'raw sockets disabled by network policy' "$TMP/raw.err" \
    || { echo "net-allow-gate: missing raw policy error"; cat "$TMP/raw.err"; exit 1; }
WHISPER_NET_RAW=1 "$TMP/raw.exe" >"$TMP/raw2.out" 2>"$TMP/raw2.err" \
    || { echo "net-allow-gate: WHISPER_NET_RAW=1 rejected"; cat "$TMP/raw2.err"; exit 1; }
if WHISPER_NET_RAW=1 WHISPER_NET_LISTEN_PORTS=8080 "$TMP/raw.exe" >"$TMP/raw3.out" 2>"$TMP/raw3.err"; then
    echo "net-allow-gate: listen port allowlist ignored"; exit 1
fi
grep -q 'listen port not allowed by WHISPER_NET_LISTEN_PORTS' "$TMP/raw3.err" \
    || { echo "net-allow-gate: missing listen port error"; cat "$TMP/raw3.err"; exit 1; }

echo "net-allow-gate: PASS (host/private/method/port/raw-listen policy; override reaches connect)"
