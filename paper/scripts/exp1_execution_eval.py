#!/usr/bin/env python3
"""
Experiment 1: Execution-based Evaluation (pass@k)
Runs generated Whisper code on the VM and checks output correctness.

Usage (on machine with whisper binary):
    python exp1_execution_eval.py --whisper-bin ./target/release/whisper \
        --eval-data ../results/eval_finetuned.json \
        --output ../results/exp1_pass_at_1.json
"""
import argparse, json, os, subprocess, sys, tempfile

def run_whisper(code, whisper_bin, timeout=10):
    """Execute Whisper code via subprocess, return stdout."""
    tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".ws", delete=False, encoding="utf-8")
    tmp.write(code)
    tmp.close()
    try:
        result = subprocess.run(
            [whisper_bin, "run", tmp.name],
            capture_output=True, text=True, timeout=timeout,
            env={**os.environ, "WHISPER_ALLOW_IO": "1"}
        )
        output = result.stdout.strip()
        stderr = result.stderr.strip()
        if result.returncode != 0:
            return None, f"Exit {result.returncode}: {stderr[:200]}"
        return output, None
    except subprocess.TimeoutExpired:
        return None, "Timeout"
    except Exception as e:
        return None, str(e)
    finally:
        os.unlink(tmp.name)

def normalize_output(s):
    """Normalize output for comparison."""
    return s.strip().replace("\r", "")

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--whisper-bin", required=True, help="Path to whisper binary")
    parser.add_argument("--eval-data", required=True, help="Path to eval JSON results")
    parser.add_argument("--output", default="../results/exp1_pass_at_1.json")
    args = parser.parse_args()

    with open(args.eval_data, "r") as f:
        eval_results = json.load(f)

    tasks = eval_results.get("results", [])
    print(f"Loaded {len(tasks)} tasks from {args.eval_data}")
    print(f"Whisper binary: {args.whisper_bin}")
    print()

    results = []
    pass_count = 0
    run_count = 0

    for i, task in enumerate(tasks):
        instruction = task["instruction"]
        expected_code = task.get("expected", "")
        generated_code = task.get("generated", "")
        expected_output = ""  # eval.jsonl has output field

        # Clean generated code: strip markdown fences
        gen_clean = generated_code
        if gen_clean.startswith("```"):
            gen_clean = gen_clean.split("\n", 1)[-1]
            if gen_clean.endswith("```"):
                gen_clean = gen_clean[:-3]
        gen_clean = gen_clean.strip()

        print(f"[{i+1}/{len(tasks)}] {task.get('task', f'task_{i}')}: {instruction[:60]}")

        # Step 1: Verify expected code runs correctly
        exp_out, exp_err = run_whisper(expected_code, args.whisper_bin)
        expected_ok = exp_err is None

        # Step 2: Run generated code
        gen_out, gen_err = run_whisper(gen_clean, args.whisper_bin)
        generated_ok = gen_err is None

        # Step 3: Compare if both ran
        match = False
        if expected_ok and generated_ok:
            match = normalize_output(exp_out) == normalize_output(gen_out)
            if match:
                pass_count += 1
            run_count += 1

        status = "PASS" if match else ("EXEC_ERR" if not generated_ok else "MISMATCH")
        print(f"  Expected: {'OK' if expected_ok else 'FAIL'} | Generated: {'OK' if generated_ok else 'FAIL'} | {status}")
        if not match and expected_ok and generated_ok:
            print(f"    Expected output: {exp_out[:80]}")
            print(f"    Generated output: {gen_out[:80]}")

        results.append({
            "task": task.get("task", f"task_{i}"),
            "instruction": instruction,
            "expected_code": expected_code,
            "generated_code": gen_clean,
            "expected_ran": expected_ok,
            "expected_error": exp_err,
            "generated_ran": generated_ok,
            "generated_error": gen_err,
            "expected_output": exp_out,
            "generated_output": gen_out,
            "output_match": match,
        })

    print(f"\n{'='*60}")
    print(f"PASS@1: {pass_count}/{run_count} = {100*pass_count/run_count:.1f}%" if run_count else "No tasks ran")
    print(f"Expected code failures: {sum(1 for r in results if not r['expected_ran'])}")
    print(f"Generated code failures: {sum(1 for r in results if not r['generated_ran'])}")

    output = {
        "experiment": "exp1_execution_pass_at_1",
        "whisper_binary": args.whisper_bin,
        "tasks": run_count,
        "pass_count": pass_count,
        "pass_rate_pct": 100*pass_count/run_count if run_count else 0,
        "results": results,
    }
    os.makedirs(os.path.dirname(args.output), exist_ok=True)
    with open(args.output, "w") as f:
        json.dump(output, f, indent=2, ensure_ascii=False)
    print(f"\nSaved: {args.output}")

if __name__ == "__main__":
    main()
