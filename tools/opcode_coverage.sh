#!/bin/bash
# K-2 opcode 覆盖门禁：逐特性双跑对拍（whisper run VM vs 自举链原生编译）。
# 自举编译器：NSC 环境变量注入（跳过 27s 构建）；未注入则现场构建一次。
# 任何 divergence = 门禁红——把 seed C-gen 的"静默 unknown"变成响亮失败。
# 用法：bash tools/opcode_coverage.sh [whisper-bin]（需 gcc 在 PATH）
set -u
W="${1:-./target/debug/whisper.exe}"
GCC="${GCC:-gcc}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
PASS=0
FAIL=0
GAP=0
GAPS=""
FAILED=""
# c2 目标债务清偿记录（L 阶段）：try/json/conf 已全链落地，白名单为空。
# 新缺口出现时在此登记（大声记账，不遮不藏）。
KNOWN_GAPS=""
WS2=""
case $(uname) in MINGW*|MSYS*|CYGWIN*) WS2="-lws2_32" ;; esac

if [ -z "${NSC:-}" ]; then
  echo "[coverage] building bootstrap compiler (one-shot)..."
  if ! "$W" build -t c2 -o "$TMP/nsc.exe" "$ROOT/whisperc/compiler.all.ws" >/dev/null 2>&1; then
    echo "FATAL: bootstrap build failed"; exit 2
  fi
  NSC="$TMP/nsc.exe"
fi

t() {  # t <name> <source>
  local name="$1" src="$2"
  printf '%s\n' "$src" > "$TMP/$name.ws"
  local vm_out c_out
  vm_out=$(timeout 20 "$W" run "$TMP/$name.ws" 2>/dev/null)
  gap() { GAP=$((GAP+1)); GAPS="$GAPS|$name:$1"; return 0; }
  if ! "$NSC" -c < "$TMP/$name.ws" > "$TMP/$name.c" 2>"$TMP/$name.err"; then
    case " $KNOWN_GAPS " in *" $name "*) gap bootstrap-compile-failed; return ;; esac
    FAIL=$((FAIL+1)); FAILED="$FAILED|$name:bootstrap-compile-failed"; return
  fi
  if ! $GCC -O2 -fno-strict-aliasing -w -o "$TMP/$name.exe" "$TMP/$name.c" -I "$ROOT/whisperc" -lm $WS2 2>"$TMP/$name.gcc"; then
    # 白名单债务通常长这样：C 里 #error（K-3 硬失败）或引用不存在的运行时助手（如 W_json_parse）
    case " $KNOWN_GAPS " in *" $name "*) gap gcc-failed; return ;; esac
    FAIL=$((FAIL+1)); FAILED="$FAILED|$name:gcc-failed"; return
  fi
  c_out=$(timeout 10 "$TMP/$name.exe" 2>/dev/null)
  # 归一化已知打印口径差异：布尔 #t/#f vs 1/0；字符串带/不带引号
  local vm_n c_n
  vm_n=$(printf '%s' "$vm_out" | sed 's/#t/1/g; s/#f/0/g' | tr -d '"')
  c_n=$(printf '%s' "$c_out" | sed 's/#t/1/g; s/#f/0/g' | tr -d '"')
  if [ "$vm_n" = "$c_n" ]; then
    PASS=$((PASS+1))
    return
  fi
  case " $KNOWN_GAPS " in
    *" $name "*)
      GAP=$((GAP+1)); GAPS="$GAPS|$name:vm=[$vm_out]native=[$c_out]"; return ;;
  esac
  FAIL=$((FAIL+1)); FAILED="$FAILED|$name:vm=[$vm_out]native=[$c_out]"
}

