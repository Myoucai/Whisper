#!/usr/bin/env python3
"""
Verify rejection-sampling candidates by execution and assemble train_v5.

train_v5 = train_v4 + drills + accepted rejection samples (deduped by
whisper code). Only samples whose executed output equals the verified
reference output are accepted — the compiler is the judge.
"""
import argparse
import json
import os
import re
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
        return " ".join(r.stdout.split()) if r.returncode == 0 else None
    except subprocess.TimeoutExpired:
        return None
    finally:
        os.unlink(tmp.name)


def clean(gen):
    g = gen.strip()
    if g.startswith("```"):
        g = g.split("\n", 1)[-1]
        if g.endswith("```"):
            g = g[:-3]
    # keep only the first line block that looks like whisper code: drop
    # leading prose lines (contain spaces between words and end with .)
    return g.strip()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--whisper-bin", required=True)
    ap.add_argument("--samples", default="../results/reject_samples.jsonl")
    ap.add_argument("--out", default="../data/train_v5.jsonl")
    args = ap.parse_args()

    with open(args.samples, encoding="utf-8") as f:
        cand = [json.loads(l) for l in f if l.strip()]
    with open("../data/train_v4.jsonl", encoding="utf-8") as f:
        base = [json.loads(l) for l in f if l.strip()]
    try:
        with open("../data/train_drills.jsonl", encoding="utf-8") as f:
            drills = [json.loads(l) for l in f if l.strip()]
    except FileNotFoundError:
        drills = []

    seen = {r["whisper"] for r in base}
    accepted = []
    tried = 0
    for rec in cand:
        for s in rec.get("samples", []):
            code = clean(s)
            if not code or code in seen:
                continue
            tried += 1
            out = run(code, args.whisper_bin)
            exp = " ".join(rec.get("reference_output", "").split())
            if out is not None and out == exp:
                seen.add(code)
                accepted.append({
                    "instruction": rec["instruction"],
                    "input": rec.get("input", ""),
                    "whisper": code, "output": out,
                    "rejected_sampling": True})
    total = base + drills + accepted
    with open(args.out, "w", encoding="utf-8") as f:
        for r in total:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"candidates tried : {tried}")
    print(f"accepted         : {len(accepted)}")
    print(f"drills           : {len(drills)}")
    print(f"train_v5 total   : {len(total)} = {len(base)} + {len(drills)} + {len(accepted)}")


if __name__ == "__main__":
    main()
