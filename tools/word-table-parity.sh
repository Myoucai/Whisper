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
#   ⚠️ 初版写成 18 是**在错测量上建的棘轮**：提取时把 lexer.rs 里
#   `read_at_word` 的名字表也扫了进来，而那张表的键只在 `@` **之后**生效
#   （`@nth` / `@map` …），不是裸词关键字。于是 nth/map/each/fold/filter/times
#   六个被误报成"只在 Rust 侧"。已把提取限定在第一个名字表（见下面的 awk）。
#   2026-09-21 收敛到 0：`chr`/`listslice` 补进词表；其余 10 个是 **VM-only
#   功能**（见白名单），不是拼写缺口。
CEIL="${PARITY_CEIL:-0}"

# ── 白名单：确认良性，不算缺陷 ──────────────────────────────────────
# 三类：
#   (1) 拼写之外的等价通路
#   (2) 结构性关键字，自举前端有意丢弃
#   (3) **VM-only 功能**：opcode 在 `emit-bare` 里**没有分支**，C 链发射不了。
#       这与 `@6`-`@9` 同类——是"功能只有 VM 有"，不是"拼写只有一侧认"。
#       逐条核实过（`grep 'dup NN =' compiler.all.ws` 全部无命中）：
#         op-effect(42) size-of(43) word-size(44) seed!(45) rand(46)
#         bytes-new(189) bytes-push(190) bytes-len(191)
#         bytes-write(192) bytes->hex(206)
#       其中 bytes-* 另有 Rust 转译器自述佐证：
#         "#error bytes-* opcodes are not supported by the C backend".
ALLOW="over
import
export
call
op-effect
size-of
word-size
seed!
rand
bytes-new
bytes-push
bytes-len
bytes-write
bytes->hex"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Rust 侧：词法关键字表里 `"词" => TokenKind::X`。
# **必须止于 `fn read_at_word`**：那之后还有一张名字表，但它的键只在 `@` 之后
# 生效（`@nth`/`@map`/…），不是裸词。初版漏了这个截止条件，把六个 `@` 别名
# 误报成"只有 Rust 认的裸词"，棘轮基线因此虚高。
awk '/fn read_at_word/{exit}
     /^[[:space:]]*"[^"]+"[[:space:]]*=>[[:space:]]*TokenKind::/ {print}' "$LEX" \
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
