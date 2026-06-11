#!/usr/bin/env python3
"""Experiment 3: Data Scaling Ablation — train on 250/500/750 examples."""
import json, os, random, subprocess, sys

def run(cmd):
    print(f"  $ {cmd[:120]}...")
    r = subprocess.run(cmd, shell=True, check=False)
    if r.returncode != 0:
        print(f"  WARNING: command exited with {r.returncode}")
    return r.returncode

# ---- Step 1: Generate subsets ----
print("=" * 60)
print("STEP 1: Generate data subsets")
print("=" * 60)
run("python gen_subsets.py")

# ---- Step 2: Train and evaluate each size ----
for size in [250, 500, 750]:
    print(f"\n{'='*60}")
    print(f"STEP: Train & evaluate on {size} examples")
    print(f"{'='*60}")
    run(f"python train_and_eval.py ../data/train_{size}.jsonl ./whisper-qwen-{size}")

# ---- Step 3: Show summary ----
print(f"\n{'='*60}")
print("RESULTS: Data Scaling Ablation")
print(f"{'='*60}")

for size in [250, 500, 750]:
    path = f"../results/eval_{size}.jsonl"
    alt_path = f"../results/eval_{size}.json"
    for p in [path, alt_path]:
        if os.path.exists(p):
            with open(p) as f:
                d = json.load(f)
            print(f"  {size:>4} examples: EM={d.get('exact_match_pct',0):.1f}%  SV={d.get('syntax_valid_pct',0):.1f}%")
            break
    else:
        print(f"  {size:>4} examples: NOT FOUND")

# Also show 1149 (already trained)
for p in ["../results/eval_finetuned.json"]:
    if os.path.exists(p):
        with open(p) as f:
            d = json.load(f)
        print(f"  1149 examples: EM={d.get('exact_match_pct',0):.1f}%  SV={d.get('syntax_valid_pct',0):.1f}%")

print("\nDone.")
