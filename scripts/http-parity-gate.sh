#!/usr/bin/env bash
# VM/C parity for native HTTP session words (@6/@7/@8).
# Note: this VM revision currently wants a dummy argument before @7 even
# though the typechecker declares @7 arity 0; both chains are fed "" below so
# the comparison measures output, not that VM quirk.
set -u
unset WHISPER_NET_ALLOW
unset WHISPER_NET_AUDIT
unset WHISPER_NET_REDIRECT
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
export PATH="/mingw64/bin:/usr/bin:/c/msys64/clang64/bin:$PATH"
export MSYS2_ARG_CONV_EXCL='*'
BIN="${BIN:-$ROOT/target/debug/whisper}"
[ -f "$BIN" ] || BIN="$BIN.exe"
[ -f "$BIN" ] || { echo "http-parity-gate: missing $BIN" >&2; exit 1; }
winpath() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
PY=""
for p in python3 python; do command -v "$p" >/dev/null 2>&1 && { PY="$p"; break; }; done
[ -n "$PY" ] || { echo "http-parity-gate: python not found" >&2; exit 1; }
TMP="$ROOT/target/http-parity"
rm -rf "$TMP"; mkdir -p "$TMP"
PORT="${HTTP_PARITY_PORT:-18082}"
HTTP6_PORT="$PORT" "$PY" -u "$(winpath "$ROOT/tests/security/http6_server.py")" >"$TMP/server.log" 2>&1 &
SRV=$!
stage=server-ready
cleanup() {
    rc=$?
    trap - EXIT
    kill "$SRV" 2>/dev/null
    rm -rf "$TMP"
    if [ "$rc" -ne 0 ]; then
        echo "::error::http-parity-gate failed stage=$stage rc=$rc" >&2
        echo "--- server ---" >&2
        cat "$TMP/server.log" >&2 2>/dev/null || true
    fi
    exit "$rc"
}
trap cleanup EXIT
for _ in $(seq 1 50); do
    if (exec 3<>"/dev/tcp/127.0.0.1/$PORT") 2>/dev/null; then
        exec 3<&- 3>&-
        break
    fi
    sleep 0.1
done

norm() { tr -d '\r' < "$1"; }

run_case() {
    name="$1"; src="$2"; expected="$3"
    stage="$name-vm"
    "$BIN" run --no-strict-types --allow-http "$(winpath "$src")" >"$TMP/$name.vm" 2>"$TMP/$name.vm.err" \
        || { echo "http-parity-gate: $name VM failed"; cat "$TMP/$name.vm.err"; exit 1; }
    stage="$name-c2-build"
    "$BIN" build "$(winpath "$src")" -t c2 -o "$(winpath "$TMP/$name.exe")" >/dev/null 2>"$TMP/$name.build.err" \
        || { echo "http-parity-gate: $name c2 build failed"; cat "$TMP/$name.build.err"; exit 1; }
    stage="$name-c2-run"
    WHISPER_NET=1 "$TMP/$name.exe" >"$TMP/$name.c2" 2>"$TMP/$name.c2.err" \
        || { echo "http-parity-gate: $name c2 failed"; cat "$TMP/$name.c2.err"; exit 1; }
    stage="$name-compare"
    vmout="$(tr -d '\r"' < "$TMP/$name.vm")"
    c2out="$(tr -d '\r"' < "$TMP/$name.c2")"
    if [ "$vmout" != "$c2out" ]; then
        echo "http-parity-gate: $name output differs"
        echo "--- VM ---"; printf '%s\n' "$vmout"
        echo "--- C2 ---"; printf '%s\n' "$c2out"
        exit 1
    fi
    if ! printf '%s\n' "$vmout" | grep -q "$expected"; then
        echo "http-parity-gate: $name missing expected [$expected]"
        printf '%s\n' "$vmout"; exit 1
    fi
}

run_header_case() {
    name="$1"; src="$2"; expect_key="${3:-}"
    stage="$name-vm"
    "$BIN" run --no-strict-types --allow-http "$(winpath "$src")" >"$TMP/$name.vm" 2>"$TMP/$name.vm.err" \
        || { echo "http-parity-gate: $name VM failed"; cat "$TMP/$name.vm.err"; exit 1; }
    stage="$name-c2-build"
    "$BIN" build "$(winpath "$src")" -t c2 -o "$(winpath "$TMP/$name.exe")" >/dev/null 2>"$TMP/$name.build.err" \
        || { echo "http-parity-gate: $name c2 build failed"; cat "$TMP/$name.build.err"; exit 1; }
    stage="$name-c2-run"
    WHISPER_NET=1 "$TMP/$name.exe" >"$TMP/$name.c2" 2>"$TMP/$name.c2.err" \
        || { echo "http-parity-gate: $name c2 failed"; cat "$TMP/$name.c2.err"; exit 1; }
    stage="$name-compare"
    vmjson="$(tr -d '\r' < "$TMP/$name.vm")"
    c2json="$(tr -d '\r' < "$TMP/$name.c2")"
    vmjson="${vmjson#\"}"; vmjson="${vmjson%\"}"
    printf '%s' "$vmjson" >"$TMP/$name.vm.json"
    printf '%s' "$c2json" >"$TMP/$name.c2.json"
    "$PY" - "$(winpath "$TMP/$name.vm.json")" "$(winpath "$TMP/$name.c2.json")" "$expect_key" <<'PY'
import json
import sys

def load(path):
    text = open(path, encoding="utf-8").read().strip()
    value = json.loads(text)
    if isinstance(value, str):
        value = json.loads(value)
    return {k.lower(): v for k, v in value.items()}

vm = load(sys.argv[1])
c2 = load(sys.argv[2])
if vm != c2:
    raise SystemExit("headers differ: VM=%r C2=%r" % (vm, c2))
expected = sys.argv[3]
if expected and expected not in vm:
    raise SystemExit("expected header %r missing: %r" % (expected, vm))
print("header parity: %s" % ", ".join(sorted(vm)))
PY
    rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "http-parity-gate: $name header JSON parity failed"
        exit 1
    fi
}

