#!/bin/bash
# Whisper Native Compiler — Full Test Suite
echo "=========================================="
echo " Whisper Native Compiler Test Suite"
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

# Basic
run_test 1  "42 ." "42" "Integer literal"
run_test 2  "3 4 + ." "7" "Addition"
run_test 3  "10 3 - ." "7" "Subtraction"
run_test 4  "3 4 * ." "12" "Multiplication"
run_test 5  "10 3 / ." "3" "Division"
run_test 6  "10 3 % ." "1" "Modulo"

# Comparison
run_test 7  "5 3 > ." "1" "Greater than"
run_test 8  "3 5 < ." "1" "Less than"
run_test 9  "3 3 = ." "1" "Equal"
run_test 10 "3 4 = ." "0" "Not equal"

# Stack
run_test 11 "5 _ * ." "25" "Dup and multiply"
run_test 12 "3 4 \` - ." "1" "Swap and subtract"

# Conditional
run_test 13 "1 ?? 42 | 0 ] ." "42" "If true"
run_test 14 "0 ?? 42 | 99 ] ." "99" "If false"
run_test 15 "5 3 > ?? 10 | 20 ] ." "10" "Conditional with comparison"

# Functions
run_test 16 ": sq { _ * } ; 5 sq ." "25" "Simple function"
run_test 17 ": double { _ + } ; 7 double ." "14" "Double function"
run_test 18 ": add3 { _ 3 + } ; 10 add3 ." "13" "Add3 function"
run_test 19 ": neg { 0 swap - } ; 5 neg ." "-5" "Negate function"

# Conditional functions
run_test 20 ": abs { _ 0 < ?? 0 swap - | ] } ; -7 abs ." "7" "Absolute value"
# Nested functions
run_test 21 ": inc { 1 + } ; : add3 { inc inc inc } ; 5 add3 ." "8" "Nested function calls"

# Multiple definitions (each needs its own ;)
run_test 24 ": a { 10 } ; : b { 20 } ; : c { a b + } ; c ." "30" "Multiple definitions"

echo ""
echo "=========================================="
echo " Results: $PASS passed, $FAIL failed"
echo "=========================================="
