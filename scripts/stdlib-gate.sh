#!/bin/bash
# Stdlib gate: compile every module through the self-hosted c2 chain
# (Rust VM hosting compiler.all.ws -> C -> gcc -> run) and verify output.
# Requires the whisper CLI built (cargo build -p whisper-cli) and gcc.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${BIN:-$ROOT/target/debug/whisper.exe}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0

t() {
    local name="$1" src="$2" expected="$3"
    printf '%s' "$src" > "$TMP/g.ws"
    local c2out
    c2out=$(timeout 180 "$BIN" build "$TMP/g.ws" -t c2 -o "$TMP/g.exe" 2>/dev/null >/dev/null \
        && "$TMP/g.exe" 2>/dev/null | tr -d '\r')
    if [ "$c2out" = "$expected" ]; then
        pass=$((pass+1))
    else
        fail=$((fail+1))
        echo "FAIL $name: expected [$expected] got [$c2out]"
    fi
}

t math-sq      'import "std/math" 5 sq .' 25
t math-pi      'import "std/math" pi floor f64toi64 .' 3
t math-max     'import "std/math" 3 4 max .' 4
t math-clamp   'import "std/math" 5 1 10 clamp .' 5
t math-atan2   'import "std/math" 0 1 atan2 f64toi64 .' 0
t math-bitand  'import "std/math" 12 10 bitand .' 8
t list-mapsum  'import "std/list" [1 2 3 4 5] { _ _ * } @map 0 { + } @fold .' 55
t list-filter  'import "std/list" [1 2 3 4 5 6] { _ 2 % 0 = } @filter len .' 3
t list-nth     'import "std/list" [10 20 30] 1 @nth .' 20
t list-range   'import "std/list" 5 range-to 2 @nth .' 3
t list-maxval  'import "std/list" [3 1 4 1 5] max-val .' 5
t list-rev     'import "std/list" [1 2 3] rev 0 @nth .' 3
t list-sort    'import "std/list" [3 1 2] sort 0 @nth .' 1
t p1-mkdir     '"t_g1" mkdir . "t_g1" file-exists .' $'0
1'
t p1-strftime  '0 "%Y-%m-%d" strftime .' 1970-01-01
t p1-strftime2 '1770000000 "%H:%M:%S" strftime .' 02:40:00
t p1-listdir   '"." listdir len 0 > .' 1
t p1-leave     '0 { _ 3 = ?? leave | ] _ . 1 + } { _ 5 < } # drop "end" .' $'0
1
2
end'
t p1-loop-plain '0 { _ . 1 + } { _ 2 < } #' $'0
1'
t p2-re-dot    '"hello" "h.llo" strmatch .' 1
t p2-re-star   '"aaa" "a*" strmatch .' 1
t p2-re-class  '"abc123def" "[a-z]+[0-9]+[a-z]+" strmatch .' 1
t p2-re-neg    '"z" "[^a-y]" strmatch .' 1
t p2-re-find   '"abc123" "\d+" strfind-re .' 3
t p2-re-group  '"abab" "(ab)+" strmatch .' 1
t p2-re-anchor '"hello" "^hello$" strmatch .' 1
t p2-re-back   '"aaab" "a*ab" strmatch .' 1
t p2-re-alt    '"foo" "foo|baz" strmatch .' 1
t p2-re-no     '"hello" "hxllo" strmatch .' 0
t p2-sp-len     '[1 2] "<ih" pack strlen .' 6
t p2-sp-rt      '[-2 300 -30000] "<bih" pack "<bih" unpack .' '[-2 300 -30000]'
t p2-sp-sign    '[511] "<b" pack "<b" unpack .' '[-1]'
t p2-sp-str     '[65 "ok"] "<bs" pack "<bs" unpack 1 @nth .' ok
t p2-sp-trunc   '"ab" "<i" unpack .' '[]'
t p2-sp-f64bits '[3.5] "<d" pack "<d" unpack 0 @nth .' 4615063718147915776
t fix-strjoin  '"-" ["a" "b" "c"] strjoin .' 'a-b-c'
t fix-strjoin2 '", " ["x" "y"] strjoin .' 'x, y'
t p8-ffi-crc32 '"zlib1.dll" ffi-open dup "crc32" ffi-sym ` drop [0 "123456789" 9] "iipi" rot ffi-call .' 3421780262
t p8-ffi-sym-zero '0 "x" ffi-sym 0 = .' 1
t p0-upper     '"Hello" str-upper .' HELLO
t p0-lower     '"aBc" str-lower .' abc
t p0-strip     '"  pad  " str-strip .' pad
t p0-fmt       '"x={} y={}" 5 fmt 7 fmt .' 'x=5 y=7'
t p0-fmt-str   '"n={}" "Li" fmt .' 'n=Li'
t p0-fmt-mix   '"{} and {}" 3 fmt "x" fmt .' '3 and x'
t str-len      'import "std/str" "hello" strlen .' 5
t str-concat   'import "std/str" "hello" "world" strcat strlen .' 10
t str-upper    'import "std/str" "hello" upper .' HELLO
t str-split    'import "std/str" "a,b,c" "," strsplit len .' 3
t str-contains 'import "std/str" "hello" "ell" contains? .' 1
t str-fromint  'import "std/str" 42 from-int strlen .' 2
t mathf-sqrt   'import "std/mathf" 9 i64>f64 fsqrt f64>i64 .' 3
t core-gcd     'import "std/core" 12 18 gcd .' 6
t core-fib     'import "std/math" 10 fib .' 55
t core-pow     'import "std/math" 2 10 pow .' 1024
t core-range   'import "std/list" 5 range-to len .' 5

echo "=== stdlib gate: PASS=$pass FAIL=$fail ==="
[ "$fail" -eq 0 ]
rm -rf t_g1 t_zz