run_parallel_case() {
    name="$1"; src="$2"
    stage="$name-vm"
    "$BIN" run --no-strict-types --allow-http "$(winpath "$src")" >"$TMP/$name.vm" 2>"$TMP/$name.vm.err" \
        || { echo "http-parity-gate: $name VM failed"; cat "$TMP/$name.vm.err"; exit 1; }
    stage="$name-c2-build"
    "$BIN" build "$(winpath "$src")" -t c2 -o "$(winpath "$TMP/$name.exe")" >/dev/null 2>"$TMP/$name.build.err" \
        || { echo "http-parity-gate: $name c2 build failed"; cat "$TMP/$name.build.err"; exit 1; }
    stage="$name-c2-run"
    WHISPER_NET=1 "$TMP/$name.exe" >"$TMP/$name.c2" 2>"$TMP/$name.c2.err" \
        || { echo "http-parity-gate: $name c2 failed"; cat "$TMP/$name.c2.err"; exit 1; }
    stage="$name-compare"
    vmjson="$(tr -d '\r' < "$TMP/$name.vm")"
    c2json="$(tr -d '\r' < "$TMP/$name.c2")"
    vmjson="${vmjson#\"}"; vmjson="${vmjson%\"}"
    printf '%s' "$vmjson" >"$TMP/$name.vm.json"
    printf '%s' "$c2json" >"$TMP/$name.c2.json"
    "$PY" - "$(winpath "$TMP/$name.vm.json")" "$(winpath "$TMP/$name.c2.json")" <<'PY'
import json
import sys

def load(path):
    text = open(path, encoding="utf-8").read().strip()
    value = json.loads(text)
    if isinstance(value, str):
        value = json.loads(value)
    return value

vm = load(sys.argv[1])
c2 = load(sys.argv[2])
if len(vm) != len(c2):
    raise SystemExit("length differs: VM=%d C2=%d" % (len(vm), len(c2)))
for i, (a, b) in enumerate(zip(vm, c2)):
    if a.get("status") != b.get("status"):
        raise SystemExit("item %d status differs: %r vs %r" % (i, a.get("status"), b.get("status")))
    if a.get("body") != b.get("body"):
        raise SystemExit("item %d body differs: %r vs %r" % (i, a.get("body"), b.get("body")))
    ah = {k.lower(): v for k, v in (a.get("headers") or {}).items()}
    bh = {k.lower(): v for k, v in (b.get("headers") or {}).items()}
    if ah != bh:
        raise SystemExit("item %d headers differ: %r vs %r" % (i, ah, bh))
print("parallel status/body/header parity: %d items" % len(vm))
PY
    rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "http-parity-gate: $name JSON parity failed"
        exit 1
    fi
}

cat > "$TMP/session.ws" <<EOF
"GET" "http://127.0.0.1:$PORT/set" "{}" "" @6 drop drop drop
"GET" "http://127.0.0.1:$PORT/echo" "{\\"X-Test\\":\\"yes\\"}" "" @6 . drop drop
"" @7 .
EOF
run_case session "$TMP/session.ws" 'cookie=sid=abc;xtest=yes'

cat > "$TMP/headers.ws" <<EOF
"GET" "http://127.0.0.1:$PORT/echo" "{}" "" @6 drop . drop
EOF
run_header_case headers "$TMP/headers.ws" content-type

cat > "$TMP/redirect.ws" <<EOF
"GET" "http://127.0.0.1:$PORT/redirect" "{}" "" @6 drop drop .
EOF
run_case redirect "$TMP/redirect.ws" '302'

cat > "$TMP/redirect_headers.ws" <<EOF
"GET" "http://127.0.0.1:$PORT/redirect" "{}" "" @6 drop . drop
EOF
run_header_case redirect_headers "$TMP/redirect_headers.ws" location

cat > "$TMP/raw.ws" <<EOF
"POST" "http://127.0.0.1:$PORT/echo" "{\\"Content-Type\\":\\"application/octet-stream\\"}" "6869" @8 . drop drop
EOF
run_case raw "$TMP/raw.ws" 'len=2 hex=6869'

cat > "$TMP/parallel.ws" <<EOF
2 "GET" "http://127.0.0.1:$PORT/echo" "{}" "" @9 .
EOF
run_parallel_case parallel "$TMP/parallel.ws"

echo "http-parity-gate: PASS (@6/@7 + @8 + @9 status/body/header + redirect 302/Location parity)"
