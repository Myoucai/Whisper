#!/usr/bin/env bash
# Step 4 fmt 的第一道门禁：布局规范化器 wfmt。
# 判据见 tests/step4_fmt_preregistered.md。
#
# 用法：bash scripts/fmt-gate.sh
# 依赖：target/debug/whisper、gcc；wfmt 不存在时会用 -t c2 现场构建。
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${BIN:-$ROOT/target/debug/whisper}"
WFMT="${WFMT:-$ROOT/target/wfmt/wfmt}"
if [ ! -f "$BIN" ] && [ "${OS:-}" = "Windows_NT" ] && [ -f "$BIN.exe" ]; then BIN="$BIN.exe"; fi
if [ ! -f "$WFMT" ] && [ "${OS:-}" = "Windows_NT" ] && [ -f "$WFMT.exe" ]; then WFMT="$WFMT.exe"; fi
if [ ! -f "$BIN" ]; then
    echo "SKIP: build the whisper CLI first (cargo build -p whisper-cli)"
    exit 0
fi
mkdir -p "$(dirname "$WFMT")"
if [ ! -f "$WFMT" ]; then
    "$BIN" build "$ROOT/whisperc/fmt.ws" -t c2 -o "$WFMT" >/dev/null 2>&1 || {
        echo "FAIL: cannot build $WFMT"; exit 1; }
fi

TMP="target/fmt-gate.$$"
mkdir -p "$TMP"
cd "$ROOT" || exit 1
trap 'rm -rf "$TMP"' EXIT

pass=0; fail=0
ok()  { printf '  %-14s ✅ %s\n' "$1" "$2"; pass=$((pass+1)); }
bad() { printf '  %-14s ❌ %s\n' "$1" "$2"; fail=$((fail+1)); }
hash() { sha256sum "$1" | cut -d' ' -f1; }
run_vm() { "$BIN" run "$1" 2>/dev/null; }

# F-1：一个含定义/字符串/注释的文件
printf ': sq { _ * } ;\n"hi" .\n' > "$TMP/f1.ws"
if "$WFMT" "$TMP/f1.ws" "$TMP/f1.fmt.ws" >/dev/null 2>&1 && [ -s "$TMP/f1.fmt.ws" ]; then
    ok F-1 "构建并处理含定义/字符串的文件"
else
    bad F-1 "wfmt 构建/运行失败"
fi

# F-4/F-5：多行字符串、转义引号、注释里的引号
cat > "$TMP/f45.ws" <<'EOF'
// comment " with a quote
: sq { _ * } ;
"a  b" .
"say \"hi\"" .
"line1
   line2" strlen .
EOF
"$WFMT" "$TMP/f45.ws" "$TMP/f45.fmt.ws" >/dev/null 2>&1
run_vm "$TMP/f45.ws" > "$TMP/f45.a" 2>/dev/null
run_vm "$TMP/f45.fmt.ws" > "$TMP/f45.b" 2>/dev/null
if grep -qF '   line2"' "$TMP/f45.fmt.ws" \
   && grep -qF '// comment " with a quote' "$TMP/f45.fmt.ws" \
   && grep -qF '"say \"hi\""' "$TMP/f45.fmt.ws" \
   && [ "$(hash "$TMP/f45.a")" = "$(hash "$TMP/f45.b")" ]; then
    ok F-4/F-5 "多行字符串/转义/注释保持且语义一致"
else
    bad F-4/F-5 "字符串或注释未保持"
fi

# F-2/F-3：conformance 语料的幂等与 VM 语义保持
corpus_pass=0; corpus_fail=0
for f in "$ROOT"/tests/conformance/*.ws; do
    name="$(basename "$f" .ws)"
    fmt="$TMP/$name.fmt.ws"
    fmt2="$TMP/$name.fmt2.ws"
    if ! "$WFMT" "$f" "$fmt" >/dev/null 2>&1; then
        corpus_fail=$((corpus_fail+1)); echo "    wfmt failed: $name"; continue
    fi
    "$WFMT" "$fmt" "$fmt2" >/dev/null 2>&1
    if [ "$(hash "$fmt")" != "$(hash "$fmt2")" ]; then
        corpus_fail=$((corpus_fail+1)); echo "    not idempotent: $name"; continue
    fi
    run_vm "$f" > "$TMP/$name.a" 2>/dev/null
    run_vm "$fmt" > "$TMP/$name.b" 2>/dev/null
    if [ "$(hash "$TMP/$name.a")" != "$(hash "$TMP/$name.b")" ]; then
        corpus_fail=$((corpus_fail+1)); echo "    VM output changed: $name"; continue
    fi
    corpus_pass=$((corpus_pass+1))
done
if [ "$corpus_fail" -eq 0 ]; then
    ok F-2/F-3 "conformance $corpus_pass 个文件幂等且 VM 输出一致"
else
    bad F-2/F-3 "$corpus_fail 个文件失败（$corpus_pass 个通过）"
fi

# F-3b：5 个代表文件经 c2 编译运行，原/格式化输出一致
c2_pass=0; c2_fail=0
for name in 01_int_arith 07_comparisons 12_while_loop 13_times 16_defs_recursion; do
    src="$ROOT/tests/conformance/$name.ws"
    fmt="$TMP/$name.fmt.ws"
    [ -f "$fmt" ] || "$WFMT" "$src" "$fmt" >/dev/null 2>&1
    for side in orig fmt; do
        if [ "$side" = orig ]; then in="$src"; else in="$fmt"; fi
        out="$TMP/${name}_${side}"
        "$BIN" build -t c2 -o "$out" "$in" >/dev/null 2>&1 || c2_fail=$((c2_fail+1))
        bin="$out"; [ -f "$bin.exe" ] && bin="$out.exe"
        "$bin" > "$TMP/${name}_${side}.out" 2>/dev/null || c2_fail=$((c2_fail+1))
    done
    if [ "$(hash "$TMP/${name}_orig.out")" = "$(hash "$TMP/${name}_fmt.out")" ]; then
        c2_pass=$((c2_pass+1))
    else
        c2_fail=$((c2_fail+1)); echo "    c2 output changed: $name"
    fi
done
if [ "$c2_fail" -eq 0 ]; then
    ok F-3b "5 个代表文件经 c2 编译运行输出一致"
else
    bad F-3b "$c2_fail 个 c2 用例失败"
fi

# F-6：错误路径
"$WFMT" >/dev/null 2>&1; rc_usage=$?
"$WFMT" "$TMP/does-not-exist.ws" >/dev/null 2>&1; rc_missing=$?
if [ "$rc_usage" -eq 2 ] && [ "$rc_missing" -eq 1 ]; then
    ok F-6 "用法错=2，源文件缺失=1"
else
    bad F-6 "退出码不符（usage=$rc_usage missing=$rc_missing）"
fi

echo
echo "  fmt gate: $pass pass, $fail fail"
[ "$fail" -eq 0 ]
