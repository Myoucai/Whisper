#!/usr/bin/env python3
"""
Step 2: expand the evaluation set 15 -> 100+ stratified tasks.

Parameters are deliberately DISJOINT from the training templates
(rewrite_templates.py / gen_newvocab.py): unseen numbers, unseen
strings. Every record is execution-verified; output = actual stdout.

Strata: arithmetic, stack, conditionals, loops, lists, strings,
floats, bitwise, recursion, KV/files/env/url, algorithms.
"""
import argparse
import json
import os
import subprocess
import tempfile

IO_ENV = {**os.environ, "WHISPER_ALLOW_IO": "1"}
NET = 'import "std/net"'
FIB = ': fib { _ 1 > ?? _ 1 - fib ` 2 - fib + | ] } ;'
FACT = ': fact { _ 0 = ?? drop 1 | _ 1 - fact * ] } ;'
POW = ': pow { _ 0 = ?? drop drop 1 | over swap 1 - pow * ] } ;'
MAX = ': max { over over < ?? ` drop | drop ] } ;'
MIN = ': min { over over < ?? drop | ` drop ] } ;'
PRIME = (': prime { _ 2 < ?? drop #f | _ 1 - range-to { 1 > } @filter '
         '` { over ` % 0 = ?? drop 0 | 0 max ] } @fold 0 > ] } ;')
LEAP = ': leap { _ 4 % 0 = over 100 % 0 != @ 400 % 0 = ` | & } ;'

