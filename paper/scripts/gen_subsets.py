#!/usr/bin/env python3
"""Generate training data subsets at 250, 500, 750 sizes."""
import json, random
random.seed(42)

with open('../data/train.jsonl') as f:
    data = [json.loads(l) for l in f if l.strip()]
print(f"Full dataset: {len(data)} examples")

random.shuffle(data)

for size in [250, 500, 750]:
    subset = data[:size]
    path = f'../data/train_{size}.jsonl'
    with open(path, 'w') as f:
        for ex in subset:
            f.write(json.dumps(ex, ensure_ascii=False) + '\n')
    print(f'  train_{size}.jsonl: {len(subset)} examples')

print("Done.")
