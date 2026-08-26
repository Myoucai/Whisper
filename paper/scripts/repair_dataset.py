#!/usr/bin/env python3
"""
Step 0b: repair the training dataset based on the execution audit.

Repair policy (code is only blessed when its intent is safe):
  - PASS                          -> keep as-is
  - OUTPUT_MISMATCH, multiset-of-printed-tokens equal (print-order
    convention: records assumed bottom-up display, VM prints top-first)
                                  -> keep, regenerate output from execution
  - OUTPUT_MISMATCH, numerically equal (float formatting/rounding)
                                  -> keep, regenerate output from execution
  - everything else (wrong math, syntax drift, runtime drift, timeouts)
                                  -> quarantine for rewriting in Step 1

Outputs train_v2.jsonl / train_quarantine.jsonl / repair_log.json
"""
import argparse
import json
import os


def tofloat(s):
    try:
        return float(s.strip())
    except (ValueError, AttributeError):
        return None


def tokens(s):
    return s.split()


def multiset_equal(exp, act):
    e, a = tokens(exp), tokens(act)
    if len(e) != len(a) or not e:
        return False
    ev = [tofloat(t) for t in e]
    av = [tofloat(t) for t in a]
    if any(v is None for v in ev) or any(v is None for v in av):
        return sorted(e) == sorted(a)
    return sorted(ev) == sorted(av)


def numerically_equal(exp, act):
    ev, av = tofloat(exp), tofloat(act)
    if ev is None or av is None:
        return False
    return abs(ev - av) < 5e-4 * max(1.0, abs(ev))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", default="../data/train.jsonl")
    ap.add_argument("--audit", default="../results/audit_train.json")
    ap.add_argument("--out", default="../data/train_v2.jsonl")
    ap.add_argument("--quarantine", default="../data/train_quarantine.jsonl")
    ap.add_argument("--log", default="../results/repair_log.json")
    args = ap.parse_args()

    with open(args.data, encoding="utf-8") as f:
        records = [json.loads(l) for l in f if l.strip()]
    with open(args.audit, encoding="utf-8") as f:
        audit = json.load(f)
    by_idx = {a["idx"]: a for a in audit["failures"]}

    kept, quarantined, repaired = [], [], []
    for i, rec in enumerate(records):
        a = by_idx.get(i)
        if a is None:
            rec2 = dict(rec)
            kept.append(rec2)
            continue
        exp = (rec.get("output") or "").strip()
        act = " ".join(a["actual_output"].split())
        if a["status"] == "OUTPUT_MISMATCH" and act and (
                multiset_equal(exp, act) or numerically_equal(exp, act)):
            rec2 = dict(rec)
            rec2["output"] = act
            rec2["repaired"] = ("print-order" if multiset_equal(exp, act)
                                else "float-format")
            kept.append(rec2)
            repaired.append({"idx": i, "kind": rec2["repaired"],
                             "old": exp, "new": act})
        else:
            q = dict(rec)
            q["fail_status"] = a["status"]
            q["fail_error"] = a["error"]
            q["fail_actual"] = act
            quarantined.append(q)

    for path, data in [(args.out, kept), (args.quarantine, quarantined)]:
        with open(path, "w", encoding="utf-8") as f:
            for r in data:
                f.write(json.dumps(r, ensure_ascii=False) + "\n")
    os.makedirs(os.path.dirname(args.log), exist_ok=True)
    with open(args.log, "w", encoding="utf-8") as f:
        json.dump({"kept": len(kept), "repaired": repaired,
                   "quarantined": len(quarantined)}, f, ensure_ascii=False, indent=1)

    po = sum(1 for r in repaired if r["kind"] == "print-order")
    ff = sum(1 for r in repaired if r["kind"] == "float-format")
    print(f"kept (clean)      : {len(kept) - len(repaired)}")
    print(f"auto-repaired     : {len(repaired)}  (print-order {po}, float-format {ff})")
    print(f"quarantined       : {len(quarantined)}")
    print(f"train_v2 total    : {len(kept)}")


if __name__ == "__main__":
    main()
