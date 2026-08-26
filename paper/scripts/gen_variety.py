#!/usr/bin/env python3
"""
Parameterized variety generator (v6 data).

Each idiom family gets many variants with randomized numbers, strings,
and phrasings — the model must READ the instruction, not memorize
literals. Every record is execution-verified; outputs are derived from
actual VM runs. eval_v2 whisper codes are excluded (no contamination).
"""
import argparse
import json
import os
import random
import subprocess
import tempfile

IO_ENV = {**os.environ, "WHISPER_ALLOW_IO": "1"}

NUMS = [2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 15, 17, 21, 23, 25, 28, 31,
        34, 42, 45, 47, 52, 56, 63, 68, 71, 77, 84, 89, 91, 96]
WORDS = ["go", "hi", "ok", "red", "blue", "sky", "cat", "dog", "sun",
         "win", "yes", "no", "big", "low", "old", "new", "fast", "slow",
         "left", "right", "up", "down", "hot", "cold", "early", "late",
         "alpha", "beta", "gamma", "delta", "echo", "fox", "jump", "king"]
PAIRS = [("yes", "no"), ("win", "lose"), ("hot", "cold"), ("fast", "slow"),
         ("high", "low"), ("on", "off"), ("open", "shut"), ("pass", "fail"),
         ("even", "odd"), ("pos", "neg"), ("adult", "minor"), ("zero", "nonzero")]
PHRASES_COND = ["Is {a} bigger than {b}? Print {w1} or {w2}",
                "Compare {a} and {b}: answer {w1} if {a} > {b}, else {w2}",
                "Check whether {a} exceeds {b} ({w1}/{w2})",
                "{a} vs {b} — print {w1} when greater, {w2} otherwise",
                "Test if {a} is greater than {b}; respond {w1} or {w2}"]
PHRASES_TIMES = ["Print the word '{w}' {k} times",
                 "Say '{w}' exactly {k} times",
                 "Output '{w}' on {k} lines",
                 "Repeat the string '{w}' {k} times",
                 "Run a block that prints '{w}', {k} times"]
PHRASES_ARITH = ["Add {a} and {b}", "Compute {a} + {b}",
                 "What is {a} plus {b}?", "Sum of {a} and {b}",
                 "Calculate {a} + {b}"]
PHRASES_SUB = ["Subtract {b} from {a}", "Compute {a} - {b}",
               "What is {a} minus {b}?", "{a} decreased by {b}"]
PHRASES_MUL = ["Multiply {a} by {b}", "Compute {a} * {b}",
               "What is {a} times {b}?", "Product of {a} and {b}"]
PHRASES_DIV = ["Integer-divide {a} by {b}", "Compute {a} / {b}",
               "What is {a} divided by {b} (integer)?"]
PHRASES_MOD = ["Remainder of {a} divided by {b}", "{a} modulo {b}",
               "What is {a} % {b}?"]
PHRASES_SQ = ["Square {a} using dup", "Compute {a}^2 with _",
              "{a} squared (use dup)", "Double {a} by adding it to itself"]
PHRASES_OVER = ["Copy the second value ({a} {b}) and multiply",
                "over: {a} {b} — multiply the copy by the top",
                "For stack {a} {b}, use over then multiply",
                "Duplicate the second element and multiply ({a}, {b})"]
PHRASES_SWAP = ["Swap {a} and {b} then subtract",
                "Exchange {a} {b} and subtract top from second",
                "Reverse {a} and {b}, then compute a-b"]
PHRASES_STRCAT = ["Join '{s1}' and '{s2}'", "Concatenate '{s1}' '{s2}'",
                  "Stick '{s2}' onto the end of '{s1}'"]
PHRASES_STRLEN = ["Length of '{s}'", "Count characters in '{s}'",
                  "How long is '{s}'?"]
PHRASES_LEN = ["Length of the list {lst}", "Count elements in {lst}",
               "How many items in {lst}?"]
PHRASES_REV = ["Reverse the list {lst}", "Turn {lst} around",
               "Reverse order of {lst}"]
PHRASES_MAP = ["Double every element of {lst}", "Map *2 over {lst}",
               "Multiply each item of {lst} by 2"]
PHRASES_FILT = ["Keep only even numbers from {lst}",
                "Filter {lst} for even values", "Odd numbers out of {lst}: drop them ({lst})"]
PHRASES_SUM = ["Sum all of {lst}", "Add up {lst}", "Total of {lst}"]
PHRASES_ABS = ["Absolute value of {a}", "abs({a})",
               "Make {a} non-negative"]
PHRASES_CNT = ["Count from 0 up to {k}-1 (inclusive) with a while loop",
               "Print the numbers 0 through {k}-1 using #"]


