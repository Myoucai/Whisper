#!/usr/bin/env bash
# 收口 ③：`wd` 内容寻址缓存的验收门禁（C-1..C-6）。
#
# 判据先写在 tests/wd_cache_preregistered.md；本脚本只做机器可验的断言。
# 依赖：已构建的 target/wd/wd、target/debug/whisper、gcc；本机临时目录。
#
# 用法：bash scripts/wd-cache-gate.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WD="${WD:-$ROOT/target/wd/wd}"
BIN="${BIN:-$ROOT/target/debug/whisper}"
[ -f "$WD" ] || WD="$WD.exe"
[ -f "$BIN" ] || BIN="$BIN.exe"
if [ ! -f "$WD" ]; then
    echo "SKIP: build $ROOT/target/wd/wd first (bash scripts/build-wd.sh)"
    exit 0
fi
if [ ! -f "$BIN" ]; then
    echo "SKIP: build the whisper CLI first (cargo build -p whisper-cli)"
    exit 0
fi

cd "$ROOT" || exit 1
export WHISPER_ROOT="$ROOT"

TMP="$(mktemp -d)"
RUNTIME="$ROOT/whisperc/runtime.c"
RUNTIME_BAK=""
cleanup() {
    if [ -n "$RUNTIME_BAK" ] && [ -f "$RUNTIME_BAK" ]; then
        cp "$RUNTIME_BAK" "$RUNTIME"
    fi
    rm -rf "$TMP"
}
trap cleanup EXIT INT TERM

CACHE="$ROOT/target/whisper-cache"
rm -rf "$CACHE"

# wd run 的 stdout 第一行是 "wd: compiled -> <out>"；判据只比转发段。
prog() { tail -n +2 "$1"; }
pass=0; fail=0
ok()  { printf '  %-10s ✅ %s\n' "$1" "$2"; pass=$((pass+1)); }
bad() { printf '  %-10s ❌ %s\n' "$1" "$2"; fail=$((fail+1)); }

printf 'import "std/math"\n5 sq .\n"abc" strlen .\n' > "$TMP/p.ws"

echo "=== C-2：第一次（未命中并回填）==="
t1=$(date +%s%N)
"$WD" run "$TMP/p.ws" "$TMP/out1" > "$TMP/o1" 2>"$TMP/e1"; r1=$?
t2=$(date +%s%N)
n1=$(find "$CACHE" -type f 2>/dev/null | wc -l)
printf '  exit=%s 耗时=%sms 缓存条目=%s 输出=[%s]\n' "$r1" "$(( (t2-t1)/1000000 ))" "$n1" "$(tr -d '\r\n' < "$TMP/o1")"
[ "$r1" -eq 0 ] && [ "$n1" -eq 1 ] && ok C-2a "首次未命中并写入 1 条缓存" || bad C-2a "未命中/回填异常"

echo "=== C-2：第二次（命中且更快）==="
t3=$(date +%s%N)
"$WD" run "$TMP/p.ws" "$TMP/out2" > "$TMP/o2" 2>"$TMP/e2"; r2=$?
t4=$(date +%s%N)
n2=$(find "$CACHE" -type f 2>/dev/null | wc -l)
printf '  exit=%s 耗时=%sms 缓存条目=%s 输出=[%s]\n' "$r2" "$(( (t4-t3)/1000000 ))" "$n2" "$(tr -d '\r\n' < "$TMP/o2")"
if [ "$r2" -eq 0 ] && [ "$n2" -eq 1 ] && [ "$(( t4-t3 ))" -lt "$(( t2-t1 ))" ]; then
    ok C-2b "命中且更快（$(( (t2-t1)/1000000 ))ms → $(( (t4-t3)/1000000 ))ms）"
else
    bad C-2b "命中判据不成立"
fi

echo "=== C-1：两次输出一致，且与从零编译的产物一致 ==="
"$BIN" build -t c2 -o "$TMP/ref" "$TMP/p.ws" >/dev/null 2>&1
REF="$TMP/ref.exe"; [ -f "$REF" ] || REF="$TMP/ref"
"$REF" > "$TMP/oref" 2>&1
if [ "$(prog "$TMP/o1" | sha256sum)" = "$(prog "$TMP/o2" | sha256sum)" ] \
   && [ "$(prog "$TMP/o1" | sha256sum)" = "$(sha256sum < "$TMP/oref")" ]; then
    ok C-1 "两次调用与参考实现三者输出逐字节相同"
else
    bad C-1 "输出不一致"
fi

echo "=== C-1b：命中时仍必须留下 <out> ==="
A="$TMP/out2"; [ -f "$A.exe" ] && A="$A.exe"
[ -f "$A" ] && ok C-1b "命中后产物仍在 $A" || bad C-1b "命中后没有产出 <out>"

echo "=== C-3：改源码必须失效 ==="
printf 'import "std/math"\n6 sq .\n"abc" strlen .\n' > "$TMP/p2.ws"
"$WD" run "$TMP/p2.ws" "$TMP/out3" > "$TMP/o3" 2>/dev/null
n3=$(find "$CACHE" -type f 2>/dev/null | wc -l)
got=$(prog "$TMP/o3" | tr -d '\r\n')
if [ "$n3" -eq 2 ] && [ "$got" = "363" ]; then
    ok C-3 "新条目产生，输出变为 [$got]"
else
    bad C-3 "条目=$n3 输出=[$got]（期望 2 与 363）"
fi

echo "=== C-4：改 runtime.c 必须失效 ==="
RUNTIME_BAK="$TMP/runtime.bak"
cp "$RUNTIME" "$RUNTIME_BAK"
printf '\n// cache-invalidation-probe\n' >> "$RUNTIME"
"$WD" run "$TMP/p.ws" "$TMP/out4" > "$TMP/o4" 2>/dev/null
n4=$(find "$CACHE" -type f 2>/dev/null | wc -l)
cp "$RUNTIME_BAK" "$RUNTIME"
[ "$n4" -eq 3 ] && ok C-4 "runtime.c 变了 → 新条目（$n4 条）" || bad C-4 "条目=$n4（期望 3，说明没失效）"
[ "$(prog "$TMP/o4" | sha256sum)" = "$(prog "$TMP/o1" | sha256sum)" ] && ok C-4b "改回后输出不变" || bad C-4b "输出变了"

echo "=== C-6：gcc 失败不写缓存 ==="
n_before=$(find "$CACHE" -type f 2>/dev/null | wc -l)
printf 'import "std/math"\n5 sq .\n' > "$TMP/bad.ws"
WHISPER_CC=/nonexistent/cc "$WD" run "$TMP/bad.ws" "$TMP/outbad" >/dev/null 2>&1
rc=$?
n_after=$(find "$CACHE" -type f 2>/dev/null | wc -l)
if [ "$rc" -ne 0 ] && [ "$n_before" -eq "$n_after" ]; then
    ok C-6 "失败($rc)且缓存条目未增（$n_before→$n_after）"
else
    bad C-6 "rc=$rc 条目 $n_before→$n_after"
fi

echo
echo "  wd 缓存: $pass pass, $fail fail"
[ "$fail" -eq 0 ]
