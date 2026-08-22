#!/bin/bash
# Syntax freeze gate: every conformance program must produce identical
# output through BOTH implementations — the Rust VM ('whisper run') and
# the self-hosted c2 chain (compiler.all.ws on the VM -> C -> gcc).
#
# Normalizations for known, documented display divergences:
#   - CRLF stripped
#   - surrounding double quotes stripped per line (VM prints strings
#     quoted, the C chain prints them raw — the self-hosted driver
#     depends on raw output)
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${BIN:-$ROOT/target/debug/whisper.exe}"
CORPUS="$ROOT/tests/conformance"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0

norm() { tr -d '\r' | sed -E 's/^"(.*)"$/\1/' | grep -v '^$' | paste -sd'|' -; }

for f in "$CORPUS"/*.ws; do
    name="$(basename "$f" .ws)"
    expected="$(head -1 "$f" | sed 's|^// expect: ||')"

    vm_out=$(timeout 60 "$BIN" run "$f" 2>/dev/null | norm)
    c2_out=$(timeout 180 "$BIN" build "$f" -t c2 -o "$TMP/t.exe" 2>/dev/null >/dev/null \
        && "$TMP/t.exe" 2>/dev/null | norm)

    if [ "$vm_out" = "$expected" ] && [ "$c2_out" = "$expected" ]; then
        pass=$((pass+1))
    else
        fail=$((fail+1))
        echo "FAIL $name:"
        echo "  expected: [$expected]"
        echo "  vm:       [$vm_out]"
        echo "  c2:       [$c2_out]"
    fi
done

echo "=== syntax gate: PASS=$pass FAIL=$fail ==="
[ "$fail" -eq 0 ]