TASKS = [
    # arithmetic (10)
    ("Add 17 and 25", "17 25 + ."),
    ("Subtract 9 from 44", "44 9 - ."),
    ("Multiply 7 by 8", "7 8 * ."),
    ("Integer-divide 100 by 7", "100 7 / ."),
    ("Remainder of 87 divided by 11", "87 11 % ."),
    ("Sum 4 5 6 in one expression", "4 5 + 6 + ."),
    ("Compute (2+3)*4", "2 3 + 4 * ."),
    ("Compute 100 - 3*7", "100 3 7 * - ."),
    ("Negate 42", "42 -1 * ."),
    ("Absolute value of -13", "-13 _ 0 < ?? -1 * | ] ."),
    # stack ops (8)
    ("Square 9 using dup", "9 _ * ."),
    ("Compute 2*11 with dup", "11 _ + ."),
    ("Swap 6 and 7 then multiply", "6 7 ` * ."),
    ("Over: copy second and add to top (3 8)", "3 8 over + ."),
    ("Rot three values then drop two (4 5 6)", "4 5 6 @ drop drop ."),
    ("Drop the top of 12 30", "12 30 drop ."),
    ("Pick the third element (1 2 3 -> copy 1)", "1 2 3 $2 drop drop drop ."),
    ("Double-dup then multiply top two (4 -> 4*4... via _ _ *)", "4 _ _ * * ."),
    # conditionals (8)
    ("Is 12 greater than 5?", "12 5 > ."),
    ("Is 3 equal to 4?", "3 4 = ."),
    ("Sign check: -4 vs 0 (print neg/pos)", '-4 0 < ?? "neg" . | "pos" . ]'),
    ("Adult check: age 20 vs 18 (print adult/minor)", '20 18 >= ?? "adult" . | "minor" . ]'),
    ("Max of 19 and 7 via ??", f"{MAX}\n19 7 max ."),
    ("Min of 19 and 7 via ??", f"{MIN}\n19 7 min ."),
    ("Classify 0: zero/nonzero", '0 0 = ?? "zero" . | "nonzero" . ]'),
    ("Check 9 < 3", "9 3 < ."),
    # loops (7)
    ("Count 0..4 with a while loop", "0 { _ . 1 + } { _ 5 < } #"),
    ('Run a block 4 times printing "go"', '4 { "go" . } @times'),
    ("Double a counter until >= 8", "1 { _ 2 * } { _ 8 < } # ."),
    ("Sum 1..6 with while", "0 1 { over + ` 1 + } { _ 7 < } # ` drop ."),
    ("Print countdown 3,2,1", "3 { _ . 1 - } { _ 0 > } #"),
    ("Sum five 2s via range and fold", "5 range-to { drop 2 } @map 0 { + } @fold ."),
    ("Loop 3 times building a count", "0 3 { 1 + } @times ."),
    # lists (14)
    ("Map square over [2 4 6]", "[2 4 6] { _ * } @map ."),
    ("Map +10 over [1 2 3]", "[1 2 3] { 10 + } @map ."),
    ("Filter multiples of 4 in [4 7 8 12]", "[4 7 8 12] { 4 % 0 = } @filter ."),
    ('Filter strings of length 3 from ["ab" "cde" "f"]', '["ab" "cde" "f"] { strlen 3 = } @filter .'),
    ("Fold subtraction over [10 3 2] from 20", "[10 3 2] 20 { - } @fold ."),
    ("Sum [4 5 6 7]", "[4 5 6 7] 0 { + } @fold ."),
    ("Max of a list via fold with max word", f"{MAX}\n[3 9 2 7] 0 {{ max }} @fold ."),
    ("Reverse [9 8 7 6]", "[9 8 7 6] rev ."),
    ("Sort [6 2 9 1 5]", "[6 2 9 1 5] sort ."),
    ("Take 3 of [a b c d e]", '["a" "b" "c" "d" "e"] 3 take .'),
    ("Drop 2 of [1 2 3 4 5]", "[1 2 3 4 5] 2 drop-n ."),
    ("Slice [1..7] from 2 to 5", "7 range-to 2 5 list-slice ."),
    ("Dedup [4 4 2 2 2 9]", "[4 4 2 2 2 9] dedup ."),
    ("Nth element 1 of [30 40 50]", "[30 40 50] 1 @nth ."),
    # strings (14)
    ("Length of 'whisper lang'", '"whisper lang" strlen .'),
    ("Concat 'good' and 'bye'", '"good" "bye" strcat .'),
    ("Slice 'abcdef' 2..5", '"abcdef" 2 5 strslice .'),
    ("Are 'one' and 'one' equal?", '"one" "one" streq .'),
    ("Find 'de' in 'abcdef'", '"abcdef" "de" strfind .'),
    ("Replace 'cat' with 'dog' in 'cat and cat'", '"cat and cat" "cat" "dog" strreplace .'),
    ("Split 'x-y-z' on '-'", '"x-y-z" "-" strsplit .'),
    ("Join [p q r] with '+'", '"+" ["p" "q" "r"] strjoin .'),
    ("Parse '777' as integer", '"777" strtoi64 .'),
    ("Convert 365 to string length", '365 i64tostr strlen .'),
    ("Char codes of 'hi'", '"hi" strchars .'),
    ("Character at index 2 of 'xyz'", '"xyz" 2 strnth .'),
    ("Contains 'an' in 'banana'?", '"banana" "an" strfind 0 >= .'),
    ("Char from code 122", "122 ctos ."),
    # floats (7)
    ("Float add 1.5 + 2.25", "1.5 2.25 + ."),
    ("Float multiply 0.5 * 8.0", "0.5 8.0 * ."),
    ("Float divide 7.0 / 2.0", "7.0 2.0 / ."),
    ("Average of [1 2 3 4 5] as float", "5 range-to _ len ` 0.0 { + } @fold ` / ."),
    ("Square root of 144.0", "144.0 fsqrt ."),
    ("Floor of 3.9", "3.9 floor ."),
    ("Compare floats 0.1+0.2 vs 0.3", "0.1 0.2 + 0.3 = ."),
    # bitwise (6)
    ("Bitwise AND 60 and 13", "60 13 bitand ."),
    ("Bitwise OR 60 and 13", "60 13 bitor ."),
    ("Bitwise XOR 60 and 13", "60 13 bitxor ."),
    ("Shift 3 left 5", "3 5 shl ."),
    ("Shift 200 right 3", "200 3 shr ."),
    ("Bitwise NOT of 0", "0 bitnot ."),
    # recursion (8)
    ("Fibonacci of 11", f"{FIB}\n11 fib ."),
    ("Fibonacci of 7", f"{FIB}\n7 fib ."),
    ("Factorial of 9", f"{FACT}\n9 fact ."),
    ("Factorial of 4", f"{FACT}\n4 fact ."),
    ("3 to the 5th", f"{POW}\n3 5 pow ."),
    ("11 to the 3rd", f"{POW}\n11 3 pow ."),
    ("Any base to power: 2 20", f"{POW}\n2 20 pow ."),
    ("5 to the 0", f"{POW}\n5 0 pow ."),
    # KV / files / env / url (8)
    ('KV: store "ada" under key "user" then read it back', '"user" "ada" kv-set "user" kv-get .'),
    ('KV: set key "tmp" to "x", delete it, print the length of its value', '"tmp" "x" kv-set "tmp" kv-del "tmp" kv-get strlen .'),
    ("KV keys count after 3 sets", '"q1" "1" kv-set "q2" "2" kv-set "q3" "3" kv-set kv-keys len .'),
    ('File: write "persisted" to "ev_r.txt" then read it back', '"ev_r.txt" "persisted" write-file drop "ev_r.txt" read-file .'),
    ('File: write "1234567" to "ev_l.txt" then print its length', '"ev_l.txt" "1234567" write-file drop "ev_l.txt" read-file strlen .'),
    ("PATH env is nonempty", '"PATH" getenv strlen 0 > .'),
    ("URL-decode 'x%20y%20z'", f'{NET}\n"x%20y%20z" url-decode .'),
    ("Extract path from 'POST /form?x=1 HTTP/1.0'", f'{NET}\n"POST /form?x=1 HTTP/1.0" req-path path-only .'),
    # algorithms (5)
    ("Is 31 prime?", f"{PRIME}\n31 prime ."),
    ("Is 49 prime?", f"{PRIME}\n49 prime ."),
    ("Is 1996 a leap year?", f"{LEAP}\n1996 leap ."),
    ("Is 2100 a leap year?", f"{LEAP}\n2100 leap ."),
    ("Is 1 prime?", f"{PRIME}\n1 prime ."),
    ("Sum of squares of [1..4]", "4 range-to { _ * } @map 0 { + } @fold ."),
    ("Count even numbers in [1..8]", "8 range-to { 2 % 0 = } @filter len ."),
    ("Concatenate three strings", '"x" "y" strcat "z" strcat .'),
    ("Absolute value of -99", "-99 _ 0 < ?? -1 * | ] ."),
    ("Modulo chain: (100 % 7) % 3", "100 7 % 3 % ."),
]


