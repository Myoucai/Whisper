#!/usr/bin/env python3
"""Generate training data without stdlib import patterns."""
import json

with open('../data/train.jsonl') as f:
    data = [json.loads(l) for l in f if l.strip()]

no_stdlib = [ex for ex in data if 'import std/' not in ex.get('whisper', '')]

path = '../data/train_no_stdlib.jsonl'
with open(path, 'w') as f:
    for ex in no_stdlib:
        f.write(json.dumps(ex, ensure_ascii=False) + '\n')

print(f"Full dataset: {len(data)} examples")
print(f"No-stdlib dataset: {len(no_stdlib)} examples")
print(f"Removed: {len(data) - len(no_stdlib)} stdlib examples")
print("Done.")
