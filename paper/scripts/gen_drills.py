#!/usr/bin/env python3
"""
Targeted drills for observed failure modes of whisper-qwen30b-v4:
stack-op exact sequences (over/rot/$N/dup chains) and ??-conditionals.
All records execution-verified. Output: ../data/train_drills.jsonl
"""
import argparse
import json
import os
import subprocess
import tempfile

IO_ENV = {**os.environ, "WHISPER_ALLOW_IO": "1"}


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


TASKS = [
    # over: a b → a b a — consume in varied ways
    ("Copy the second value and multiply (4 7)", "4 7 over * ."),
    ("Copy the second value and add it (5 9)", "5 9 over + ."),
    ("Copy the second value and subtract top from copy (10 3)", "10 3 over - ."),
    ("Duplicate the second element onto the top (2 8)", "2 8 over drop ."),
    ("Compute a*(a+b) for 3 4", "3 4 over ` + * ."),
    ("Copy second, then drop original top (6 6)", "6 6 over ` drop ."),
    # rot: a b c → b c a
    ("Rotate three values and keep the deepest (1 2 3)", "1 2 3 @ drop drop ."),
    ("Rotate twice brings the bottom up (1 2 3)", "1 2 3 @ @ drop drop ."),
    ("Rotate then add all three (7 8 9)", "7 8 9 @ + + ."),
    ("Rotate, swap, then subtract (10 20 30)", "10 20 30 @ ` - ."),
    ("Sum after rotating (4 5 6)", "4 5 6 @ + + ."),
    # $N pick
    ("Pick the third stack element (11 22 33)", "11 22 33 $2 drop drop ."),
    ("Pick the second element (11 22)", "11 22 $1 drop ."),
    ("Copy deep element and multiply (1 2 3 4)", "1 2 3 4 $3 drop drop drop + ."),
    # dup chains — exact stack accounting
    ("Square via dup (6)", "6 _ * ."),
    ("Cube via double dup (2)", "2 _ _ * * ."),
    ("a+a*a via dup chain (3)", "3 _ _ * + ."),
    ("Compute 2a^2 leaving nothing extra (5)", "5 _ _ drop * ."),
    ("Sum of three dups (4)", "4 _ _ + + ."),
    # ?? conditionals — bool branches printing strings
    ("Is 5 positive? (prints pos/neg)", '5 0 > ?? "pos" . | "neg" . ]'),
    ("Is -9 positive? (prints pos/neg)", '-9 0 > ?? "pos" . | "neg" . ]'),
    ("Is 12 even? (prints even/odd)", '12 2 % 0 = ?? "even" . | "odd" . ]'),
    ("Is 7 even? (prints even/odd)", '7 2 % 0 = ?? "even" . | "odd" . ]'),
    ("Adult check age 21 (adult/minor)", '21 18 >= ?? "adult" . | "minor" . ]'),
    ("Adult check age 15 (adult/minor)", '15 18 >= ?? "adult" . | "minor" . ]'),
    ("Compare 3 and 8 (smaller/bigger first)", '3 8 < ?? "a<b" . | "a>=b" . ]'),
    ("Zero test (zero/nonzero)", '0 0 = ?? "zero" . | "nonzero" . ]'),
    ("Passing grade test 85 (pass/fail)", '85 60 >= ?? "pass" . | "fail" . ]'),
    ("Failing grade test 42 (pass/fail)", '42 60 >= ?? "pass" . | "fail" . ]'),
    # conditional returning values
    ("Max of 6 2 via ?? ", '6 2 over over < ?? ` drop | drop ] .'),
    ("Min of 6 2 via ??", '6 2 over over < ?? drop | ` drop ] .'),
    ("Absolute value of -8", '-8 _ 0 < ?? -1 * | ] .'),
    ("Absolute value of 8", '8 _ 0 < ?? -1 * | ] .'),
    # swap drills with arithmetic
    ("Swap then subtract (9 4 → 4-9)", "9 4 ` - ."),
    ("Swap then divide (3 12 → 12/3)", "3 12 ` / ."),
    ("Swap then multiply (3 7)", "3 7 ` * ."),
    ("Swap drop keeps original first (8 2)", "8 2 ` drop ."),
    # drop/nip
    ("Drop the top (17 5)", "17 5 drop ."),
    ("Keep only the first (13 29)", "13 29 ` drop ."),
]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--whisper-bin", required=True)
    ap.add_argument("--out", default="../data/train_drills.jsonl")
    args = ap.parse_args()

    records, failed = [], []
    for instr, code in TASKS:
        out = run(code, args.whisper_bin)
        if out is None:
            failed.append(instr)
            continue
        records.append({"instruction": instr, "input": "",
                        "whisper": code, "output": out, "drill": True})

    with open(args.out, "w", encoding="utf-8") as f:
        for r in records:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"drills: {len(records)}/{len(TASKS)} verified -> {args.out}")
    for i in failed:
        print(f"  FAILED: {i}")


if __name__ == "__main__":
    main()
