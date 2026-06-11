#!/usr/bin/env python3
"""Experiment 1: Execution Correctness — run generated code on Whisper VM."""
import json, os, subprocess, sys, tempfile

WHISPER_BIN = os.path.join(os.path.dirname(__file__), "..", "..", "target", "release", "whisper")
if not os.path.exists(WHISPER_BIN):
    WHISPER_BIN = "./target/release/whisper"
if not os.path.exists(WHISPER_BIN):
    print("ERROR: whisper binary not found. Run 'cargo build --release' first.")
    print(f"  Tried: {WHISPER_BIN}")
    sys.exit(1)

def run_whisper(code):
    tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".ws", delete=False, encoding="utf-8")
    tmp.write(code)
    tmp.close()
    try:
        r = subprocess.run([WHISPER_BIN, "run", tmp.name],
                           capture_output=True, text=True, timeout=15)
        if r.returncode == 0:
            return r.stdout.strip(), None
        return None, f"Exit {r.returncode}: {r.stderr.strip()[:120]}"
    except Exception as e:
        return None, str(e)
    finally:
        os.unlink(tmp.name)

# Load eval data and model-generated results
with open("../data/eval.jsonl") as f:
    expected_tasks = {json.loads(l)["task"]: json.loads(l) for l in f if l.strip()}

with open("../results/eval_finetuned.json") as f:
    model_results = json.load(f)

print(f"Whisper binary: {WHISPER_BIN}")
print(f"Executable tasks in eval: {len(expected_tasks)}")
print(f"Model results: {len(model_results['results'])} tasks\n")

outputs = []
pass_count, run_count = 0, 0

for mr in model_results["results"]:
    task_id = mr["task"]
    expected = expected_tasks.get(task_id)
    gen_code = mr.get("generated", "").strip()

    # Clean markdown fences from generated code
    if gen_code.startswith("```"):
        gen_code = "\n".join(gen_code.split("\n")[1:])
    if gen_code.endswith("```"):
        gen_code = "\n".join(gen_code.split("\n")[:-1])
    gen_code = gen_code.strip()

    exp_output = expected["output"] if expected else ""

    # Run expected code
    exp_out, exp_err = run_whisper(expected["whisper"]) if expected else (None, "No expected")
    exp_ok = exp_err is None and exp_out is not None

    # Run generated code
    gen_out, gen_err = run_whisper(gen_code)
    gen_ok = gen_err is None and gen_out is not None

    # Compare if both ran
    match = False
    if exp_ok and gen_ok:
        exp_norm = exp_out.strip().replace("\r", "")
        gen_norm = gen_out.strip().replace("\r", "")
        match = exp_norm == gen_norm
        if match:
            pass_count += 1
        run_count += 1

    status = "PASS" if match else ("EXEC_ERR" if not gen_ok else "MISMATCH")
    print(f"  {task_id}: expected={'OK' if exp_ok else 'FAIL'} gen={'OK' if gen_ok else 'FAIL'} => {status}")
    if not match and exp_ok and gen_ok:
        print(f"    expected output: {exp_out[:60]}")
        print(f"    generated output: {gen_out[:60]}")

    outputs.append({
        "task": task_id,
        "expected_ran": exp_ok, "expected_error": exp_err,
        "generated_ran": gen_ok, "generated_error": gen_err,
        "expected_output": exp_out, "generated_output": gen_out,
        "output_match": match,
    })

pct = 100 * pass_count / run_count if run_count else 0
print(f"\n{'='*50}")
print(f"PASS@1: {pass_count}/{run_count} = {pct:.1f}%")

os.makedirs("../results", exist_ok=True)
with open("../results/exp1_pass_at_1.json", "w") as f:
    json.dump({
        "experiment": "exp1_execution_pass_at_1",
        "tasks": run_count, "pass_count": pass_count,
        "pass_rate_pct": pct, "results": outputs,
    }, f, indent=2, ensure_ascii=False)
print(f"Saved: ../results/exp1_pass_at_1.json")
