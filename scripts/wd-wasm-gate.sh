#!/usr/bin/env bash
# Gate for `wd wasm` (zero-Rust C -> wasm). Needs Zig + WAMR.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
export PATH="/mingw64/bin:/usr/bin:/c/msys64/clang64/bin:$PATH"
WD="${WHISPER_WD:-$ROOT/target/wd/wd}"
[ -f "$WD" ] || WD="$WD.exe"
[ -f "$WD" ] || { echo "wd-wasm-gate: missing $WD" >&2; exit 1; }
RT="${WASM_RUNTIME:-}"
if [ -z "$RT" ]; then
    for r in "$ROOT/target/toolchain/wamr/iwasm.exe" "$ROOT/target/toolchain/wamr/iwasm" iwasm; do
        if [ -x "$r" ] || command -v "$r" >/dev/null 2>&1; then RT="$r"; break; fi
    done
fi
[ -n "$RT" ] || { echo "wd-wasm-gate: no iwasm" >&2; exit 1; }
TMP="$ROOT/target/wd-wasm-gate"
rm -rf "$TMP"; mkdir -p "$TMP"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf '  %-6s ✅ %s\n' "$1" "$2"; }
bad() { FAIL=$((FAIL+1)); printf '  %-6s ❌ %s\n' "$1" "$2"; }

# W-1: hello
if WHISPER_ROOT="$ROOT" "$WD" wasm docs/examples/hello.ws "$TMP/hello.wasm" >/dev/null 2>"$TMP/hello.err" \
   && "$RT" "$TMP/hello.wasm" >"$TMP/hello.out" 2>/dev/null \
   && grep -q 'Hello, World!' "$TMP/hello.out"; then
    ok W-1 "wd wasm hello"
else
    bad W-1 "wd wasm hello"; tail -3 "$TMP/hello.err"
fi

# W-2: HOF (top-level _qN, no lifter)
if WHISPER_ROOT="$ROOT" "$WD" wasm tests/conformance/14_map_fold.ws "$TMP/hof.wasm" >/dev/null 2>"$TMP/hof.err" \
   && "$RT" "$TMP/hof.wasm" >"$TMP/hof.out" 2>/dev/null \
   && [ "$(tr -d '\r' < "$TMP/hof.out" | tr '\n' '|')" = "9|15|55|" ]; then
    ok W-2 "wd wasm HOF 9/15/55"
else
    bad W-2 "wd wasm HOF"; tail -3 "$TMP/hof.err"; cat "$TMP/hof.out" 2>/dev/null
fi

# W-3: no nested functions / old bridge
if ! grep -q 'auto void _qb' "$TMP/hof.wasm.c" 2>/dev/null; then
    ok W-3 "C 无 auto void _qb"
else
    bad W-3 "C 出现嵌套函数"
fi

# W-4: usage
if "$WD" 2>&1 | grep -q 'build|run|c|wasm'; then
    ok W-4 "usage 列出 wasm"
else
    bad W-4 "usage 未列出 wasm"
fi

echo "wd-wasm-gate: $PASS/$((PASS+FAIL))"
[ "$FAIL" -eq 0 ]
