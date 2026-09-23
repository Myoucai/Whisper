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

# Byte-identity check. The helper lives in scripts/lib/same-bytes.sh because
# bootstrap.sh needs the same rule — and because getting it wrong is not
# cosmetic (`if ! cmp -s` with a missing cmp evaluates to true).
# shellcheck source=lib/same-bytes.sh
. "$(dirname "$0")/lib/same-bytes.sh"

echo "[fixed-point] building seed compiler (whisperc/seed/compiler_seed.c)"
gcc -O2 -o "$TMP/whisper0" "$ROOT/whisperc/seed/compiler_seed.c" -lm $EXTRA_LIBS 2>/dev/null

echo "[fixed-point] gen1.c = whisper0 compiling compiler.all.ws"
"$TMP/whisper0" -c < "$ROOT/whisperc/compiler.all.ws" > "$TMP/gen1.c" 2>/dev/null
gcc -O2 -o "$TMP/whisper1" "$TMP/gen1.c" -I "$ROOT/whisperc" -lm $EXTRA_LIBS 2>/dev/null

echo "[fixed-point] gen2.c = whisper1 compiling compiler.all.ws"
"$TMP/whisper1" -c < "$ROOT/whisperc/compiler.all.ws" > "$TMP/gen2.c" 2>/dev/null
gcc -O2 -o "$TMP/whisper2" "$TMP/gen2.c" -I "$ROOT/whisperc" -lm $EXTRA_LIBS 2>/dev/null

echo "[fixed-point] gen3.c = whisper2 compiling compiler.all.ws"
"$TMP/whisper2" -c < "$ROOT/whisperc/compiler.all.ws" > "$TMP/gen3.c" 2>/dev/null

if same_bytes "$TMP/gen2.c" "$TMP/gen3.c"; then
    echo "[fixed-point] OK: gen2 == gen3 (self-hosting fixed point holds)"
    exit 0
else
    echo "[fixed-point] BROKEN: gen2 != gen3"
    if command -v diff >/dev/null 2>&1; then
        diff "$TMP/gen2.c" "$TMP/gen3.c" | head -20 || true
    else
        echo "[fixed-point] (diff unavailable; sizes: $(wc -c < "$TMP/gen2.c") vs $(wc -c < "$TMP/gen3.c"))"
    fi
    exit 1
fi
