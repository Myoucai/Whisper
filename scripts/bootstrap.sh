#!/bin/bash
# Official Whisper bootstrap: from the frozen C seed to a working,
# self-hosted native compiler — with gcc as the only dependency.
#
#   bash scripts/bootstrap.sh [install-dir]
#
# Stages:
#   1. gcc compiles whisperc/seed/compiler_seed.c  -> whisper0 (the seed)
#   2. whisper0 compiles whisperc/compiler.all.ws  -> gen1.c -> whisper1
#   3. whisper1 compiles whisperc/compiler.all.ws  -> gen2.c -> whisper2
#   4. fixed-point check: whisper2's output (gen3.c) == gen2.c
#   5. smoke: whisper2 compiles a stdlib program to C
#
# After this, 'whisper2 -c < file.ws > prog.c && gcc ...' compiles any
# Whisper source with zero Rust involvement.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-$ROOT/target/bootstrap}"
GCC="${GCC:-gcc}"
EXTRA_LIBS="$(case $(uname) in MINGW*|MSYS*|CYGWIN*) echo -lws2_32;; esac)"
mkdir -p "$OUT"
trap 'rm -rf "$OUT/.tmp"' EXIT
TMP="$OUT/.tmp"; mkdir -p "$TMP"

# shellcheck source=lib/same-bytes.sh
. "$(dirname "$0")/lib/same-bytes.sh"

step() { echo "[bootstrap] $*"; }

step "1/7 seed: gcc -> whisper0"
"$GCC" -O2 -o "$TMP/whisper0" "$ROOT/whisperc/seed/compiler_seed.c" -lm $EXTRA_LIBS

step "2/7 whisper0 compiles the Whisper-written compiler -> whisper1"
"$TMP/whisper0" -c < "$ROOT/whisperc/compiler.all.ws" > "$TMP/gen1.c" 2>/dev/null
"$GCC" -O2 -o "$TMP/whisper1" "$TMP/gen1.c" -I "$ROOT/whisperc" -lm $EXTRA_LIBS

step "3/7 whisper1 recompiles the compiler -> whisper2 (the toolchain)"
"$TMP/whisper1" -c < "$ROOT/whisperc/compiler.all.ws" > "$TMP/gen2.c" 2>/dev/null
"$GCC" -O2 -o "$OUT/whisper2" "$TMP/gen2.c" -I "$ROOT/whisperc" -lm $EXTRA_LIBS

step "4/7 fixed-point check (whisper2 output must equal whisper1's)"
"$OUT/whisper2" -c < "$ROOT/whisperc/compiler.all.ws" > "$TMP/gen3.c" 2>/dev/null
if ! same_bytes "$TMP/gen2.c" "$TMP/gen3.c"; then
    echo "[bootstrap] FIXED POINT BROKEN: gen2 != gen3" >&2
    exit 1
fi

step "5/7 smoke: whisper2 compiles a stdlib program"
printf 'import "std/math"\n5 sq .\npi floor f64toi64 .\n' > "$TMP/smoke.ws"
# Inline the stdlib import the way c2 does (the compiler itself drops
# import/export tokens, so plain concatenation is equivalent).
{ cat "$ROOT/stdlib/math.ws"; printf '5 sq .\npi floor f64toi64 .\n'; } > "$TMP/smoke-flat.ws"
"$OUT/whisper2" -c < "$TMP/smoke-flat.ws" > "$TMP/smoke.c" 2>/dev/null
"$GCC" -O2 -w -o "$TMP/smoke" "$TMP/smoke.c" -I "$ROOT/whisperc" -lm $EXTRA_LIBS
SMOKE_OUT=$("$TMP/smoke" 2>/dev/null | tr -d '\r')
if [ "$SMOKE_OUT" != "$(printf '25\n3')" ]; then
    echo "[bootstrap] smoke FAILED: got [$SMOKE_OUT]" >&2
    exit 1
fi

# 6) 用刚造出来的 whisper2 编出 `wd` —— 纯 Whisper 写的驱动，之后
#    `wd build/run` 一条命令就能编任何程序（不用再手工内联 import、调 gcc）。
#    本步把「零 Rust 工具链」推到「零 Rust 使用」。
step "6/7 wd: whisper2 compiles the Whisper-written driver"
sed '$d' "$ROOT/whisperc/compiler.all.ws" > "$TMP/wd-src.ws"
cat "$ROOT/whisperc/wd.ws" >> "$TMP/wd-src.ws"
"$OUT/whisper2" -c < "$TMP/wd-src.ws" > "$TMP/wd.c" 2>/dev/null
"$GCC" -O2 -w -o "$OUT/wd" "$TMP/wd.c" -I "$ROOT/whisperc" -lm $EXTRA_LIBS

step "7/7 smoke: wd builds and runs a program on its own"
printf '1 2 + .\n' > "$TMP/wd-smoke.ws"
WD_OUT=$(cd "$ROOT" && WHISPER_ROOT="$ROOT" "$OUT/wd" run "$TMP/wd-smoke.ws" "$TMP/wd-smoke" 2>/dev/null | tr -d '\r' | tail -1)
if [ "$WD_OUT" != "3" ]; then
    echo "[bootstrap] wd smoke FAILED: got [$WD_OUT]" >&2
    exit 1
fi

echo
echo "[bootstrap] OK — toolchain at $OUT/whisper2"
echo "  compile:  $OUT/whisper2 -c < program.ws > prog.c"
echo "  build:    gcc -O2 -o prog prog.c -I $ROOT/whisperc -lm"
echo "  driver:   $OUT/wd build|run program.ws out   （自带 import 内联）"
