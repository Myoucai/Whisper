#!/usr/bin/env python3
"""
Step 0 of the re-tuning plan: execution audit of the training data.

Runs every record's Whisper code through the current (v1.0 frozen)
implementation and compares actual stdout against the recorded `output`
field. The compiler is the validator: anything that fails here is
language drift, not model error.

Usage:
    python check_dataset_exec.py --whisper-bin ../../target/release/whisper.exe \
        --data ../data/train.jsonl --report ../results/audit_train.json
"""
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from collections import Counter

# Words that only make sense with IO capability enabled
IO_ENV = {**os.environ, "WHISPER_ALLOW_IO": "1"}


def run_whisper(code, whisper_bin, timeout=10):
    """Execute Whisper code, return (stdout, error_kind, error_detail)."""
    tmp = tempfile.NamedTemporaryFile(
        mode="w", suffix=".ws", delete=False, encoding="utf-8")
    tmp.write(code)
    tmp.close()
    try:
        r = subprocess.run(
            [whisper_bin, "run", tmp.name],
            capture_output=True, text=True, timeout=timeout, env=IO_ENV)
        out = r.stdout.strip().replace("\r", "")
        if r.returncode == 0:
            return out, None, None
        err = (r.stderr or "").strip().splitlines()
        detail = err[-1] if err else f"exit {r.returncode}"
        if "Syntax" in detail or "Parse" in detail or "Lex" in detail:
            return out, "SYNTAX_ERROR", detail
        return out, "RUNTIME_ERROR", detail
    except subprocess.TimeoutExpired:
        return "", "TIMEOUT", "timeout"
    except Exception as e:
        return "", "RUNTIME_ERROR", str(e)
    finally:
        os.unlink(tmp.name)


def classify_error(detail):
    d = detail or ""
    if re.search(r"Undefined word", d):
        m = re.search(r"Undefined word: (\S+)", d)
        return "UNDEFINED_WORD:" + (m.group(1) if m else "?")
    if re.search(r"stack", d, re.I):
        return "STACK"
    if re.search(r"type|Type", d):
        return "TYPE"
    return "OTHER"


def normalize(s):
    """Collapse all whitespace runs to single spaces — record outputs use
    '3 2' while the VM prints one value per line."""
    return " ".join(s.split())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--whisper-bin", required=True)
    ap.add_argument("--data", required=True)
    ap.add_argument("--report", default=None)
    ap.add_argument("--limit", type=int, default=0, help="audit only first N records")
    args = ap.parse_args()

    with open(args.data, encoding="utf-8") as f:
        records = [json.loads(l) for l in f if l.strip()]
    if args.limit:
        records = records[:args.limit]

    print(f"Auditing {len(records)} records from {args.data}")
    results = []
    buckets = Counter()
    undefined_words = Counter()

    for i, rec in enumerate(records):
        code = rec.get("whisper", "")
        expected = (rec.get("output") or "").strip().replace("\r", "")
        out, err_kind, detail = run_whisper(code, args.whisper_bin)
        if err_kind is None:
            status = "PASS" if normalize(out) == normalize(expected) else "OUTPUT_MISMATCH"
        else:
            status = err_kind
        buckets[status] += 1
        if status == "OUTPUT_MISMATCH":
            buckets["MISMATCH_DETAIL"] += 0  # keep key order stable in report
        if err_kind == "RUNTIME_ERROR" and "Undefined word" in (detail or ""):
            undefined_words[classify_error(detail).split(":", 1)[1]] += 1
        results.append({
            "idx": i,
            "instruction": rec.get("instruction", "")[:80],
            "whisper": code,
            "expected_output": expected,
            "actual_output": out[:200],
            "status": status,
            "error": detail,
        })
        if (i + 1) % 200 == 0:
            print(f"  ... {i+1}/{len(records)}")

    total = len(results)
    print(f"\n===== AUDIT RESULT ({os.path.basename(args.data)}) =====")
    for k in ["PASS", "OUTPUT_MISMATCH", "SYNTAX_ERROR", "RUNTIME_ERROR", "TIMEOUT"]:
        if buckets[k]:
            print(f"  {k:16s} {buckets[k]:5d}  ({buckets[k]*100.0/total:.1f}%)")
    if undefined_words:
        print("\n  Top undefined words:")
        for w, c in undefined_words.most_common(15):
            print(f"    {w:20s} {c}")

    fails = [r for r in results if r["status"] != "PASS"]
    print(f"\n  SAMPLE FAILURES (of {len(fails)}):")
    for r in fails[:8]:
        print(f"  [{r['status']}] #{r['idx']} {r['instruction']}")
        print(f"      code: {r['whisper'][:90]}")
        print(f"      err/actual: {(r['error'] or r['actual_output'])[:90]}")

    if args.report:
        os.makedirs(os.path.dirname(args.report), exist_ok=True)
        with open(args.report, "w", encoding="utf-8") as f:
            json.dump({
                "data": args.data,
                "whisper_bin": args.whisper_bin,
                "total": total,
                "buckets": dict(buckets),
                "undefined_words": dict(undefined_words),
                "failures": fails,
            }, f, ensure_ascii=False, indent=1)
        print(f"\n  Report written: {args.report}")

    sys.exit(0 if not fails else 1)


if __name__ == "__main__":
    main()
