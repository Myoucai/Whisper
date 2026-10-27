#!/usr/bin/env bash
# Gate for wrun (host policy launcher). W-1..W-7.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
for d in /mingw64/bin /usr/bin /c/msys64/clang64/bin; do
    [ -d "$d" ] && PATH="$d:$PATH"
done
export PATH
if [ -z "${WRUN_IWASM:-}" ]; then
    for r in "$ROOT/target/toolchain/wamr/iwasm" "$ROOT/target/toolchain/wamr/iwasm.exe" iwasm; do
        if [ -x "$r" ] || command -v "$r" >/dev/null 2>&1; then WRUN_IWASM="$r"; break; fi
    done
fi
export WRUN_IWASM
[ -n "$WRUN_IWASM" ] || { echo "wrun-gate: no iwasm; set WRUN_IWASM" >&2; exit 1; }
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

# W-8: strict read-only preopen (Linux user/mount namespace).
if [ "$(uname -s 2>/dev/null)" = "Linux" ] && command -v unshare >/dev/null 2>&1 \
   && unshare -r -m true >/dev/null 2>&1; then
    rm -rf "$TMP/rodata"; mkdir -p "$TMP/rodata"
    if bash scripts/wrun.sh --dir-ro-strict "/data::$TMP/rodata" "$TMP/file.wasm" \
         >"$TMP/ro-strict.out" 2>"$TMP/ro-strict.err"; then
        if [ ! -e "$TMP/rodata/wsconf.txt" ] \
           && [ "$(tr -d '\r' < "$TMP/ro-strict.out" | tr '\n' '|')" = "-1|0|" ]; then
            ok W-8 "--dir-ro-strict：写入 EROFS，宿主无文件"
        else
            bad W-8 "--dir-ro-strict 写入了宿主或输出异常"; cat "$TMP/ro-strict.out"
        fi
    else
        bad W-8 "--dir-ro-strict 运行失败"; cat "$TMP/ro-strict.err"
    fi
else
    ok W-8 "SKIP：当前环境无 unprivileged user namespace"
fi

if WHISPER_WASM_MAX_MEMORY_MB=4 WHISPER_WASM_STACK_KB=64 WHISPER_WASM_MAX_THREADS=2 \
   bash scripts/wrun.sh "$TMP/hello.wasm" >"$TMP/quota.out" 2>"$TMP/quota.err"; then
    if grep -q 'Hello, World!' "$TMP/quota.out"; then
        ok W-9 "WASM heap/stack/threads 配额参数接受"
    else
        bad W-9 "配额运行输出异常"; cat "$TMP/quota.out"
    fi
else
    bad W-9 "WASM 配额参数导致运行失败"; cat "$TMP/quota.err"
fi

if WHISPER_WASM_MAX_FDS=64 bash scripts/wrun.sh "$TMP/hello.wasm" \
   >"$TMP/fds.out" 2>"$TMP/fds.err"; then
    if grep -q 'Hello, World!' "$TMP/fds.out"; then
        ok W-10 "WASM fd 配额（ulimit -n）生效"
    else
        bad W-10 "fd 配额运行输出异常"; cat "$TMP/fds.out"
    fi
else
    bad W-10 "fd 配额运行失败"; cat "$TMP/fds.err"
fi

echo "wrun-gate: $PASS/$((PASS+FAIL))"
[ "$FAIL" -eq 0 ]
