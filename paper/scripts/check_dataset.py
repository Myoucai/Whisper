#!/usr/bin/env python3
"""Quick quality check on the generated training dataset."""
import json

with open("../data/train.jsonl", "r", encoding="utf-8") as f:
    train = [json.loads(l) for l in f if l.strip()]

print(f"Total examples: {len(train)}")

# Verify required fields
missing = 0
for i, ex in enumerate(train):
    for field in ["instruction", "whisper"]:
        if field not in ex or not ex[field]:
            missing += 1
            print(f"  Missing {field} at line {i}")

if missing == 0:
    print("All examples have required fields: OK")

# Check whisper code patterns
has_conditional = sum(1 for t in train if "??" in t["whisper"])
has_definition = sum(1 for t in train if ": " in t["whisper"] and " ;" in t["whisper"])
has_loop = sum(1 for t in train if "#" in t["whisper"] and "{" in t["whisper"])
has_map = sum(1 for t in train if "@map" in t["whisper"])
has_fold = sum(1 for t in train if "@fold" in t["whisper"])
has_each = sum(1 for t in train if "@each" in t["whisper"])
has_times = sum(1 for t in train if "@times" in t["whisper"])
has_strcat = sum(1 for t in train if "strcat" in t["whisper"])
has_strlen = sum(1 for t in train if "strlen" in t["whisper"])
has_len = sum(1 for t in train if " len " in t["whisper"])
has_nth = sum(1 for t in train if "@nth" in t["whisper"])
has_append = sum(1 for t in train if "append" in t["whisper"])
has_arithmetic = sum(1 for t in train if any(op in t["whisper"] for op in [" + ", " - ", " * ", " / "]))
has_logic = sum(1 for t in train if "#t" in t["whisper"] or "#f" in t["whisper"])
has_stack = sum(1 for t in train if any(x in t["whisper"] for x in ["dup", "swap", "rot", "drop", "_ ."]))
has_recursion = sum(1 for t in train if any(w in t["instruction"].lower() for w in ["recursiv", "ackermann", "hanoi"]))

print()
print("=== Feature Distribution ===")
print(f"  Arithmetic (+,-,*,/):     {has_arithmetic}")
print(f"  Conditionals (??):        {has_conditional}")
print(f"  Definitions (: name body): {has_definition}")
print(f"  Loops (#):                {has_loop}")
print(f"  @map over lists:          {has_map}")
print(f"  @fold over lists:         {has_fold}")
print(f"  @each iteration:          {has_each}")
print(f"  @times repetition:        {has_times}")
print(f"  String concat (strcat):   {has_strcat}")
print(f"  String length (strlen):   {has_strlen}")
print(f"  List length (len):        {has_len}")
print(f"  List index (@nth):        {has_nth}")
print(f"  List append:              {has_append}")
print(f"  Stack ops (dup/swap/rot): {has_stack}")
print(f"  Logic (#t/#f):            {has_logic}")
print(f"  Recursion:                {has_recursion}")

# Instruction length distribution
inst_lens = [len(t["instruction"].split()) for t in train]
print(f"\n=== Instruction Stats ===")
print(f"  Avg instruction words: {sum(inst_lens)/len(inst_lens):.1f}")
print(f"  Min/Max words: {min(inst_lens)}/{max(inst_lens)}")

# Whisper code length distribution
ws_lens = [len(t["whisper"].split()) for t in train]
print(f"\n=== Whisper Code Stats ===")
print(f"  Avg tokens: {sum(ws_lens)/len(ws_lens):.1f}")
print(f"  Min/Max tokens: {min(ws_lens)}/{max(ws_lens)}")

# Top 10 instructions by whisper code length
print("\n=== Longest Examples ===")
sorted_by_len = sorted(train, key=lambda t: len(t["whisper"].split()), reverse=True)
for ex in sorted_by_len[:5]:
    print(f"  [{len(ex['whisper'].split())} tokens] {ex['instruction'][:60]}")
