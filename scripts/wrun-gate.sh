#!/usr/bin/env bash
# Gate for wrun (host policy launcher). W-1..W-6.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
export PATH="/mingw64/bin:/usr/bin:/c/msys64/clang64/bin:$PATH"
export WRUN_IWASM="${WRUN_IWASM:-$ROOT/target/toolchain/wamr/iwasm.exe}"
TMP="$ROOT/target/wrun-gate"
rm -rf "$TMP"; mkdir -p "$TMP/data"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf '  %-6s ✅ %s\n' "$1" "$2"; }
bad() { FAIL=$((FAIL+1)); printf '  %-6s ❌ %s\n' "$1" "$2"; }

build() { bash scripts/build-wasm.sh "$1" "$2" >/dev/null 2>"$TMP/build.err"; }
build docs/examples/hello.ws "$TMP/hello.wasm" || { echo "wrun-gate: build hello failed" >&2; exit 1; }
build tests/security/wasm_file.ws "$TMP/file.wasm" || { echo "wrun-gate: build file fixture failed" >&2; exit 1; }
build tests/security/wasm_env.ws "$TMP/env.wasm" || { echo "wrun-gate: build env fixture failed" >&2; exit 1; }

# W-1: hello
bash scripts/wrun.sh "$TMP/hello.wasm" >"$TMP/hello.out" 2>/dev/null || true
if grep -q 'Hello, World!' "$TMP/hello.out"; then
    ok W-1 "wrun hello"
else
    bad W-1 "wrun hello"; cat "$TMP/hello.out"
fi

# W-2/W-3: file denied by default, allowed with --dir
bash scripts/wrun.sh "$TMP/file.wasm" >"$TMP/no-dir.out" 2>/dev/null || true
if [ "$(tr -d '\r' < "$TMP/no-dir.out" | tr '\n' '|')" = "-1|0|" ]; then
    ok W-2 "默认无 preopen：write=-1 / read=0"
else
    bad W-2 "默认未拒绝文件"; cat "$TMP/no-dir.out"
fi
if bash scripts/wrun.sh --dir "/data::$TMP/data" "$TMP/file.wasm" >"$TMP/dir.out" 2>/dev/null \
   && [ "$(tr -d '\r' < "$TMP/dir.out" | tr '\n' '|')" = "10|10|" ]; then
    ok W-3 "--dir /data：10/10"
else
    bad W-3 "--dir 读写失败"
fi

# W-4: env allowlist
WHISPER_TEST=ok bash scripts/wrun.sh --env WHISPER_TEST "$TMP/env.wasm" >"$TMP/env-ok.out" 2>/dev/null
if [ "$(tr -d '\r\n' < "$TMP/env-ok.out")" = "ok" ]; then
    ok W-4 "--env WHISPER_TEST：ok"
else
    bad W-4 "--env 未生效"
fi

# W-5/W-6: unsupported host features are rejected, not ignored
for opt in --http-allow --exec --ffi; do
    if bash scripts/wrun.sh "$opt" example.com "$TMP/hello.wasm" >/dev/null 2>"$TMP/$opt.err"; then
        bad "W-5" "$opt 应被拒绝"
    elif grep -q 'not implemented' "$TMP/$opt.err"; then
        ok "W-5" "$opt 显式拒绝"
    else
        bad "W-5" "$opt 错误信息不明确"
    fi
done

echo "wrun-gate: $PASS/$((PASS+FAIL))"
[ "$FAIL" -eq 0 ]
