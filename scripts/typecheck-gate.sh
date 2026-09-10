#!/bin/bash
# N-2 Phase 0 判据 harness：typecheck 误报棘轮（docs/language-backlog.md）。
# 语料：std/encoding 向量链（J-3 战败 fronts）+ tests/conformance +
# Lexicon 50（存在时）。诊断数只许降不许升（ratchet）。
# 用法：bash scripts/typecheck-gate.sh [whisper-bin]
set -u
W="${1:-./target/debug/whisper.exe}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# 预注册棘轮上限（2026-09-10 实测基线 = N-1 前后完全一致：
# encoding 0 / conformance 2 / lexicon 32——J-3 报告的 6 是当时语料规模，
# 后续新增测试（t_q*/t_mk4 等）带入存量近似。只许降，降了要更新此表）
readonly CEIL_ENCODING=0
readonly CEIL_CONFORMANCE=2
readonly CEIL_LEXICON=32

count_diag() {  # file → stdout: diagnostic count
  timeout 60 "$W" run "$1" 2>&1 >/dev/null | grep -c "^Type error:" || true
}

fail=0

# ── 语料 A：std/encoding 向量链（255 ctos —— J-3 反向失败 fronts）──
cat > "$TMP/enc.ws" <<'EOF'
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
EOF
n=$(count_diag "$TMP/enc.ws")
echo "encoding-chain: $n (ceiling $CEIL_ENCODING)"
[ "$n" -gt "$CEIL_ENCODING" ] && { echo "  RATCHET-BROKEN"; fail=1; }

# ── 语料 B：tests/conformance ──
total=0
for f in "$ROOT"/tests/conformance/*.ws; do
  c=$(count_diag "$f")
  total=$((total + c))
done
echo "conformance: $total (ceiling $CEIL_CONFORMANCE)"
[ "$total" -gt "$CEIL_CONFORMANCE" ] && { echo "  RATCHET-BROKEN"; fail=1; }

# ── 语料 C：Lexicon（存在才跑；CI 无此目录）──
LEX="${LEXICON_DIR:-/d/lexicon}"
if [ -d "$LEX/tests" ]; then
  total=0; worst=""
  for f in "$LEX"/tests/*.ws; do
    c=$(count_diag "$f") || c=0
    total=$((total + c))
    [ "$c" -gt 0 ] && worst="$worst $(basename "$f" .ws):$c"
  done
  echo "lexicon: $total (ceiling $CEIL_LEXICON) [$worst ]"
  [ "$total" -gt "$CEIL_LEXICON" ] && { echo "  RATCHET-BROKEN"; fail=1; }
else
  echo "lexicon: skipped (no $LEX/tests)"
fi

[ "$fail" -eq 0 ] && echo "typecheck-gate: PASS" || echo "typecheck-gate: FAIL"
exit $fail
