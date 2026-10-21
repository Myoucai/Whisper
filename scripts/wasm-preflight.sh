#!/usr/bin/env bash
# WASM/WASI 目标的前置检查。只报告，不构建、不改仓库。
#
# 约束：**运行时必须零 Rust**。因此只接受 C 实现的 WASM 运行时：
#   - WAMR `iwasm`（推荐，官方 Windows/Linux 预编译）
#   - wasm3（C，命令名 wasm3）
#   - wasm2c + 宿主 C shim（WABT，AOT 到 C，无外部 runtime）
# 不接受 wasmtime/wasmer（Rust 实现）。
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

echo "== C → wasm 编译器（零 Rust）=="
COMPILER=""
if command -v zig >/dev/null 2>&1; then
    COMPILER="zig"
    ok zig "$(zig version 2>/dev/null) · 自带 wasi-libc，零 Rust"
elif command -v clang >/dev/null 2>&1; then
    ver="$(clang --version 2>/dev/null | head -1)"
    if clang --print-targets 2>/dev/null | grep -q 'wasm32'; then
        COMPILER="clang"
        ok clang "$ver · wasm32 target 可用（仍需 wasi-sysroot）"
    else
        warn clang "$ver · 未列出 wasm32 target（需要 wasi-sdk）"
    fi
else
    warn compiler "未找到 zig 或 wasi-sdk clang；至少需要其中一个"
fi

echo "== wasi-sysroot =="
if [ "$COMPILER" = "zig" ]; then
    ok wasi-sysroot "由 zig 自带，无需单独安装"
else
    SYSROOT=""
    for d in "${WASI_SDK_PATH:-}" "${WASI_SYSROOT:-}" /opt/wasi-sdk /usr/local/wasi-sdk; do
        [ -n "$d" ] || continue
        if [ -d "$d/share/wasi-sysroot" ]; then SYSROOT="$d/share/wasi-sysroot"; break; fi
        if [ -d "$d" ] && [ -f "$d/lib/wasi/crt1.o" ]; then SYSROOT="$d"; break; fi
    done
    if [ -n "$SYSROOT" ]; then
        ok wasi-sysroot "$SYSROOT"
    else
        warn wasi-sysroot "未找到；设置 WASI_SDK_PATH 或 WASI_SYSROOT（或改用 zig）"
    fi
fi

echo "== 零 Rust WASM runtime =="
found=""
if [ -n "${WASM_RUNTIME:-}" ] && [ -x "${WASM_RUNTIME}" ]; then
    found="${WASM_RUNTIME}: $("${WASM_RUNTIME}" --version 2>/dev/null | head -1)"
fi
if [ -z "$found" ]; then
    for r in iwasm wasm3; do
        if command -v "$r" >/dev/null 2>&1; then
            found="$r: $("$r" --version 2>/dev/null | head -1)"
            break
        fi
    done
fi
if [ -n "$found" ]; then
    ok runtime "$found（C 实现）"
elif command -v wasm2c >/dev/null 2>&1; then
    ok runtime "wasm2c（WABT，AOT 到 C；需要宿主 shim）"
else
    warn runtime "未找到 iwasm/wasm3/wasm2c（wasmtime/wasmer 是 Rust，不采用）"
fi

echo
if [ "$missing" -eq 0 ]; then
    echo "wasm preflight: OK（零 Rust 工具链）"
else
    echo "wasm preflight: $missing 个前置缺失（见 docs/wasm-wasi-plan.md W-1）"
fi
if [ "$STRICT" -eq 1 ] && [ "$missing" -ne 0 ]; then
    exit 1
fi
exit 0
