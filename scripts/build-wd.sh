#!/usr/bin/env bash
# 构建 wd：零 Rust —— 冻结种子 + gcc 而已。
#
#   1. gcc 编种子                 → whisper0
#   2. whisper0 编编译器源码       → whisper1
#   3. whisper1 编「编译器 + wd」  → wd
#
# 用法：build-wd.sh [--fast] [out-dir]
#   --fast 仅用于 CI 测试工具：用 Rust CLI bootstrap 发射 C，再 gcc；
#          生产/零 Rust 验证仍走默认三段链。
#
# 与 verify-fixedpoint.sh 用同一条链，只是最后一步把 wd.ws 接在
# compiler.all.ws 后面（去掉其中末尾那行 `driver` 调用，否则它会先去读
# stdin 把整个程序吃掉）。
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FAST=0
if [ "${1:-}" = "--fast" ]; then FAST=1; shift; fi
OUT="${1:-$ROOT/target/wd}"
GCC="${GCC:-gcc}"
EXTRA_LIBS="$(case $(uname) in MINGW*|MSYS*|CYGWIN*) echo -lws2_32;; esac)"
mkdir -p "$OUT" "$OUT/.tmp"
TMP="$OUT/.tmp"

# CI fast path: use the Rust CLI only to emit C for the test tool.  The
# production zero-Rust path below remains the default; the runtime is still C.
if [ "$FAST" = 1 ]; then
    CLI="${WHISPER_CLI:-$ROOT/target/debug/whisper}"
    [ -f "$CLI" ] || CLI="$CLI.exe"
    [ -f "$CLI" ] || { echo "[wd] fast: missing $CLI" >&2; exit 1; }
    echo "[wd] fast: whisper bootstrap -> C -> gcc" >&2
    sed '$d' "$ROOT/whisperc/compiler.all.ws" > "$TMP/wd-src.ws"
    cat "$ROOT/whisperc/wd.ws" >> "$TMP/wd-src.ws"
    "$CLI" bootstrap "$TMP/wd-src.ws" > "$TMP/wd.c" 2>/dev/null
    "$GCC" -O1 -w -o "$OUT/wd" "$TMP/wd.c" -I "$ROOT/whisperc" -lm $EXTRA_LIBS
    rm -rf "$TMP"
    echo "[wd] OK (fast) -> $OUT/wd" >&2
    exit 0
fi

echo "[wd] 1/3 seed (gcc)" >&2
"$GCC" -O2 -o "$TMP/whisper0" "$ROOT/whisperc/seed/compiler_seed.c" -lm $EXTRA_LIBS

echo "[wd] 2/3 whisper0 -> whisper1" >&2
"$TMP/whisper0" -c < "$ROOT/whisperc/compiler.all.ws" > "$TMP/gen1.c" 2>/dev/null
"$GCC" -O2 -o "$TMP/whisper1" "$TMP/gen1.c" -I "$ROOT/whisperc" -lm $EXTRA_LIBS

echo "[wd] 3/3 whisper1 compiles compiler+wd -> wd" >&2
# 去掉 compiler.all.ws 末尾的 `driver` 调用（它读 stdin），再接上 wd.ws
sed '$d' "$ROOT/whisperc/compiler.all.ws" > "$TMP/wd-src.ws"
cat "$ROOT/whisperc/wd.ws" >> "$TMP/wd-src.ws"
"$TMP/whisper1" -c < "$TMP/wd-src.ws" > "$TMP/wd.c" 2>/dev/null
# wd is a build tool, not a hot path; -O1 keeps gcc memory/time down on CI.
"$GCC" -O1 -w -o "$OUT/wd" "$TMP/wd.c" -I "$ROOT/whisperc" -lm $EXTRA_LIBS

rm -rf "$TMP"
echo "[wd] OK -> $OUT/wd" >&2
