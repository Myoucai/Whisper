#!/usr/bin/env bash
# Gate for `wd c` (zero-Rust C emission).
#   1. emits the requested .c and no executable
#   2. byte-identical to `whisper bootstrap`
#   3. does not invoke cc (WHISPER_CC=/bin/false still succeeds)
#   4. `wd build` still produces/runs a native hello
#   5. usage advertises `c`
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
export PATH="/mingw64/bin:/usr/bin:$PATH"
WD="${WHISPER_WD:-$ROOT/target/wd/wd}"
[ -f "$WD" ] || WD="$WD.exe"
BIN="${WHISPER_BIN:-$ROOT/target/debug/whisper}"
[ -f "$BIN" ] || BIN="$BIN.exe"
[ -f "$WD" ] || { echo "wd-c-gate: missing $WD" >&2; exit 1; }
[ -f "$BIN" ] || { echo "wd-c-gate: missing $BIN" >&2; exit 1; }
TMP="$ROOT/target/wd-c-gate"
rm -rf "$TMP"; mkdir -p "$TMP"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf '  %-6s ✅ %s\n' "$1" "$2"; }
bad() { FAIL=$((FAIL+1)); printf '  %-6s ❌ %s\n' "$1" "$2"; }

SRC="docs/examples/hello.ws"

# C-1: wd c -> only .c, no executable
if WHISPER_ROOT="$ROOT" "$WD" c "$SRC" "$TMP/hello.c" >/dev/null 2>"$TMP/c.err" \
   && [ -s "$TMP/hello.c" ] && [ ! -e "$TMP/hello.c.exe" ] && [ ! -e "$TMP/hello.exe" ]; then
    ok C-1 "wd c 只产出 .c（无 exe）"
else
    bad C-1 "wd c 未产出预期的 .c"; tail -3 "$TMP/c.err"
fi

# C-2: byte-identical to bootstrap
"$BIN" bootstrap "$SRC" > "$TMP/bootstrap.c" 2>/dev/null
H1=$(sha256sum "$TMP/hello.c" 2>/dev/null | cut -d' ' -f1)
H2=$(sha256sum "$TMP/bootstrap.c" 2>/dev/null | cut -d' ' -f1)
if [ -s "$TMP/hello.c" ] && [ "$H1" = "$H2" ]; then
    ok C-2 "wd c 与 whisper bootstrap 逐字节一致"
else
    bad C-2 "wd c 与 bootstrap 输出不一致 ($H1 vs $H2)"
fi

# C-3: no cc invocation (c branch runs before cache/cc)
if WHISPER_ROOT="$ROOT" WHISPER_CC=/bin/false "$WD" c "$SRC" "$TMP/no-cc.c" >/dev/null 2>"$TMP/no-cc.err" \
   && [ -s "$TMP/no-cc.c" ]; then
    ok C-3 "不调用 cc（WHISPER_CC=/bin/false 仍成功）"
else
    bad C-3 "wd c 仍依赖 cc"; tail -3 "$TMP/no-cc.err"
fi

# C-4: build still works
if WHISPER_ROOT="$ROOT" "$WD" build "$SRC" "$TMP/native" >/dev/null 2>"$TMP/build.err" \
   && [ -f "$TMP/native.exe" ] && "$TMP/native.exe" | grep -q 'Hello, World!'; then
    ok C-4 "wd build 仍编译并运行 hello"
else
    bad C-4 "wd build 回归"; tail -3 "$TMP/build.err"
fi

# C-5: usage mentions c
if "$WD" 2>&1 | grep -q 'build|run|c'; then
    ok C-5 "usage 列出 c"
else
    bad C-5 "usage 未列出 c"
fi

echo "wd-c-gate: $PASS/$((PASS+FAIL))"
[ "$FAIL" -eq 0 ]