def run(code, bin_path, timeout=10):
    tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".ws",
                                      delete=False, encoding="utf-8")
    tmp.write(code)
    tmp.close()
    try:
        r = subprocess.run([bin_path, "run", tmp.name], capture_output=True,
                           text=True, timeout=timeout, env=IO_ENV,
                           cwd=os.path.dirname(os.path.abspath(__file__)))
        if r.returncode != 0:
            return None
        return " ".join(r.stdout.split())
    except subprocess.TimeoutExpired:
        return None
    finally:
        os.unlink(tmp.name)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--whisper-bin", required=True)
    ap.add_argument("--out", default="../data/eval_v2.jsonl")
    ap.add_argument("--min", type=int, default=100)
    args = ap.parse_args()

    records, failed = [], []
    for instr, code in TASKS:
        out = run(code, args.whisper_bin)
        if out is None:
            failed.append(instr)
            continue
        records.append({"instruction": instr, "input": "",
                        "whisper": code, "output": out})

    for f in os.listdir("."):
        if f.startswith("ev_") and f.endswith(".txt"):
            os.unlink(f)

    with open(args.out, "w", encoding="utf-8") as f:
        for r in records:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")

    print(f"eval_v2 : {len(records)}/{len(TASKS)} records verified")
    if len(records) < args.min:
        print(f"!! below target {args.min}")
    if failed:
        print("FAILED:")
        for i in failed:
            print(f"  - {i}")
            print(next(c for ins, c in TASKS if ins == i))


if __name__ == "__main__":
    main()
