#!/usr/bin/env bash
# Build a Whisper source file to a WASI module, with a zero-Rust *runtime*.
#
#   bash scripts/build-wasm.sh hello.ws [hello.wasm]
#
# Compiler: Zig `cc` (bundled wasi-libc) or wasi-sdk clang.
# Runtime:  WAMR iwasm / wasm3 / wasm2c (C only; wasmtime/wasmer rejected).
# C emission: prefers `wd c` (zero-Rust emitter, target/wd/wd); falls back to
# `whisper bootstrap` (Rust CLI) only when wd is not built. GNU nested quote
# bodies are lifted by scripts/lift-nested.py (see W2.5 in the plan).
#
# Environment:
#   WASM_CC=zig|clang   force compiler (default: zig if present, else clang)
#   WASI_SDK_PATH=...   wasi-sdk root for clang's sysroot
#   WASM_RUNTIME=...    iwasm/wasm3 path for --run
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="${1:-}"
OUT="${2:-}"
if [ -z "$SRC" ]; then
    echo "usage: bash scripts/build-wasm.sh <src.ws> [out.wasm]" >&2
    exit 2
fi
[ -f "$SRC" ] || { echo "build-wasm: no such source: $SRC" >&2; exit 1; }
if [ -z "$OUT" ]; then
    mkdir -p "$ROOT/target/wasm"
    OUT="$ROOT/target/wasm/$(basename "${SRC%.ws}").wasm"
fi
mkdir -p "$(dirname "$OUT")"
BUILD="$ROOT/target/wasm-build"
mkdir -p "$BUILD"
C="$BUILD/$(basename "${SRC%.ws}").c"
LIFT="$BUILD/$(basename "${SRC%.ws}").lift.c"

BIN="${WHISPER_BIN:-$ROOT/target/debug/whisper}"
[ -f "$BIN" ] || BIN="$BIN.exe"
WD="${WHISPER_WD:-$ROOT/target/wd/wd}"
[ -f "$WD" ] || WD="$WD.exe"

# C emission. Prefer `wd c` (zero-Rust emitter); fall back to the Rust CLI
# bootstrap path only when wd is not built. Keep stdout raw (PowerShell
# redirection would add a UTF-16 BOM).
if [ -f "$WD" ]; then
    if ! WHISPER_ROOT="$ROOT" "$WD" c "$SRC" "$C" 2> "$BUILD/bootstrap.err"; then
        echo "build-wasm: wd c failed (see $BUILD/bootstrap.err)" >&2
        exit 1
    fi
else
    [ -f "$BIN" ] || { echo "build-wasm: missing $BIN and $WD" >&2; exit 1; }
    "$BIN" bootstrap "$SRC" > "$C" 2> "$BUILD/bootstrap.err"
fi
if [ ! -s "$C" ]; then
    echo "build-wasm: bootstrap produced no C (see $BUILD/bootstrap.err)" >&2
    exit 1
fi

# clang/Zig do not support GNU nested functions. cgen still emits them for
# quote bodies, so lift them to file-scope functions before compiling.
PY="${PYTHON:-python}"
command -v "$PY" >/dev/null 2>&1 || { echo "build-wasm: python not found (needed to lift nested functions)" >&2; exit 1; }
"$PY" "$ROOT/scripts/lift-nested.py" "$C" "$LIFT"
C="$LIFT"

CC="${WASM_CC:-}"
if [ -z "$CC" ]; then
    if command -v zig >/dev/null 2>&1; then CC="zig"; else CC="clang"; fi
fi

echo "[build-wasm] C: $C"
case "$CC" in
    zig)
        command -v zig >/dev/null 2>&1 || { echo "build-wasm: zig not found" >&2; exit 1; }
        zig cc -target wasm32-wasi -O2 \
            -D_WASI_EMULATED_MMAN -D_WASI_EMULATED_SIGNAL \
            -I "$ROOT/whisperc" "$C" -o "$OUT" \
            -lwasi-emulated-mman -lwasi-emulated-signal
        ;;
    clang)
        command -v clang >/dev/null 2>&1 || { echo "build-wasm: clang not found" >&2; exit 1; }
        SYSROOT="${WASI_SYSROOT:-}"
        if [ -z "$SYSROOT" ] && [ -n "${WASI_SDK_PATH:-}" ]; then
            SYSROOT="$WASI_SDK_PATH/share/wasi-sysroot"
        fi
        [ -n "$SYSROOT" ] || { echo "build-wasm: set WASI_SDK_PATH/WASI_SYSROOT for clang" >&2; exit 1; }
        clang --target=wasm32-wasip1 --sysroot="$SYSROOT" -O2 \
            -D_WASI_EMULATED_MMAN -D_WASI_EMULATED_SIGNAL \
            -I "$ROOT/whisperc" "$C" -o "$OUT" \
            -lwasi-emulated-mman -lwasi-emulated-signal
        ;;
    *)
        echo "build-wasm: unsupported WASM_CC=$CC" >&2; exit 1
        ;;
esac

echo "[build-wasm] wasm: $OUT ($(wc -c < "$OUT") bytes, compiler=$CC)"
if [ "${3:-}" = "--run" ] || [ "${WASM_RUN:-0}" = "1" ]; then
    RT="${WASM_RUNTIME:-}"
    if [ -z "$RT" ]; then
        for r in iwasm wasm3; do
            if command -v "$r" >/dev/null 2>&1; then RT="$r"; break; fi
        done
    fi
    [ -n "$RT" ] || { echo "build-wasm: no iwasm/wasm3 runtime for --run" >&2; exit 1; }
    echo "[build-wasm] run: $RT $OUT"
    "$RT" "$OUT"
fi
