#!/usr/bin/env bash
# Dependency-free mutation smoke test for the WASM import allowlist.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
PY="${PYTHON:-}"
if [ -z "$PY" ]; then
    for p in python3 python; do command -v "$p" >/dev/null 2>&1 && { PY="$p"; break; }; done
fi
[ -n "$PY" ] || { echo "wasm-fuzz-gate: python required" >&2; exit 1; }
WASM_FUZZ_ITERS="${WASM_FUZZ_ITERS:-300}" "$PY" fuzz/wasm_import_mutate.py
