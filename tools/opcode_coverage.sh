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
  # 行为对拍门禁：类型检查降级警告（用例含刻意的栈残留 drop）。
  # 能力门禁用例（@4/@5）需要对应的 capability 绑定，故一并打开；
  # 对不涉及能力的用例无影响（绑定不改变语义）。
  vm_out=$(timeout 20 "$W" run --no-strict-types \
    --allow-env --allow-exec --allow-file-read --allow-file-write \
    "$TMP/$name.ws" 2>/dev/null)
  gap() { GAP=$((GAP+1)); GAPS="$GAPS|$name:$1"; return 0; }
  if ! "$NSC" -c < "$TMP/$name.ws" > "$TMP/$name.c" 2>"$TMP/$name.err"; then
    case " $KNOWN_GAPS " in *" $name "*) gap bootstrap-compile-failed; return ;; esac
    FAIL=$((FAIL+1)); FAILED="$FAILED|$name:bootstrap-compile-failed"; return
  fi
  if ! $GCC -O2 -w -o "$TMP/$name.exe" "$TMP/$name.c" -I "$ROOT/whisperc" -lm $WS2 2>"$TMP/$name.gcc"; then
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
# ── N-1 bool 数值协变（VM 放松对齐 C 链，tests/n1_preregistered.md）──
t bool-arith '#t 1 + . #f 5 - . #t #f + .'
t bool-atan2 '0 1 atan2 f64toi64 .'
t bool-bit   '#t bitnot .'
t bool-f64   '#t 0.5 + .'

# ── 能力 ABI（Step 2a：@n 令牌在 C 链的接线）──────────────────────
# cgen 把 `@n` 发射成 `W__<n>()`；runtime.c 现实现 0/1/4/5 四个 id。
# 这四组用例是那条 ABI 的双链契约——在它们之前，六道 c2 门禁里
# 没有任何一个碰过能力令牌，`import "std/os"` 编不过却无人发现。
# 文件能力（@0/@1）不在此处覆盖：VM 侧的能力白名单是 CWD，
# 而 shell 里构造跨平台绝对路径过于脆弱——由 Rust 集成测试
# `capability_abi_matches_across_chains` 负责。
export WHISPER_GATE_VAR=gateval
t cap-env      '"WHISPER_GATE_VAR" @4 .'
t cap-exec-out '"echo hi" @5 1 @nth .'
t cap-exec-st  '"exit 7" @5 0 @nth .'
t cap-exec-err '"echo oops 1>&2" @5 2 @nth .'

# ── call（CapExec：动态调用栈顶引号）──────────────────────────────
# 该 opcode 自始就在 VM 里实现，却一直没有可达的拼写：设计原意是 `!`，
# 但 `!` 被逻辑非占用（spec §3.2 冻结），TokenKind::Bang 从此不再被词法
# 产出，opcode 就此悬空。现以 `call` 词暴露。
#
# 引号作为值一度只在"单个引号字面量"形态下两链一致：≥2 个引号、或引号
# 出现在列表字面量里都会让 whisperc compile-full 失败（战役 O / O′，
# 现已修复，见下方 quote-* 与 qlist-* 用例）。
t call-basic   '{ 42 } call .'
t call-input   '10 { 2 * } call .'
t call-in-def  ': ap { call } ; 5 { 1 + } ap .'

# ── 战役 O：多个引号字面量 ────────────────────────────────────────
# 两个不同的缺陷曾让这里失守：
#   1. genq-name 在非首次调用时把计数器字符串留在栈上（每多一个引号多一份
#      垃圾），两个引号即报 Type mismatch。
#   2. try-def-colon / try-loop-inner 对「体的后一个 token」直接调裸 streq，
#      而它可能是另一个引号节点（列表）。VM 报 expected str, got [T]；
#      C 链的 w_str_eq 拿列表指针比较、静默返回 false —— **两个执行引擎
#      在此分歧**，三个引号起才暴露。
#      这是 VM 与 C 链的真实语义分歧，已改用带类型守卫的 str-eq?。
#
# 列表字面量里的引号：fuse-hof 为"引号作值"合成的 [9,name,body] 定义只有
# 落在顶层才会被 split-defs 收走并发射成静态函数；落在容器节点内部就会走到
# compile-one 的兜底分支（它只认 tag 16 的引用），变成
# `#error whisper-cgen: unknown bare opcode`，引用悬空。hs/part 现在把定义
# 抬到容器之外——五种容器共用同一条通路。
t quote-pair   '{ 1 + } { 2 * } drop drop 7 .'
t quote-triple '{ 1 + } { 2 * } { 3 * } drop drop drop 7 .'
t quote-quad   '{ 1 + } { 2 * } { 3 * } { 4 * } drop drop drop drop 9 .'

# ── 战役 O′：列表字面量里的引号 + 动态分发 ────────────────────────
t qlist-len    '[{ 1 + }] len .'
t qlist-len2   '[{ 1 + } { 2 * }] len .'
t qlist-plain  '[1 2 3] len .'
t qlist-nest   '[[1] [2]] len .'
t qlist-call   '[{ 1 + }] 0 @nth 5 swap call .'
t qlist-disp   '[{ 1 + } { 2 * } { 3 * }] 1 @nth 5 swap call .'
t qlist-deep   '[[{ 1 + }]] 0 @nth 0 @nth 5 swap call .'
t qlist-arity  '9 [{ 1 + }] 0 @nth call .'

# ── 战役 O₂：另外四种容器（条件/循环/词定义/高阶融合体）里的引号 ──
# 同一条根因，hs 按 tag 区间 6..9 与 249..254 统一处理；这十例把另外四种
# 钉住，并各带一条"无引号"对照证明没误伤正常路径。
t qcond-then   '#t ?? { 9 } | { 8 } ] call .'
t qcond-else   '#f ?? { 9 } | { 8 } ] call .'
t qcond-multi  '#t ?? { 9 } { 10 } | { 8 } ] drop call .'
t qcond-word   ': f { 3 } ; #t ?? { f } | { 7 } ] call .'
t qcond-plain  '#t ?? 1 | 2 ] .'
t qloop-body   '2 { { 5 } drop 1 - } { dup 0 > } # .'
t qloop-plain  '0 { 1 + } { dup 3 < } # .'
t qdef-body    ': f { { 5 } drop 7 } ; f .'
t qquote-nest  '{ { 5 } } call call .'
t qhof-body    '[1 2] { { 7 } drop 1 + } @map .'

echo "coverage: $PASS pass, $FAIL fail, $GAP known-gap"
echo "$GAPS" | tr '|' '\n' | grep -v '^$' | sed 's/^/  GAP /'
echo "$FAILED" | tr '|' '\n' | grep -v '^$' | sed 's/^/  DIVERGE /'
[ "$FAIL" -eq 0 ]
