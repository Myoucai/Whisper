#!/bin/bash
# Rebuild the self-hosting chain from the frozen C seed and verify the
# fixed point: the compiler compiled by whisper1, when run on
# compiler.all.ws again, must produce byte-identical output (gen2 == gen3).
#
# Usage: scripts/verify-fixedpoint.sh   (requires gcc)
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXTRA_LIBS="$(case $(uname) in MINGW*|MSYS*|CYGWIN*) echo -lws2_32;; esac)"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "[fixed-point] building seed compiler (whisperc/seed/compiler_seed.c)"
gcc -O2 -o "$TMP/whisper0" "$ROOT/whisperc/seed/compiler_seed.c" -lm $EXTRA_LIBS 2>/dev/null

echo "[fixed-point] gen1.c = whisper0 compiling compiler.all.ws"
"$TMP/whisper0" -c < "$ROOT/whisperc/compiler.all.ws" > "$TMP/gen1.c" 2>/dev/null
gcc -O2 -fno-strict-aliasing -o "$TMP/whisper1" "$TMP/gen1.c" -I "$ROOT/whisperc" -lm $EXTRA_LIBS 2>/dev/null

echo "[fixed-point] gen2.c = whisper1 compiling compiler.all.ws"
"$TMP/whisper1" -c < "$ROOT/whisperc/compiler.all.ws" > "$TMP/gen2.c" 2>/dev/null
gcc -O2 -fno-strict-aliasing -o "$TMP/whisper2" "$TMP/gen2.c" -I "$ROOT/whisperc" -lm $EXTRA_LIBS 2>/dev/null

echo "[fixed-point] gen3.c = whisper2 compiling compiler.all.ws"
"$TMP/whisper2" -c < "$ROOT/whisperc/compiler.all.ws" > "$TMP/gen3.c" 2>/dev/null

if cmp -s "$TMP/gen2.c" "$TMP/gen3.c"; then
    echo "[fixed-point] OK: gen2 == gen3 (self-hosting fixed point holds)"
    exit 0
else
    echo "[fixed-point] BROKEN: gen2 != gen3"
    diff "$TMP/gen2.c" "$TMP/gen3.c" | head -20 || true
    exit 1
fi
