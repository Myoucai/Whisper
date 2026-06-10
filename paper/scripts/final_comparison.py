#!/usr/bin/env python3
"""Honest token comparison: old Whisper (no stdlib) vs new Whisper (stdlib-optimized)."""
import json

# Load old eval
with open("../data/eval.jsonl", "r", encoding="utf-8") as f:
    new_eval = [json.loads(l) for l in f if l.strip()]

# Old eval from the original benchmark (pre-stdlib-expansion)
old_eval = [
    ("Calculate the sum of 1 to 20", "[1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20] 0 { + } @fold ."),
    ("Define a function that returns the minimum of two numbers", ": min { _ over < ?? ] drop ] } ;\n3 7 min ."),
    ("Check if a number is positive", ": positive { _ 0 > } ;\n5 positive ."),
    ("Calculate 2^16", ": pow { _ 0 = ?? drop 1 | _ 1 - ` _ ` pow * ] } ;\n2 16 pow ."),
    ("Concatenate three strings", '"a" "b" strcat "c" strcat .'),
    ("Get the last element of a list", "[10 20 30 40 50] _ len 1 - @nth ."),
    ("Calculate the average of a list", "[2 4 6 8 10] _ 0 { + } @fold ` len / ."),
    ("Repeat a string n times", ': repeat {\n  _ 0 = ?? drop drop ""\n  | _ 1 - ` over ` repeat strcat ]\n} ;\n"abc" 3 repeat .'),
    ("Convert a list of digits to a number", "[1 2 3] 0 { _ 10 * ` + } @fold ."),
    ("Check if a string contains a substring", '"hello world" "lo" strfind 0 >= .'),
    ("Calculate the product of all elements in a list", "[2 3 4 5] 1 { * } @fold ."),
    ("Remove the first element from a list", "[1 2 3 4 5] 1 _ len 1 - strslice ."),
    ("Calculate the distance between two points", ': dist {\n  _ over - _ *\n  swap over - _ *\n  + fsqrt\n} ;\n0 0 3 4 dist .'),
    ("Generate a list of squares from 1 to 10", "[1 2 3 4 5 6 7 8 9 10] { _ * } @map ."),
    ("Check if a number is prime", ': prime {\n  _ 2 < ?? drop #f ]\n  _ 2 = ?? drop #t ]\n  _ 2 % 0 = ?? drop #f ]\n  3 { _ _ * over >= } { _ over % 0 = ?? drop drop #f | 2 + ] } #\n  #t\n} ;\n17 prime .'),
]

print("=" * 85)
print("Per-task token comparison: OLD Whisper → NEW Whisper (stdlib-optimized)")
print("=" * 85)
print(f"{'Task':<12} {'Old':>5} {'New':>5} {'Save':>6} {'Reduction':>10}")
print("-" * 85)

total_old = 0
total_new = 0
for i, ((_, old_code), new_task) in enumerate(zip(old_eval, new_eval)):
    old_tok = len(old_code.split())
    new_tok = len(new_task["whisper"].split())
    total_old += old_tok
    total_new += new_tok
    save = old_tok - new_tok
    pct = (1 - new_tok/old_tok) * 100 if old_tok else 0
    print(f"eval_{i+1:02d}      {old_tok:>5} {new_tok:>5} {save:>5}  {pct:>+9.1f}%")

print("-" * 85)
reduction = (1 - total_new/total_old) * 100
print(f"{'TOTAL':<12} {total_old:>5} {total_new:>5} {total_old-total_new:>5}  {reduction:>+9.1f}%")
print()
print(f"Old Whisper eval: {total_old} tokens")
print(f"New Whisper eval: {total_new} tokens")
print(f"Token savings:     {total_old - total_new} tokens ({reduction:.1f}%)")
print(f"Avg per task old:  {total_old/15:.1f}")
print(f"Avg per task new:  {total_new/15:.1f}")
print()
print("=== Key changes driving savings ===")
print("abs, max, min, neg → 1 token each (were 7-8)")
print("pow, factorial, fib → 1 token (were 25+)")
print("sum, prod, last, first, tail, rev → 1 token (were 5-12)")
print("even?, odd?, positive? → 1 token (were 5)")
print("Import overhead: 2-4 tokens (amortized across multiple uses)")