def build(rng):
    """Yield (instruction, code) pairs forever."""
    while True:
        fam = rng.randrange(15)
        if fam == 0:  # conditionals with word pairs — top failure mode
            a, b = rng.sample(NUMS, 2)
            w1, w2 = rng.choice(PAIRS)
            yield rng.choice(PHRASES_COND).format(a=a, b=b, w1=w1, w2=w2), \
                f'{a} {b} > ?? "{w1}" . | "{w2}" . ]'
        if fam == 1:  # @times with explicit words — top failure mode
            k = rng.randrange(2, 7)
            w = rng.choice(WORDS)
            yield rng.choice(PHRASES_TIMES).format(w=w, k=k), \
                f'{k} {{ "{w}" . }} @times'
        if fam == 2:
            a, b = rng.sample(NUMS, 2)
            yield rng.choice(PHRASES_ARITH).format(a=a, b=b), f"{a} {b} + ."
        if fam == 3:
            a, b = rng.sample(NUMS, 2)
            if a < b:
                a, b = b, a
            yield rng.choice(PHRASES_SUB).format(a=a, b=b), f"{a} {b} - ."
        if fam == 4:
            a, b = rng.sample(NUMS[:16], 2)
            yield rng.choice(PHRASES_MUL).format(a=a, b=b), f"{a} {b} * ."
        if fam == 5:
            a, b = rng.sample(NUMS[:16], 2)
            yield rng.choice(PHRASES_DIV).format(a=a, b=b), f"{a} {b} / ."
        if fam == 6:
            a, b = rng.sample(NUMS[:16], 2)
            yield rng.choice(PHRASES_MOD).format(a=a, b=b), f"{a} {b} % ."
        if fam == 7:
            a = rng.choice(NUMS)
            kind = rng.randrange(4)
            if kind == 0:
                yield rng.choice(PHRASES_SQ).format(a=a), f"{a} _ * ."
            elif kind == 1:
                yield f"Cube {a} using dup twice", f"{a} _ _ * * ."
            elif kind == 2:
                yield f"Compute {a}+{a} via dup", f"{a} _ + ."
            else:
                yield f"Compute {a}*{a} leaving one result", f"{a} _ _ drop * ."
        if fam == 8:  # over
            a, b = rng.sample(NUMS[:20], 2)
            op = rng.choice(["*", "+", "-"])
            yield rng.choice(PHRASES_OVER).format(a=a, b=b), f"{a} {b} over {op} ."
        if fam == 9:  # swap
            a, b = rng.sample(NUMS, 2)
            yield rng.choice(PHRASES_SWAP).format(a=a, b=b), f"{a} {b} ` - ."
        if fam == 10:
            s1, s2 = rng.sample(WORDS, 2)
            yield rng.choice(PHRASES_STRCAT).format(s1=s1, s2=s2), \
                f'"{s1}" "{s2}" strcat .'
        if fam == 11:
            s = rng.choice(WORDS)
            yield rng.choice(PHRASES_STRLEN).format(s=s), f'"{s}" strlen .'
        if fam == 12:  # lists
            n = rng.randrange(3, 7)
            lst = sorted(rng.sample(range(1, 30), n))
            lstr = "[" + " ".join(map(str, lst)) + "]"
            kind = rng.randrange(5)
            if kind == 0:
                yield rng.choice(PHRASES_LEN).format(lst=lstr), f"{lstr} len ."
            elif kind == 1:
                yield rng.choice(PHRASES_REV).format(lst=lstr), f"{lstr} rev ."
            elif kind == 2:
                yield rng.choice(PHRASES_MAP).format(lst=lstr), f"{lstr} {{ 2 * }} @map ."
            elif kind == 3:
                yield rng.choice(PHRASES_FILT).format(lst=lstr), f"{lstr} {{ 2 % 0 = }} @filter ."
            else:
                yield rng.choice(PHRASES_SUM).format(lst=lstr), f"{lstr} 0 {{ + }} @fold ."
        if fam == 13:
            a = rng.choice(NUMS + [-n for n in NUMS[:12]])
            yield rng.choice(PHRASES_ABS).format(a=a), f"{a} _ 0 < ?? -1 * | ] ."
        if fam == 14:  # while counting — failure mode
            k = rng.randrange(3, 8)
            yield rng.choice(PHRASES_CNT).format(k=k), \
                f"0 {{ _ . 1 + }} {{ _ {k} < }} #"


def run(code, bin_path, timeout=10):
    tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".ws",
                                      delete=False, encoding="utf-8")
    tmp.write(code)
    tmp.close()
    try:
        r = subprocess.run([bin_path, "run", tmp.name], capture_output=True,
                           text=True, timeout=timeout, env=IO_ENV,
                           cwd=os.path.dirname(os.path.abspath(__file__)))
        return " ".join(r.stdout.split()) if r.returncode == 0 else None
    except subprocess.TimeoutExpired:
        return None
    finally:
        os.unlink(tmp.name)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--whisper-bin", required=True)
    ap.add_argument("--count", type=int, default=900)
    ap.add_argument("--out", default="../data/train_variety.jsonl")
    args = ap.parse_args()

    with open("../data/eval_v2.jsonl", encoding="utf-8") as f:
        eval_codes = {json.loads(l)["whisper"] for l in f if l.strip()}

    rng = random.Random(20260826)
    gen = build(rng)
    records, seen, failed = [], set(), 0
    while len(records) < args.count:
        instr, code = next(gen)
        if code in seen or code in eval_codes:
            continue
        seen.add(code)
        out = run(code, args.whisper_bin)
        if out is None:
            failed += 1
            continue
        records.append({"instruction": instr, "input": "",
                        "whisper": code, "output": out, "variety": True})

    with open(args.out, "w", encoding="utf-8") as f:
        for r in records:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"variety: {len(records)} records -> {args.out} ({failed} templates failed)")


if __name__ == "__main__":
    main()
