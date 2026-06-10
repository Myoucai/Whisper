#!/usr/bin/env python3
"""
Evaluate fine-tuned Whisper model: generate code and compare with reference.

Usage:
    python evaluate.py --model ./whisper-qwen-7b --data ../data/eval.jsonl --output ../results/eval.json
"""

import argparse
import json
import torch
from transformers import AutoModelForCausalLM, AutoTokenizer, BitsAndBytesConfig


def generate_code(model, tokenizer, instruction: str, input_data: str = "", max_tokens: int = 256) -> str:
    """Generate Whisper code for a given instruction."""
    if input_data:
        user_msg = f"{instruction}\n\nInput: {input_data}"
    else:
        user_msg = instruction

    messages = [
        {"role": "system", "content": "You are a Whisper programming language expert. Whisper is an AI-native, stack-based, postfix language. Write only Whisper code unless asked otherwise."},
        {"role": "user", "content": user_msg},
    ]

    text = tokenizer.apply_chat_template(messages, tokenize=False, add_generation_prompt=True)
    inputs = tokenizer(text, return_tensors="pt").to(model.device)

    with torch.no_grad():
        outputs = model.generate(
            **inputs,
            max_new_tokens=max_tokens,
            temperature=0.1,
            do_sample=False,
        )

    generated = tokenizer.decode(outputs[0][inputs["input_ids"].shape[1]:], skip_special_tokens=True)
    return generated.strip()


def evaluate_exact_match(generated: str, expected: str) -> bool:
    """Check if generated code matches expected (normalized)."""
    return generated.strip() == expected.strip()


def evaluate_syntax_check(generated: str) -> bool:
    """Basic syntax check: balanced braces and brackets."""
    depth_brace = 0
    depth_bracket = 0
    for ch in generated:
        if ch == "{":
            depth_brace += 1
        elif ch == "}":
            depth_brace -= 1
        elif ch == "[":
            depth_bracket += 1
        elif ch == "]":
            depth_bracket -= 1
        if depth_brace < 0 or depth_bracket < 0:
            return False
    return depth_brace == 0 and depth_bracket == 0


def main():
    parser = argparse.ArgumentParser(description="Evaluate fine-tuned Whisper model")
    parser.add_argument("--model", required=True, help="Model path or name")
    parser.add_argument("--data", required=True, help="Evaluation data JSONL")
    parser.add_argument("--output", default="../results/eval.json", help="Output path")
    parser.add_argument("--max-tokens", type=int, default=256, help="Max generation tokens")
    args = parser.parse_args()

    print(f"Loading model: {args.model}")

    tokenizer = AutoTokenizer.from_pretrained(args.model, trust_remote_code=True)

    # Try loading with quantization, fall back to full precision
    try:
        bnb_config = BitsAndBytesConfig(load_in_4bit=True, bnb_4bit_compute_dtype=torch.bfloat16)
        model = AutoModelForCausalLM.from_pretrained(
            args.model, quantization_config=bnb_config,
            device_map="auto", trust_remote_code=True, torch_dtype=torch.bfloat16
        )
    except Exception:
        model = AutoModelForCausalLM.from_pretrained(
            args.model, device_map="auto", trust_remote_code=True, torch_dtype=torch.bfloat16
        )

    # Load evaluation data
    eval_data = []
    with open(args.data, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                eval_data.append(json.loads(line))

    print(f"Evaluating on {len(eval_data)} tasks...")

    results = []
    correct = 0
    syntax_ok = 0
    total = 0

    for i, task in enumerate(eval_data):
        instruction = task.get("instruction", "")
        input_data = task.get("input", "")
        expected = task.get("whisper", "")

        generated = generate_code(model, tokenizer, instruction, input_data, args.max_tokens)

        exact_match = evaluate_exact_match(generated, expected)
        syntax_valid = evaluate_syntax_check(generated)

        if exact_match:
            correct += 1
        if syntax_valid:
            syntax_ok += 1
        total += 1

        results.append({
            "task": task.get("task", f"task_{i}"),
            "instruction": instruction,
            "expected": expected,
            "generated": generated,
            "exact_match": exact_match,
            "syntax_valid": syntax_valid,
        })

        status = "✓" if exact_match else ("~" if syntax_valid else "✗")
        print(f"  [{status}] {task.get('task', f'task_{i}')}")

    # Summary
    print(f"\n{'='*60}")
    print(f"RESULTS: {correct}/{total} exact match ({100*correct/total:.1f}%)")
    print(f"SYNTAX:  {syntax_ok}/{total} valid ({100*syntax_ok/total:.1f}%)")
    print(f"{'='*60}")

    # Save results
    output = {
        "model": args.model,
        "tasks": total,
        "exact_match": correct,
        "exact_match_pct": 100 * correct / total if total > 0 else 0,
        "syntax_valid": syntax_ok,
        "syntax_valid_pct": 100 * syntax_ok / total if total > 0 else 0,
        "results": results,
    }

    import os
    os.makedirs(os.path.dirname(args.output), exist_ok=True)
    with open(args.output, "w", encoding="utf-8") as f:
        json.dump(output, f, indent=2, ensure_ascii=False)
    print(f"\nResults saved to {args.output}")


if __name__ == "__main__":
    main()
