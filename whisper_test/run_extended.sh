#!/bin/bash
# Whisper Native Compiler — Extended Feature Tests
echo "=========================================="
echo " Whisper Extended Feature Tests"
echo "=========================================="

PASS=0
FAIL=0

run_test() {
    local num=$1
    local input="$2"
    local expected="$3"
    local desc="$4"

    printf "\n--- Test %02d: %s ---\n" "$num" "$desc"
    output=$(echo "$input" | ./whisper 2>/dev/null)
    exitcode=$?

    if [ "$output" = "$expected" ]; then
        echo "PASS ✓ ($output)"
        PASS=$((PASS+1))
    else
        echo "FAIL ✗ (expected: '$expected', got: '$output')"
        FAIL=$((FAIL+1))
    fi
}

# ── Extended Arithmetic ────────────────────────────────────────────
run_test 1 "10 3 / ." "3" "Integer division"
run_test 2 "10 3 % ." "1" "Modulo"
run_test 3 "2 3 ^ ." "8" "Power"
run_test 4 "100 10 / ." "10" "Division"

# ── Comparison ─────────────────────────────────────────────────────
run_test 5 "5 3 != ." "1" "Not equal"
run_test 6 "3 5 <= ." "1" "Less or equal"
run_test 7 "5 5 >= ." "1" "Greater or equal"

# ── Bitwise ────────────────────────────────────────────────────────
run_test 8 "12 5 & ." "4" "Bitwise AND"
run_test 9 "12 5 | ." "13" "Bitwise OR"
run_test 10 "12 5 ^ ." "9" "Bitwise XOR"
run_test 11 "1 3 << ." "8" "Left shift"
run_test 12 "8 2 >> ." "2" "Right shift"

# ── String Operations ──────────────────────────────────────────────
run_test 13 "\"hello\" \"world\" strcat ." "helloworld" "String concat"
run_test 14 "\"hello\" strlen ." "5" "String length"
run_test 15 "\"hello world\" 6 5 strslice ." "world" "String slice"
run_test 16 "\"hello\" \"hello\" streq ." "1" "String equal"
run_test 17 "\"abc\" \"def\" < ." "1" "String less than"

# ── List Operations ────────────────────────────────────────────────
run_test 18 "1 2 3 ] 1 @nth ." "2" "List nth"
run_test 19 "1 2 3 ] 4 append ." "4" "List append"
run_test 20 "1 2 3 ] len ." "3" "List length"

# ── Float Operations ───────────────────────────────────────────────
run_test 21 "5 i64tof64 ." "5" "Int to float"
run_test 22 "9 i64tof64 fsqrt ." "3" "Square root"

# ── String Conversions ─────────────────────────────────────────────
run_test 23 "42 i64tostr ." "42" "Int to string"
run_test 24 "\"123\" strtoi64 ." "123" "String to int"

# ── Conditional with Complex Logic ─────────────────────────────────
run_test 25 "5 3 > ?? 10 | 20 ] ." "10" "Conditional true"
run_test 26 "3 5 > ?? 10 | 20 ] ." "20" "Conditional false"

# ── Functions with Multiple Operations ─────────────────────────────
run_test 27 ": sq { _ * } ; : cube { _ sq * } ; 3 cube ." "27" "Cube function"
run_test 28 ": add { + } ; : mul { * } ; 3 4 add 5 mul ." "35" "Chained operations"

echo ""
echo "=========================================="
echo " Results: $PASS passed, $FAIL failed"
echo "=========================================="
