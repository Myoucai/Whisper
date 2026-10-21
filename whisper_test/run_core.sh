#!/bin/bash
# Core 22-test suite (matches whisper_test/run_all.sh cases)
# Usage: run_core.sh <whisper-binary> [mode]
#   mode "run"  (default): binary interprets stdin directly
#   mode "c"          : binary -c emits C; compiled then run (needs gcc)
BIN="$(realpath "${1:-./whisper0}")"
MODE="${2:-run}"
GCC="${GCC:-gcc}"
HERE="$(cd "$(dirname "$0")" && pwd)"
RT_INC="$HERE/../whisperc"
PASS=0; FAIL=0
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

t() {
    local num="$1" input="$2" expected="$3"
    local out
    if [ "$MODE" = "c" ]; then
        echo "$input" | "$BIN" -c > "$TMP/t$num.c" 2>/dev/null
        ( cd "$TMP" && "$GCC" -O2 -o "t$num" "t$num.c" \
            -I "$RT_INC" -lm 2>/dev/null && "./t$num" ) > "$TMP/t$num.out" 2>/dev/null
        out=$(cat "$TMP/t$num.out")
    else
        out=$(echo "$input" | "$BIN" 2>/dev/null)
    fi
    if [ "$out" = "$expected" ]; then
        PASS=$((PASS+1))
    else
        FAIL=$((FAIL+1))
        echo "FAIL #$num: '$input' expected '$expected' got '$out'"
    fi
}

t 1  "42 ." 42
t 2  "3 4 + ." 7
t 3  "10 3 - ." 7
t 4  "3 4 * ." 12
t 5  "10 3 / ." 3
t 6  "10 3 % ." 1
t 7  "5 3 > ." "#t"
t 8  "3 5 < ." "#t"
t 9  "3 3 = ." 1
t 10 "3 4 = ." 0
t 11 "5 _ * ." 25
t 12 '3 4 ` - .' 1
t 13 "1 ?? 42 | 0 ] ." 42
t 14 "0 ?? 42 | 99 ] ." 99
t 15 "5 3 > ?? 10 | 20 ] ." 10
t 16 ": sq { _ * } ; 5 sq ." 25
t 17 ": double { _ + } ; 7 double ." 14
t 18 ": add3 { _ 3 + } ; 10 add3 ." 13
t 19 ": neg { 0 swap - } ; 5 neg ." -5
t 20 ": abs { _ 0 < ?? 0 swap - | ] } ; -7 abs ." 7
t 21 ": inc { 1 + } ; : add3 { inc inc inc } ; 5 add3 ." 8
t 24 ": a { 10 } ; : b { 20 } ; : c { a b + } ; c ." 30

echo "=== PASS=$PASS FAIL=$FAIL ==="
[ "$FAIL" -eq 0 ]
