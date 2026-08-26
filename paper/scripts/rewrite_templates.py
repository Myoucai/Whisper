#!/usr/bin/env python3
"""
Step 1a: rewrite quarantined training records using execution-verified
frozen-syntax idioms.

Every canonical snippet below has been verified against the release VM.
Outputs are derived by execution at generation time — never hand-written.

Usage:
    python rewrite_templates.py --whisper-bin ../../target/release/whisper.exe \
        --quarantine ../data/train_quarantine.jsonl --out ../data/train_rewritten.jsonl
"""
import argparse
import json
import os
import re
import subprocess
import tempfile

IO_ENV = {**os.environ, "WHISPER_ALLOW_IO": "1"}


def run(code, bin_path):
    tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".ws",
                                      delete=False, encoding="utf-8")
    tmp.write(code)
    tmp.close()
    try:
        r = subprocess.run([bin_path, "run", tmp.name], capture_output=True,
                           text=True, timeout=10, env=IO_ENV)
        if r.returncode != 0:
            return None
        return " ".join(r.stdout.split())
    except subprocess.TimeoutExpired:
        return None
    finally:
        os.unlink(tmp.name)


# ── verified idiom families ──────────────────────────────────────────
FIB = ': fib { _ 1 > ?? _ 1 - fib ` 2 - fib + | ] } ;'
FACT = ': fact { _ 0 = ?? drop 1 | _ 1 - fact * ] } ;'
POW = ': pow { _ 0 = ?? drop drop 1 | over swap 1 - pow * ] } ;'
MAX = ': max { over over < ?? ` drop | drop ] } ;'
MIN = ': min { over over < ?? drop | ` drop ] } ;'
PRIME = (': prime { _ 2 < ?? drop #f | _ 1 - range-to { 1 > } @filter '
         '` { over ` % 0 = ?? drop 0 | 0 max ] } @fold 0 > ] } ;')
LEAP = ': leap { _ 4 % 0 = over 100 % 0 != @ 400 % 0 = ` | & } ;'

