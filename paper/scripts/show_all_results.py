#!/usr/bin/env python3
"""Display all experiment results in one table."""
import json, os, glob

results_dir = os.path.join(os.path.dirname(__file__), "..", "results")

print("=" * 60)
print("ALL EXPERIMENT RESULTS")
print("=" * 60)

# Exp 1
print("\n--- Experiment 1: Execution Correctness (pass@1) ---")
p = os.path.join(results_dir, "exp1_pass_at_1.json")
if os.path.exists(p):
    with open(p) as f:
        d = json.load(f)
    print(f"  pass@1: {d['pass_count']}/{d['tasks']} = {d['pass_rate_pct']:.1f}%")
    for r in d['results']:
        s = "PASS" if r['output_match'] else "FAIL"
        print(f"    {r['task']}: {s} (gen_ran={r['generated_ran']})")
else:
    print("  Not run yet. Execute: python run_exp1_exec.py")

# Exp 2
print("\n--- Experiment 2: Fair Token Comparison ---")
p = os.path.join(results_dir, "exp2_fair_token.json")
if os.path.exists(p):
    with open(p) as f:
        d = json.load(f)
    print(f"  Reduction: {d['reduction_pct']:.1f}%")
    print(f"  Whisper wins: {d['whisper_wins']}/{d['tasks']}")
else:
    print("  Not run yet. Execute: python run_exp2_token.py")

# Exp 3
print("\n--- Experiment 3: Data Scaling Ablation ---")
sizes = [(250, "eval_250"), (500, "eval_500"), (750, "eval_750"), (1149, "eval_finetuned")]
for size, fname in sizes:
    found = False
    for ext in [".json", ".jsonl"]:
        p = os.path.join(results_dir, fname + ext)
        if os.path.exists(p):
            with open(p) as f:
                d = json.load(f)
            print(f"  {size:>4} examples: EM={d.get('exact_match_pct',0):.1f}%  SV={d.get('syntax_valid_pct',0):.1f}%")
            found = True
            break
    if not found:
        print(f"  {size:>4} examples: not found ({fname})")

# Exp 4
print("\n--- Experiment 4: Stdlib Ablation ---")
p = os.path.join(results_dir, "eval_no_stdlib.json")
if os.path.exists(p):
    with open(p) as f:
        d = json.load(f)
    print(f"  No stdlib: EM={d.get('exact_match_pct',0):.1f}%  SV={d.get('syntax_valid_pct',0):.1f}%")
else:
    print("  Not run yet. Execute: python run_exp4_stdlib.py")

print("\n" + "=" * 60)
