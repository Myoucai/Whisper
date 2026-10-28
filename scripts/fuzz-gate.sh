#!/usr/bin/env bash
# libFuzzer gate for the native HTTP/URL parsers.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
CLANG="${CLANG:-clang}"
command -v "$CLANG" >/dev/null 2>&1 || { echo "fuzz-gate: clang not found" >&2; exit 1; }
OUT="$ROOT/target/fuzz-http"
"$CLANG" -fsanitize=fuzzer,address -g -O1 -I "$ROOT/whisperc" \
    "$ROOT/fuzz/http_fuzz.c" -o "$OUT" -lm -lpthread
RUNS="${FUZZ_RUNS:-20000}"
ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0}" "$OUT" \
    -runs="$RUNS" -max_len=4096 "$ROOT/fuzz/corpus/http" 2>&1 | tail -20
echo "fuzz-gate: PASS ($RUNS runs)"