# ── 算术 ──
t arith      '40 2 + . 40 2 - . 40 2 * . 40 8 / . 40 7 % .'
# ── 比较 ──
t cmp-eq     '1 1 = . 1 2 = .'
t cmp-lt     '1 2 < . 2 1 < .'
t cmp-gt     '2 1 > . 1 2 > .'
t cmp-ne     '1 2 != . 1 1 != .'
t cmp-le     '1 1 <= . 2 1 <= .'
t cmp-ge     '1 1 >= . 1 2 >= .'
# ── 逻辑 ──
t logic      '1 1 & . 1 0 & . 0 0 | . 0 1 | . 0 ! . 1 ! .'
# ── 栈 ──
t stack      '5 dup + . 1 2 swap - . 1 2 3 rot . . . 7 drop 8 .'
t pick       '1 2 3 $0 . $2 . drop drop drop'
# ── 列表 ──
t list       '[1 2 3] 1 @nth . [1] 9 append 0 @nth . [9] 1 prepend 0 @nth . [1 2 3] len .'
t hof        '[1 2 3] { 2 * } @map 2 @nth . [1 2 3 4] { 2 % 0 = } @filter len .'
t hof-fold   '[1 2 3] 0 { + } @fold .'
t hof-times  '0 5 { 1 + } @times .'
# ── 字符串 ──
t str        '"abc" strlen . "ab" "cd" strcat . "abcdef" 2 3 strslice . "ab" "ab" streq . "ab" "ba" strlt .'
t str-find   '"abc" "b" strfind . "abc" "z" strfind .'
t str-repl   '"aXbXc" "X" "-" strreplace .'
t str-conv   '"123" strtoi64 1 + . 42 i64tostr . 65 ctos .'
t str-nth    '"Ab" 0 strnth . "Ab" 1 strnth .'
t str-chars  '"ab" strchars 1 @nth . [97 98] charsstr .'
t str-iter   '"x" striter . drop'
t str-join   '["a" "b"] "," strjoin .'
t str-split  '"a,b,c" "," strsplit len .'
t list-find  '[[1] [2]] [1] listfind drop "LF" .'
# ── map ──
t map2       'map-new dup "k" ` 1 swap map-set "k" swap map-get . drop'
t map-order  'map-new "c" 1 map-set "a" 2 map-set "b" 3 map-set map-keys . drop'
t map-order2 '"{\"c\":1,\"a\":2}" json-parse map-keys . drop'
# ── kv ──
t kv         '"K1" "V1" kv-set "K1" kv-get . "NOPE" kv-get strlen .'
# ── json（L-2：C 链全支持，对象键排序对齐 BTreeMap）──
t json       '"[1,2]" json-parse 1 @nth . [1 2] json-stringify .'
t json-obj   '"{\"b\":1,\"a\":2}" json-parse "a" map-get .'
t json-rt    '"[1,2,3]" json-parse json-stringify .'
t json-nest  '"[1,{\"k\":[2,3]}]" json-parse 1 @nth "k" map-get 1 @nth .'
# ── 浮点 ──
t float      '1.5 2.5 + f64toi64 . 3 i64tof64 0.5 + f64toi64 . 2.0 fsqrt f64toi64 .'
t float-cmp  '1.5 2.5 < . 2.5 2.5 <= .'
# ── 控制流 ──
t cond       '1 ?? "T" . | "F" . ] drop drop'
t cond-nest  '0 ?? "A" . | 1 ?? "B" . | "C" . ] ] drop drop drop'
t while      '0 { dup 3 < } { 1 + } # . drop drop drop'
# ── try（L-1：setjmp/longjmp 真错误恢复）──
t try        '{ 1 0 / } try drop "caught" .'
t try-ok     '{ 42 } try . .'
t try-type   '{ "s" 1 + } try drop drop'
# ── 词定义与递归 ──
t def        ': sq { dup * } ; 5 sq .'
t def-rec    ': fib { dup 2 < ?? drop | dup 1 - fib swap 2 - fib + ] } ; 8 fib .'
# ── 命名空间限定词（J-4/K-5：自举链已同步）──
t ns-def     ': mmod.mw { 42 } ; mmod.mw .'
t ns-float   '3.14 .'
t ns-print   '5 .'
# ── 置信度（L-3：#t:0.75 字面量 + conf-of peek + unwrap）──
t conf       '#t:0.75 conf-of . drop #t:0.5 unwrap .'
t conf-peek  '#t:0.25 conf-of . drop'
t conf-flat  '#f:0.5 unwrap .'

echo "coverage: $PASS pass, $FAIL fail, $GAP known-gap"
echo "$GAPS" | tr '|' '\n' | grep -v '^$' | sed 's/^/  GAP /'
echo "$FAILED" | tr '|' '\n' | grep -v '^$' | sed 's/^/  DIVERGE /'
[ "$FAIL" -eq 0 ]
