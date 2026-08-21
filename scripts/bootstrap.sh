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
mkdir -p "$OUT"
trap 'rm -rf "$OUT/.tmp"' EXIT
TMP="$OUT/.tmp"; mkdir -p "$TMP"

step() { echo "[bootstrap] $*"; }

step "1/5 seed: gcc -> whisper0"
"$GCC" -O2 -o "$TMP/whisper0" "$ROOT/whisperc/seed/compiler_seed.c" -lm

step "2/5 whisper0 compiles the Whisper-written compiler -> whisper1"
"$TMP/whisper0" -c < "$ROOT/whisperc/compiler.all.ws" > "$TMP/gen1.c" 2>/dev/null
"$GCC" -O2 -fno-strict-aliasing -o "$TMP/whisper1" "$TMP/gen1.c" -I "$ROOT/whisperc" -lm

step "3/5 whisper1 recompiles the compiler -> whisper2 (the toolchain)"
"$TMP/whisper1" -c < "$ROOT/whisperc/compiler.all.ws" > "$TMP/gen2.c" 2>/dev/null
"$GCC" -O2 -fno-strict-aliasing -o "$OUT/whisper2" "$TMP/gen2.c" -I "$ROOT/whisperc" -lm

step "4/5 fixed-point check (whisper2 output must equal whisper1's)"
"$OUT/whisper2" -c < "$ROOT/whisperc/compiler.all.ws" > "$TMP/gen3.c" 2>/dev/null
if ! cmp -s "$TMP/gen2.c" "$TMP/gen3.c"; then
    echo "[bootstrap] FIXED POINT BROKEN: gen2 != gen3" >&2
    exit 1
fi

step "5/5 smoke: whisper2 compiles a stdlib program"
printf 'import "std/math"\n5 sq .\npi floor f64toi64 .\n' > "$TMP/smoke.ws"
# Inline the stdlib import the way c2 does (the compiler itself drops
# import/export tokens, so plain concatenation is equivalent).
{ cat "$ROOT/stdlib/math.ws"; printf '5 sq .\npi floor f64toi64 .\n'; } > "$TMP/smoke-flat.ws"
"$OUT/whisper2" -c < "$TMP/smoke-flat.ws" > "$TMP/smoke.c" 2>/dev/null
"$GCC" -O2 -fno-strict-aliasing -w -o "$TMP/smoke" "$TMP/smoke.c" -I "$ROOT/whisperc" -lm
SMOKE_OUT=$("$TMP/smoke" 2>/dev/null | tr -d '\r')
if [ "$SMOKE_OUT" != "$(printf '25\n3')" ]; then
    echo "[bootstrap] smoke FAILED: got [$SMOKE_OUT]" >&2
    exit 1
fi

echo
echo "[bootstrap] OK — toolchain at $OUT/whisper2"
echo "  compile:  $OUT/whisper2 -c < program.ws > prog.c"
echo "  build:    gcc -O2 -fno-strict-aliasing -o prog prog.c -I $ROOT/whisperc -lm"
