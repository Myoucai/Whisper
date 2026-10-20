#!/usr/bin/env bash
# WASI negative/positive security gate (W-5/W-6/W-7 subset).
#
# Requires: target/wd/wd (wd c), Zig, WAMR iwasm.
#   bash scripts/wasm-security-gate.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
export PATH="/mingw64/bin:/usr/bin:/c/msys64/clang64/bin:$PATH"
RT="${WASM_RUNTIME:-$ROOT/target/toolchain/wamr/iwasm.exe}"
[ -f "$RT" ] || { echo "wasm-security-gate: missing $RT" >&2; exit 1; }
TMP="$ROOT/target/wasm-security"
rm -rf "$TMP"; mkdir -p "$TMP/data"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf '  %-6s ✅ %s\n' "$1" "$2"; }
bad() { FAIL=$((FAIL+1)); printf '  %-6s ❌ %s\n' "$1" "$2"; }
build() { bash scripts/build-wasm.sh "$1" "$2" >/dev/null 2>"$TMP/build.err"; }

# S-1: no preopen -> file operations denied (write -1, read 0)
if build tests/security/wasm_file.ws "$TMP/file.wasm" \
   && "$RT" "$TMP/file.wasm" >"$TMP/file-no-dir.out" 2>"$TMP/file-no-dir.err" \
   && [ "$(tr -d '\r' < "$TMP/file-no-dir.out" | tr '\n' '|')" = "-1|0|" ]; then
    ok S-1 "无 preopen：write=-1 / read=0"
else
    bad S-1 "无 preopen 未拒绝"; cat "$TMP/file-no-dir.out" 2>/dev/null
fi

HOSTDIR="$(cd "$TMP/data" && pwd -W)"
# S-2: preopen -> exact confined access
if build tests/security/wasm_file.ws "$TMP/file2.wasm" \
   && "$RT" --map-dir="/data::$HOSTDIR" "$TMP/file2.wasm" >"$TMP/file-dir.out" 2>"$TMP/file-dir.err" \
   && [ "$(tr -d '\r' < "$TMP/file-dir.out" | tr '\n' '|')" = "10|10|" ]; then
    ok S-2 "preopen /data：write=10 / read=10"
else
    bad S-2 "preopen 读写失败"; cat "$TMP/file-dir.out" 2>/dev/null
fi

# S-3: env not passed -> empty
if build tests/security/wasm_env.ws "$TMP/env.wasm" \
   && ( cd "$TMP" && "$RT" env.wasm ) >"$TMP/env-none.out" 2>/dev/null \
   && [ -z "$(tr -d '\r\n' < "$TMP/env-none.out")" ]; then
    ok S-3 "未 --env：getenv 为空"
else
    bad S-3 "未 --env 仍读到值"
fi

# S-4: explicit env -> visible
# Use a temp file, not a pipe: WAMR on Windows drops guest stdout when it is
# a pipe (this is a runtime quirk, not a Whisper output difference).
if ( cd "$TMP" && "$RT" --env=WHISPER_TEST=ok env.wasm ) >"$TMP/env-ok.out" 2>/dev/null \
   && [ "$(tr -d '\r\n' < "$TMP/env-ok.out")" = "ok" ]; then
    ok S-4 "--env=WHISPER_TEST=ok：ok"
else
    bad S-4 "--env 未生效（got: $(tr -d '\r\n' < "$TMP/env-ok.out" 2>/dev/null)）"
fi

# S-5/S-6/S-7: explicit unsupported, non-zero
check_unsupported() {
    id="$1"; src="$2"; needle="$3"
    if build "$src" "$TMP/$id.wasm" \
       && "$RT" "$TMP/$id.wasm" >"$TMP/$id.out" 2>"$TMP/$id.err"; then
        bad "$id" "应失败却成功"
    elif grep -q "$needle" "$TMP/$id.out" "$TMP/$id.err" 2>/dev/null; then
        ok "$id" "显式 unsupported：$needle"
    else
        bad "$id" "失败信息不含：$needle"; cat "$TMP/$id.out" "$TMP/$id.err" 2>/dev/null
    fi
}
check_unsupported S-5 tests/security/wasm_exec.ws "capability @5 (exec) is unsupported"
check_unsupported S-6 tests/security/wasm_http.ws "capability @2 (http-get) is unsupported"
check_unsupported S-7 tests/security/wasm_try.ws "try is unsupported on wasm profile"

# S-9: path escape via .. and absolute path must be denied
printf 'SECRET\n' > "$TMP/secret.txt"
if build tests/security/wasm_path_escape.ws "$TMP/path.wasm" \
   && "$RT" --map-dir="/data::$HOSTDIR" "$TMP/path.wasm" >"$TMP/path.out" 2>"$TMP/path.err" \
   && [ "$(tr -d '\r' < "$TMP/path.out" | tr '\n' '|')" = "0|0|" ]; then
    ok S-9 "preopen 外的 ../ 与绝对路径均不可读"
else
    bad S-9 "路径逃逸未被拒绝"; cat "$TMP/path.out" 2>/dev/null
fi

# S-10/S-11: preopen directory ops; denied without preopen
if build tests/security/wasm_dir.ws "$TMP/dir.wasm" \
   && "$RT" --map-dir="/data::$HOSTDIR" "$TMP/dir.wasm" >"$TMP/dir.out" 2>"$TMP/dir.err" \
   && [ "$(tr -d '\r' < "$TMP/dir.out" | tr '\n' '|')" = "0|2|1|" ]; then
    ok S-10 "preopen 目录：mkdir=0 / write=2 / exists=1"
else
    bad S-10 "preopen 目录操作失败"; cat "$TMP/dir.out" 2>/dev/null
fi
if "$RT" "$TMP/dir.wasm" >"$TMP/dir-none.out" 2>/dev/null \
   && [ "$(tr -d '\r' < "$TMP/dir-none.out" | tr '\n' '|')" = "-1|-1|0|" ]; then
    ok S-11 "无 preopen：mkdir=-1 / write=-1 / exists=0"
else
    bad S-11 "无 preopen 目录操作未拒绝"; cat "$TMP/dir-none.out" 2>/dev/null
fi

# S-12: FFI is not compiled into the wasm profile
if bash scripts/build-wasm.sh tests/security/wasm_ffi.ws "$TMP/ffi.wasm" >/dev/null 2>"$TMP/ffi.build.err"; then
    bad S-12 "FFI 源码竟然编译成功"
elif grep -q 'w_ffi_open' "$TMP/ffi.build.err"; then
    ok S-12 "FFI 在 wasm profile 下不可用（构建期失败）"
else
    bad S-12 "FFI 失败信息不含 w_ffi_open"; tail -3 "$TMP/ffi.build.err"
fi

# S-8: no Rust runtime / no Rust source changes
if ! grep -v '^#' scripts/build-wasm.sh | grep -q 'wasmtime\|wasmer' \
   && [ -z "$(git diff --name-only -- crates 2>/dev/null)" ]; then
    ok S-8 "构建脚本无 Rust 运行时；crates/ 无改动"
else
    bad S-8 "发现 Rust 运行时或 crates/ 改动"
fi

echo "wasm-security-gate: $PASS/$((PASS+FAIL))"
[ "$FAIL" -eq 0 ]
