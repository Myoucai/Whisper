#!/bin/bash
# 词表对拍门禁（S-5）：Rust 前端认识的关键字 × 自举链 compiler.all.ws 的词表。
#
# 为什么需要它：`mod` 是靠"随手试了一个取模"撞出来的——Rust 词法认识它、
# 自举链词表不认识它，于是 `-t c2` / `wd` 编出的 C 里出现未定义的 `W_mod()`。
# 同一类缺陷还有一批（见下），靠撞是撞不完的，只能机械枚举。
#
# 判据：只在 Rust 侧的词数 **不得超过 CEIL**（棘轮——只能减，不能增）。
# 新增一条就是往这一类里又加了一个坑，门禁会拦住。
#
# 用法：bash tools/word-table-parity.sh [repo-root]
set -u
ROOT="$(cd "${1:-$(dirname "$0")/..}" && pwd)"
LEX="$ROOT/crates/whisper-parser/src/lexer.rs"
TBL="$ROOT/whisperc/compiler.all.ws"

# 棘轮基线。数字只能往下调：每收掉一个就把它改小，别在没修的情况下调大。
# 2026-09-21 建立时 = 18（对拍出 22 个，其中 4 个是下面白名单里的良性项）。
# 可用 PARITY_CEIL 覆盖——用于自测"这门禁真的会拦"。
CEIL="${PARITY_CEIL:-18}"

# ── 白名单：确认良性，不算缺陷 ──────────────────────────────────────
#   over     自举前端在 compiler.all.ws 里用**代码路径**处理它
#            （`| _ "over" streq ?? drop [15] 1 append`），不在 [n "word"] 列表里，
#            两链都好用。
#   import   结构关键字，自举前端有意丢弃（不产生运行时行为）。
#   export   同上。
#   call     cgen 发 `W_call()`，而 `runtime.c` 定义了该符号——另有通路。
ALLOW="over
import
export
call"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Rust 侧：词法关键字表里 `"词" => TokenKind::X`
grep -oE '^[[:space:]]*"[^"]+"[[:space:]]*=>[[:space:]]*TokenKind::' "$LEX" \
  | sed -E 's/^[[:space:]]*"([^"]+)".*/\1/' | sort -u > "$tmp/rust.txt"

# 自举链侧：词表里的 `[n "word"]`
grep -oE '\[[0-9]+ "[^"]+"\]' "$TBL" \
  | sed -E 's/.*"([^"]+)".*/\1/' | sort -u > "$tmp/chain.txt"

printf '%s\n' "$ALLOW" | sort -u > "$tmp/allow.txt"

# 只在 Rust 侧 = rust − chain
grep -vxF -f "$tmp/chain.txt" "$tmp/rust.txt" > "$tmp/only_rust.txt" || true
# 再减掉白名单
grep -vxF -f "$tmp/allow.txt" "$tmp/only_rust.txt" > "$tmp/suspect.txt" || true

rust_n=$(wc -l < "$tmp/rust.txt")
chain_n=$(wc -l < "$tmp/chain.txt")
raw_n=$(wc -l < "$tmp/only_rust.txt")
n=$(wc -l < "$tmp/suspect.txt")

echo "[parity] Rust 关键字 $rust_n 个 · 自举链词表 $chain_n 项"
echo "[parity] 只在 Rust 侧 $raw_n 个，其中白名单 $((raw_n - n)) 个，**待处理 $n 个**（上限 $CEIL）"
if [ "$n" -gt 0 ]; then
  echo "[parity] 待处理清单（自举链词表里没有这些拼写）："
  sed 's/^/  /' "$tmp/suspect.txt"
fi

if [ "$n" -gt "$CEIL" ]; then
  echo "[parity] FAIL：待处理数 $n 超过基线 $CEIL —— 有新的'只有 Rust 认'的词加进来了。" >&2
  exit 1
fi
if [ "$n" -lt "$CEIL" ]; then
  echo "[parity] 比基线少了 $((CEIL - n)) 个 —— 请把脚本里的 CEIL 下调到 $n（棘轮只往下走）。"
fi
echo "[parity] PASS（$n ≤ $CEIL）"
