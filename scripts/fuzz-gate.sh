#!/usr/bin/env bash
# libFuzzer gate for the native HTTP/URL parsers.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
mkdir -p "$ROOT/target"
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
ARGS=(-max_len=4096)
if [ -n "${FUZZ_MAX_TIME:-}" ]; then
    ARGS+=("-max_total_time=$FUZZ_MAX_TIME")
else
    ARGS+=("-runs=$RUNS")
fi
if ! ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0}" "$OUT" \
    "${ARGS[@]}" "$ROOT/fuzz/corpus/http" >"$RUN_LOG" 2>&1; then
    echo "fuzz-gate: fuzzer failed" >&2
    tail -30 "$RUN_LOG" >&2
    echo "::error::fuzz run failed: $(tail -5 "$RUN_LOG" | tr '\n' ' ')" >&2
    exit 1
fi
tail -5 "$RUN_LOG"
if [ -n "${FUZZ_MAX_TIME:-}" ]; then
    echo "fuzz-gate: PASS (max_time=${FUZZ_MAX_TIME}s)"
else
    echo "fuzz-gate: PASS ($RUNS runs)"
fi
