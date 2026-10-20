#!/usr/bin/env bash
# WASM/WASI 目标的前置检查。只报告，不构建、不改仓库。
#
#   bash scripts/wasm-preflight.sh          # 缺失项只警告，退出 0
#   bash scripts/wasm-preflight.sh --strict # 缺失项退出 1（CI 用）
#
# 见 docs/wasm-wasi-plan.md 的 W-1。
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STRICT=0
[ "${1:-}" = "--strict" ] && STRICT=1

missing=0
ok()   { printf '  %-16s ✅ %s\n' "$1" "$2"; }
warn() { printf '  %-16s ⚠️  %s\n' "$1" "$2"; missing=$((missing+1)); }

echo "== clang / wasm target =="
if command -v clang >/dev/null 2>&1; then
    ver="$(clang --version 2>/dev/null | head -1)"
    if clang --print-targets 2>/dev/null | grep -q 'wasm32'; then
        ok clang "$ver · wasm32 target 可用"
    else
        warn clang "$ver · 未列出 wasm32 target（可能需要 wasi-sdk 的 clang）"
    fi
else
    warn clang "未找到；请安装 wasi-sdk 或带 wasm32 target 的 clang"
fi

echo "== wasi-sysroot =="
SYSROOT=""
for d in "${WASI_SDK_PATH:-}" "${WASI_SYSROOT:-}" /opt/wasi-sdk /usr/local/wasi-sdk; do
    [ -n "$d" ] || continue
    if [ -d "$d/share/wasi-sysroot" ]; then SYSROOT="$d/share/wasi-sysroot"; break; fi
    if [ -d "$d" ] && [ -f "$d/lib/wasi/crt1.o" ]; then SYSROOT="$d"; break; fi
done
if [ -n "$SYSROOT" ]; then
    ok wasi-sysroot "$SYSROOT"
else
    warn wasi-sysroot "未找到；设置 WASI_SDK_PATH 或 WASI_SYSROOT（wasi-sdk 的 sysroot）"
fi

echo "== WASM runtime =="
found=""
for r in wasmtime iwasm wasmer wasm3; do
    if command -v "$r" >/dev/null 2>&1; then
        found="$r: $("$r" --version 2>/dev/null | head -1)"
        break
    fi
done
if [ -n "$found" ]; then
    ok runtime "$found"
else
    warn runtime "未找到 wasmtime/iwasm/wasmer/wasm3 之一"
fi

echo
if [ "$missing" -eq 0 ]; then
    echo "wasm preflight: OK"
else
    echo "wasm preflight: $missing 个前置缺失（见 docs/wasm-wasi-plan.md W-1）"
fi
if [ "$STRICT" -eq 1 ] && [ "$missing" -ne 0 ]; then
    exit 1
fi
exit 0