# family: (regex to match instruction/code, list of (instruction, code))
FAMILIES = [
    (r'\bfib', [
        (f"Compute the nth Fibonacci number (n=6)", f"{FIB}\n6 fib ."),
        (f"Compute the nth Fibonacci number (n=10)", f"{FIB}\n10 fib ."),
        (f"Compute the nth Fibonacci number (n=12)", f"{FIB}\n12 fib ."),
        (f"Compute the nth Fibonacci number (n=15)", f"{FIB}\n15 fib ."),
        (f"Compute the nth Fibonacci number (n=20)", f"{FIB}\n20 fib ."),
        (f"Compute the nth Fibonacci number (n=1)", f"{FIB}\n1 fib ."),
        (f"Compute the nth Fibonacci number (n=2)", f"{FIB}\n2 fib ."),
    ]),
    (r'factorial', [
        ("Compute the factorial of 5", f"{FACT}\n5 fact ."),
        ("Compute the factorial of 6", f"{FACT}\n6 fact ."),
        ("Compute the factorial of 7", f"{FACT}\n7 fact ."),
        ("Compute the factorial of 10", f"{FACT}\n10 fact ."),
        ("Compute the factorial of 0", f"{FACT}\n0 fact ."),
        ("Compute the factorial of 12", f"{FACT}\n12 fact ."),
        ("Compute 5! (five factorial)", f"{FACT}\n5 fact ."),
        ("Compute 10! (ten factorial)", f"{FACT}\n10 fact ."),
        ("Compute the factorial of 3", f"{FACT}\n3 fact ."),
        ("Compute the factorial of 8", f"{FACT}\n8 fact ."),
        ("Compute the factorial of 4", f"{FACT}\n4 fact ."),
    ]),
    (r'power|pow|2 \^|\^10|exponent', [
        ("Compute 2 to the power of 10", f"{POW}\n2 10 pow ."),
        ("Compute 3 to the power of 4", f"{POW}\n3 4 pow ."),
        ("Compute 5 to the power of 3", f"{POW}\n5 3 pow ."),
        ("Compute 2 to the power of 16", f"{POW}\n2 16 pow ."),
        ("Compute 7 squared using pow", f"{POW}\n7 2 pow ."),
        ("Compute 10 to the power of 6", f"{POW}\n10 6 pow ."),
        ("Compute 2 to the power of 32", f"{POW}\n2 32 pow ."),
        ("Compute 9 to the power of 3", f"{POW}\n9 3 pow ."),
        ("Compute 2 to the power of 0", f"{POW}\n2 0 pow ."),
        ("Compute 4 to the power of 5", f"{POW}\n4 5 pow ."),
    ]),
    (r'\bmax\b|\bmin\b|maximum|minimum|larger|smaller', [
        ("Find the maximum of 3 and 7", f"{MAX}\n3 7 max ."),
        ("Find the minimum of 3 and 7", f"{MIN}\n3 7 min ."),
        ("Find the maximum of two numbers", f"{MAX}\n12 5 max ."),
        ("Find the minimum of two numbers", f"{MIN}\n12 5 min ."),
        ("Find the maximum of 100 and 42", f"{MAX}\n100 42 max ."),
        ("Find the minimum of -3 and 8", f"{MIN}\n-3 8 min ."),
        ("Find the larger of 9 and 4", f"{MAX}\n9 4 max ."),
        ("Find the smaller of 9 and 4", f"{MIN}\n9 4 min ."),
        ("Find the maximum of 7 and 7", f"{MAX}\n7 7 max ."),
        ("Find the minimum of 0 and -10", f"{MIN}\n0 -10 min ."),
        ("Find the maximum of 25 and 13", f"{MAX}\n25 13 max ."),
        ("Find the minimum of 25 and 13", f"{MIN}\n25 13 min ."),
        ("Find the maximum of 6 and 11", f"{MAX}\n6 11 max ."),
        ("Find the minimum of 6 and 11", f"{MIN}\n6 11 min ."),
        ("Find the maximum of 1000 and 999", f"{MAX}\n1000 999 max ."),
        ("Find the minimum of 1000 and 999", f"{MIN}\n1000 999 min ."),
        ("Find the maximum of -5 and -2", f"{MAX}\n-5 -2 max ."),
    ]),
    (r'prime', [
        ("Check if 7 is a prime number", f"{PRIME}\n7 prime ."),
        ("Check if 9 is a prime number", f"{PRIME}\n9 prime ."),
        ("Check if 13 is a prime number", f"{PRIME}\n13 prime ."),
        ("Check if 25 is a prime number", f"{PRIME}\n25 prime ."),
        ("Check if 97 is a prime number", f"{PRIME}\n97 prime ."),
        ("Check if 2 is a prime number", f"{PRIME}\n2 prime ."),
        ("Check if 100 is a prime number", f"{PRIME}\n100 prime ."),
        ("Check if 51 is a prime number", f"{PRIME}\n51 prime ."),
    ]),
    (r'leap', [
        ("Check if 2024 is a leap year", f"{LEAP}\n2024 leap ."),
        ("Check if 1900 is a leap year", f"{LEAP}\n1900 leap ."),
        ("Check if 2000 is a leap year", f"{LEAP}\n2000 leap ."),
        ("Check if 2023 is a leap year", f"{LEAP}\n2023 leap ."),
    ]),
    (r'reverse|rev', [
        ("Reverse a list", "[1 2 3] rev ."),
        ("Reverse a list of numbers", "[10 20 30] rev ."),
        ("Reverse the order of list elements", "[5 6 7 8] rev ."),
        ("Reverse a list with 5 elements", "[1 2 3 4 5] rev ."),
        ("Reverse a list of strings", """["a" "b" "c"] rev ."""),
        ("Reverse a longer list", "[1 3 5 7 9 11] rev ."),
    ]),
    (r'sort', [
        ("Sort a list in ascending order", "[3 1 2] sort ."),
        ("Sort a list of numbers", "[5 2 8 1] sort ."),
        ("Sort an unsorted list", "[9 4 7 2 1] sort ."),
        ("Sort a list with duplicates", "[3 1 2 3 1] sort ."),
        ("Sort a list of 6 numbers", "[10 5 20 15 25 30] sort ."),
    ]),
    (r'bitwise|bit and|bit or|bit xor|xor|shift', [
        ("Compute bitwise AND of 12 and 10", "12 10 bitand ."),
        ("Compute bitwise OR of 12 and 10", "12 10 bitor ."),
        ("Compute bitwise XOR of 12 and 10", "12 10 bitxor ."),
        ("Shift 1 left by 4 bits", "1 4 shl ."),
        ("Shift 32 right by 2 bits", "32 2 shr ."),
        ("Compute bitwise NOT of 5", "5 bitnot ."),
        ("Compute bitwise AND of 255 and 15", "255 15 bitand ."),
        ("Compute bitwise XOR of 5 and 3", "5 3 bitxor ."),
    ]),
]

