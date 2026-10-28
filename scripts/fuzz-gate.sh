#!/usr/bin/env bash
# libFuzzer gate for the native HTTP/URL parsers.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
CLANG="${CLANG:-clang}"
command -v "$CLANG" >/dev/null 2>&1 || { echo "fuzz-gate: clang not found" >&2; exit 1; }
OUT="$ROOT/target/fuzz-http"
BUILD_LOG="$ROOT/target/fuzz-build.log"
RUN_LOG="$ROOT/target/fuzz-run.log"
if ! "$CLANG" -fsanitize=fuzzer,address -g -O1 -I "$ROOT/whisperc" \
    "$ROOT/fuzz/http_fuzz.c" -o "$OUT" -lm -lpthread >"$BUILD_LOG" 2>&1; then
    echo "fuzz-gate: build failed" >&2
    tail -20 "$BUILD_LOG" >&2
    echo "::error::fuzz build failed: $(tail -3 "$BUILD_LOG" | tr '\n' ' ')" >&2
    exit 1
fi
RUNS="${FUZZ_RUNS:-20000}"
if ! ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0}" "$OUT" \
    -runs="$RUNS" -max_len=4096 "$ROOT/fuzz/corpus/http" >"$RUN_LOG" 2>&1; then
    echo "fuzz-gate: fuzzer failed" >&2
    tail -30 "$RUN_LOG" >&2
    echo "::error::fuzz run failed: $(tail -5 "$RUN_LOG" | tr '\n' ' ')" >&2
    exit 1
fi
tail -5 "$RUN_LOG"
echo "fuzz-gate: PASS ($RUNS runs)"
