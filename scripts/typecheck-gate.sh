#!/bin/bash
# N-2 判据 harness：typecheck 假阳性棘轮（docs/language-backlog.md）。
# 语料：std/encoding 向量链 + tests/conformance + Lexicon 50（存在时）。
# 2026-09-10 Phase 2 起升级为假阳性感知：诊断 × 运行时成败交叉表——
# 运行时成功的文件上的诊断才算假阳性（FP）。真阳性（运行时真错且
# 被诊断命中）是检查器的价值，不计数不惩罚。
# 用法：bash scripts/typecheck-gate.sh [whisper-bin]
set -u
W="${1:-./target/debug/whisper.exe}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# 预注册棘轮上限（2026-09-10 Phase 2 普查：FP = t_base:1 + t_mk4:2 = 3
# ——均为共享闭括号多 ?? 链的臂槽建模类（J-3 撤回项真身，Phase 2.5
# 靶子）；真阳性 15 = 检查器抓到的真 bug（t_ind6/s7 运行时亦错，
# 早先手工 rc 读数有误已由本门禁修正）。只许降，降了要更新此注）
readonly CEIL_FP_LEXICON=3
readonly CEIL_ENCODING=0
readonly CEIL_CONFORMANCE=2

count_diag() {  # file → stdout: diagnostic count
  timeout 60 "$W" run "$1" 2>&1 >/dev/null | grep -c "^Type error:" || true
}
runtime_ok() {  # file → rc 0 iff runs clean (stdout non-empty, no runtime error)
  local out
  out=$(timeout 60 "$W" run "$1" 2>"$TMP/rt.err")
  local rc=$?
  local rterr
  rterr=$(grep -c "^Error:" "$TMP/rt.err")
  [ "$rc" -eq 0 ] && [ "$rterr" -eq 0 ] && [ -n "$out" ]
}

fail=0

# ── 语料 A：std/encoding 向量链（运行时正确——诊断必须为 0）──
cat > "$TMP/enc.ws" <<'EOS'
import "std/encoding"
"Man" b64-enc .
"M" b64-enc .
"hello world" b64-enc b64-dec .
"TWFu" b64-dec .
"any carnal pleasure." b64-enc .
"a b&c" pct-enc .
"a%20b%26c" pct-dec .
255 ctos 255 ctos strcat dup b64-enc . b64url-enc .
"aGk" b64-dec .
"aGVsbG8" b64-dec .
EOS
n=$(count_diag "$TMP/enc.ws")
echo "encoding-chain: $n diag (ceiling $CEIL_ENCODING)"
[ "$n" -gt "$CEIL_ENCODING" ] && { echo "  RATCHET-BROKEN"; fail=1; }

# ── 语料 B：tests/conformance（运行时正确——诊断计为 FP）──
total=0
for f in "$ROOT"/tests/conformance/*.ws; do
  c=$(count_diag "$f")
  total=$((total + c))
done
echo "conformance: $total diag (ceiling $CEIL_CONFORMANCE)"
[ "$total" -gt "$CEIL_CONFORMANCE" ] && { echo "  RATCHET-BROKEN"; fail=1; }

# ── 语料 C：Lexicon（存在才跑；FP = 运行时成功文件上的诊断）──
LEX="${LEXICON_DIR:-/d/lexicon}"
if [ -d "$LEX/tests" ]; then
  fp=0; tp=0; fpdetail=""; tpdetail=""
  for f in "$LEX"/tests/*.ws; do
    c=$(count_diag "$f") || c=0
    [ "$c" -eq 0 ] && continue
    if runtime_ok "$f"; then
      fp=$((fp + c)); fpdetail="$fpdetail $(basename "$f" .ws):$c"
    else
      tp=$((tp + c)); tpdetail="$tpdetail $(basename "$f" .ws):$c"
    fi
  done
  echo "lexicon: FP=$fp (ceiling $CEIL_FP_LEXICON) [$fpdetail ]"
  echo "         true-positives=$tp (checker value, not penalized) [$tpdetail ]"
  [ "$fp" -gt "$CEIL_FP_LEXICON" ] && { echo "  RATCHET-BROKEN"; fail=1; }
else
  echo "lexicon: skipped (no $LEX/tests)"
fi

[ "$fail" -eq 0 ] && echo "typecheck-gate: PASS" || echo "typecheck-gate: FAIL"
exit $fail