# additional list/string/loop families appended after FAMILIES to keep
# the matcher list readable
FAMILIES += [
    (r'sum|total', [
        ("Sum all elements in a list", "[1 2 3] 0 { + } @fold ."),
        ("Sum a list of numbers", "[10 20 30] 0 { + } @fold ."),
        ("Compute the total of a list", "[5 10 15] 0 { + } @fold ."),
        ("Sum the numbers 1 through 5", "5 range-to 0 { + } @fold ."),
        ("Sum a list of 100 numbers", "100 range-to 0 { + } @fold ."),
        ("Add up all values in a list", "[7 14 21] 0 { + } @fold ."),
        ("Sum an empty list", "[] 0 { + } @fold ."),
    ]),
    (r'product|multiply all', [
        ("Multiply all elements in a list", "[2 3 4] 1 { * } @fold ."),
        ("Compute the product of a list", "[1 2 3 4] 1 { * } @fold ."),
        ("Multiply all numbers together", "[5 6 7] 1 { * } @fold ."),
    ]),
    (r'average|mean', [
        ("Compute the average of a list", "[1 2 3 4] _ len ` 0.0 { + } @fold ` / ."),
        ("Compute the mean of [10 20 30]", "[10 20 30] _ len ` 0.0 { + } @fold ` / ."),
        ("Average the numbers 1 through 5", "5 range-to _ len ` 0.0 { + } @fold ` / ."),
    ]),
    (r'@map|double each|transform|increment each', [
        ("Double each element in a list", "[1 2 3] { 2 * } @map ."),
        ("Add 1 to every element of a list", "[1 2 3] { 1 + } @map ."),
        ("Square every element in a list", "[1 2 3 4] { _ * } @map ."),
        ("Negate each element of a list", "[1 -2 3] { -1 * } @map ."),
        ("Triple each element", "[2 4 6] { 3 * } @map ."),
        ("Map a list to its lengths' halves", "[10 20 30] { 2 / } @map ."),
    ]),
    (r'filter|even|odd|positive', [
        ("Filter even numbers from a list", "[1 2 3 4] { 2 % 0 = } @filter ."),
        ("Filter odd numbers from a list", "[1 2 3 4] { 2 % 0 != } @filter ."),
        ("Keep only positive numbers", "[-2 3 -5 7] { 0 > } @filter ."),
        ("Filter numbers greater than 10", "[5 12 8 20] { 10 > } @filter ."),
        ("Keep multiples of 3", "[1 3 6 7 9] { 3 % 0 = } @filter ."),
    ]),
    (r'length|count.*element|len\b', [
        ("Count the elements in a list", "[1 2 3] len ."),
        ("Get the length of a list", "[10 20 30 40] len ."),
        ("Length of an empty list", "[] len ."),
        ("Count elements in a 7-element list", "[1 2 3 4 5 6 7] len ."),
    ]),
    (r'append|add.*to (the )?(end|list)|push.*list', [
        ("Append an element to a list", "[1 2] 3 append ."),
        ("Add 4 to the end of [1 2 3]", "[1 2 3] 4 append ."),
        ("Append 10 to a list", "[5] 10 append ."),
        ("Append a string to a list of strings", '["a" "b"] "c" append .'),
    ]),
    (r'prepend|add.*to (the )?(front|beginning)|front of', [
        ("Prepend an element to a list", "1 [2 3] prepend ."),
        ("Add 0 to the front of [1 2 3]", "0 [1 2 3] prepend ."),
        ("Prepend a string to a list", '"x" ["y" "z"] prepend .'),
    ]),
    (r'take|first n|first \d', [
        ("Take the first 2 elements of a list", "[1 2 3 4] 2 take ."),
        ("Take the first 3 elements", "[1 2 3 4 5] 3 take ."),
        ("Get the first element only", "[9 8 7] 1 take ."),
    ]),
    (r'drop-n|skip|remove.*first|rest of|tail', [
        ("Drop the first 2 elements of a list", "[1 2 3 4] 2 drop-n ."),
        ("Remove the first element from a list", "[1 2 3] 1 drop-n ."),
        ("Get the rest after the first element", "[5 6 7] 1 drop-n ."),
        ("Skip the first 3 elements", "[1 2 3 4 5] 3 drop-n ."),
    ]),
    (r'list-slice|sublist|elements.*through', [
        ("Extract elements 1 through 3 of a list", "[1 2 3 4 5] 1 3 list-slice ."),
        ("Slice a list from index 2", "[10 20 30 40] 2 4 list-slice ."),
    ]),
    (r'dedup|duplicates|unique', [
        ("Remove duplicates from a list", "[1 2 2 3 3 3] dedup ."),
        ("Deduplicate a list", "[5 5 4 4 1] dedup ."),
        ("Keep only unique elements", "[7 7 7 8] dedup ."),
    ]),
    (r'nth|element at|index \d|@nth', [
        ("Get the 2nd element of a list", "[10 20 30] 2 @nth ."),
        ("Get the element at index 0", "[5 6 7] 0 @nth ."),
        ("Get the 3rd element", "[1 2 3 4] 3 @nth ."),
    ]),
    (r'range|1 to \d|1 through', [
        ("Generate a list from 1 to 5", "5 range-to ."),
        ("Generate a list from 1 to 10", "10 range-to ."),
        ("Make a list of numbers 1 through 3", "3 range-to ."),
    ]),
]

STR_FAMILIES = [
    (r'strlen|length of.*string|string.*length', [
        ("Get the length of a string", '"hello" strlen .'),
        ("Count characters in a string", '"whisper" strlen .'),
        ("Length of an empty string", '"" strlen .'),
        ("Length of a long string", '"programming language" strlen .'),
    ]),
    (r'strcat|concat.*string|join.*strings|combine', [
        ("Concatenate two strings", '"hello" " world" strcat .'),
        ("Join two strings together", '"foo" "bar" strcat .'),
        ("Combine a first and last name", '"Li" " Ming" strcat .'),
        ("Concatenate three strings", '"a" "b" strcat "c" strcat .'),
    ]),
    (r'strslice|substring|extract.*chars', [
        ("Extract a substring", '"hello" 1 3 strslice .'),
        ("Get the first 3 characters of a string", '"whisper" 0 3 strslice .'),
        ("Slice a string from index 2", '"abcdef" 2 4 strslice .'),
    ]),
    (r'streq|string.*equal|same string|compare.*string', [
        ("Check if two strings are equal", '"hello" "world" streq .'),
        ("Compare two strings for equality", '"abc" "abc" streq .'),
        ("Check string equality with same text", '"whisper" "whisper" streq .'),
    ]),
    (r'strfind|contains|find.*in.*string|substring.*in', [
        ("Check if a string contains a substring", '"hello" "llo" strfind 0 >= .'),
        ("Find the position of a substring", '"hello" "llo" strfind .'),
        ("Find where a substring starts", '"whisper" "isp" strfind .'),
        ("Check if a string starts with a prefix", '"hello world" "hello" strfind 0 = .'),
    ]),
    (r'strreplace|replace', [
        ("Replace a substring in a string", '"hello world" "world" "whisper" strreplace .'),
        ("Replace all occurrences of a letter", '"banana" "a" "o" strreplace .'),
        ("Replace text in a string", '"aaa" "a" "b" strreplace .'),
    ]),
    (r'strchars|characters of|chars of|split.*into char', [
        ("Convert a string to a list of character codes", '"hello" strchars .'),
        ("Get the character codes of a string", '"abc" strchars .'),
    ]),
    (r'strjoin|join.*list|list.*join', [
        ("Join a list of strings with a separator", '"-" ["a" "b"] strjoin .'),
        ("Join list elements with commas", '"," ["x" "y" "z"] strjoin .'),
        ("Join with empty separator", '"" ["a" "b" "c"] strjoin .'),
    ]),
    (r'strsplit|split.*string|split.*comma', [
        ("Split a string by commas", '"a,b,c" "," strsplit .'),
        ("Split a string by spaces", '"one two three" " " strsplit .'),
        ("Split a sentence into words", '"hello whisper world" " " strsplit .'),
    ]),
    (r'strtoi64|parse.*int|string to number', [
        ("Parse a string as an integer", '"42" strtoi64 .'),
        ("Convert a numeric string to a number", '"-17" strtoi64 .'),
        ("Parse string 123", '"123" strtoi64 .'),
    ]),
    (r'i64tostr|number to string|to string', [
        ("Convert a number to a string", "7 i64tostr ."),
        ("Convert 42 to a string", "42 i64tostr ."),
        ("Convert a negative number to a string", "-7 i64tostr ."),
    ]),
    (r'strnth|character at|nth char', [
        ("Get the 2nd character of a string (as code)", '"abc" 1 strnth .'),
        ("Get the first character code", '"hello" 0 strnth .'),
    ]),
    (r'ctos|char.*from code', [
        ("Convert a character code to a string", "65 ctos ."),
        ("Make a one-character string from code 98", "98 ctos ."),
    ]),
]

LOOP_FAMILIES = [
    (r'@times|repeat.*times|n times', [
        ("Print a message 3 times", '3 { "x" . } @times'),
        ("Repeat an action 5 times", '5 { "hi" . } @times'),
        ("Execute a block 2 times", '2 { "loop" . } @times'),
    ]),
    (r'while|loop until|count.*to \d|#\b', [
        ("Count from 0 to 2 with a while loop", "0 { _ . 1 + } { _ 3 < } #"),
        ("Loop while a counter is below 5", "0 { 1 + } { _ 5 < } # ."),
        ("Count up with a while loop", "0 { 1 + } { _ 5 < } # drop 5 ."),
        ("Print numbers 0 through 4", "0 { _ . 1 + } { _ 5 < } #"),
    ]),
    (r'@each|for each|foreach|iterate', [
        ("Print each element of a list", "[1 2 3] { . } @each"),
        ("Iterate over a list", '["a" "b"] { . } @each'),
        ("Process every element", "[10 20 30] { . } @each"),
    ]),
]

ALL_FAMILIES = FAMILIES + STR_FAMILIES + LOOP_FAMILIES


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--whisper-bin", required=True)
    ap.add_argument("--quarantine", default="../data/train_quarantine.jsonl")
    ap.add_argument("--out", default="../data/train_rewritten.jsonl")
    ap.add_argument("--report", default="../results/rewrite_report.json")
    args = ap.parse_args()

    with open(args.quarantine, encoding="utf-8") as f:
        quarantined = [json.loads(l) for l in f if l.strip()]

    # pre-execute every variant: family -> list of (instr, code, output)
    print("Verifying template variants by execution...")
    verified = []
    for regex, variants in ALL_FAMILIES:
        fam = []
        for instr, code in variants:
            out = run(code, args.whisper_bin)
            if out is None:
                print(f"  !! TEMPLATE FAILED TO RUN: {instr}: {code[:60]}")
            else:
                fam.append((instr, code, out))
        if fam:
            verified.append((regex, fam))
    total_variants = sum(len(f) for _, f in verified)
    print(f"  {total_variants} variants verified executable")

    rewritten, unmatched = [], []
    cursor = {i: 0 for i in range(len(verified))}  # round-robin per family
    for rec in quarantined:
        text = rec.get("instruction", "") + " " + rec.get("whisper", "")
        target = None
        for i, (regex, fam) in enumerate(verified):
            if re.search(regex, text, re.I):
                instr, code, out = fam[cursor[i] % len(fam)]
                cursor[i] += 1
                target = {"instruction": instr, "input": rec.get("input", ""),
                          "output": out, "whisper": code,
                          "rewritten_from": rec.get("instruction", "")}
                break
        if target:
            rewritten.append(target)
        else:
            unmatched.append(rec)

    with open(args.out, "w", encoding="utf-8") as f:
        for r in rewritten:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    os.makedirs(os.path.dirname(args.report), exist_ok=True)
    with open(args.report, "w", encoding="utf-8") as f:
        json.dump({"quarantined": len(quarantined),
                   "rewritten": len(rewritten),
                   "still_unmatched": len(unmatched),
                   "unmatched_instructions": [r.get("instruction", "")[:60]
                                              for r in unmatched]},
                  f, ensure_ascii=False, indent=1)

    print(f"\nrewritten        : {len(rewritten)}")
    print(f"still unmatched  : {len(unmatched)} (dropped from v3)")
    print(f"output -> {args.out}")


if __name__ == "__main__":
    main()
