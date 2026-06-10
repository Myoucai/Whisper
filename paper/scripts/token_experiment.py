#!/usr/bin/env python3
"""
Token Reduction Experiment: Fine-tuned model generates Whisper vs Python code.

Flow:
1. Load fine-tuned model
2. For each task, generate Whisper code AND Python code
3. Count tokens for each generated snippet
4. Compare: Whisper vs Python token counts

Usage:
    python token_experiment.py \
        --model ./whisper-qwen-7b \
        --base-model Qwen/Qwen2.5-Coder-7B-Instruct \
        --data ../data/benchmark.jsonl \
        --output ../results/
"""

import argparse
import json
import os
import torch
from transformers import AutoModelForCausalLM, AutoTokenizer, BitsAndBytesConfig


def load_model(model_path: str, base_model: str):
    """Load model (fine-tuned or base)."""
    tokenizer = AutoTokenizer.from_pretrained(model_path, trust_remote_code=True)
    try:
        bnb_config = BitsAndBytesConfig(load_in_8bit=True)
        model = AutoModelForCausalLM.from_pretrained(
            model_path, quantization_config=bnb_config,
            device_map="auto", trust_remote_code=True, torch_dtype=torch.bfloat16
        )
    except Exception:
        model = AutoModelForCausalLM.from_pretrained(
            model_path, device_map="auto", trust_remote_code=True, torch_dtype=torch.bfloat16
        )
    return model, tokenizer


def generate(model, tokenizer, system_prompt: str, user_prompt: str, max_tokens: int = 512) -> str:
    """Generate code using the model."""
    messages = [
        {"role": "system", "content": system_prompt},
        {"role": "user", "content": user_prompt},
    ]
    text = tokenizer.apply_chat_template(messages, tokenize=False, add_generation_prompt=True)
    inputs = tokenizer(text, return_tensors="pt").to(model.device)
    with torch.no_grad():
        outputs = model.generate(
            **inputs, max_new_tokens=max_tokens,
            temperature=0.1, do_sample=False,
        )
    return tokenizer.decode(outputs[0][inputs["input_ids"].shape[1]:], skip_special_tokens=True).strip()


def count_tokens(tokenizer, text: str) -> int:
    return len(tokenizer.encode(text, add_special_tokens=False))


def main():
    parser = argparse.ArgumentParser(description="Token reduction experiment")
    parser.add_argument("--model", required=True, help="Fine-tuned Whisper model path")
    parser.add_argument("--base-model", default="Qwen/Qwen2.5-Coder-7B-Instruct", help="Base model for Python generation")
    parser.add_argument("--data", default="../data/benchmark.jsonl", help="Task benchmark data")
    parser.add_argument("--output", default="../results", help="Output directory")
    parser.add_argument("--max-tokens", type=int, default=512, help="Max generation tokens")
    args = parser.parse_args()

    os.makedirs(args.output, exist_ok=True)

    # Load tasks
    tasks = []
    with open(args.data, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                tasks.append(json.loads(line))
    print(f"Loaded {len(tasks)} benchmark tasks\n")

    # Load fine-tuned model (for Whisper generation)
    print(f"Loading fine-tuned model: {args.model}")
    whisper_model, tokenizer = load_model(args.model, args.base_model)

    # Load base model (for Python generation — same tokenizer)
    print(f"Loading base model: {args.base_model}")
    python_model, _ = load_model(args.base_model, args.base_model)

    whisper_system = "You are a Whisper programming language expert. Whisper is an AI-native, stack-based, postfix language. Write only Whisper code."
    python_system = "You are a Python programming expert. Write only Python code."

    results = []

    for i, task in enumerate(tasks):
        task_name = task["task"]
        instruction = task.get("instruction", task_name.replace("_", " "))
        input_data = task.get("input", "")

        user_msg = f"{instruction}"
        if input_data:
            user_msg += f"\n\nInput: {input_data}"

        print(f"[{i+1}/{len(tasks)}] {task_name}")

        # Generate Whisper code
        whisper_code = generate(whisper_model, tokenizer, whisper_system, user_msg, args.max_tokens)
        whisper_tokens = count_tokens(tokenizer, whisper_code)

        # Generate Python code (using base model)
        python_code = generate(python_model, tokenizer, python_system, user_msg, args.max_tokens)
        python_tokens = count_tokens(tokenizer, python_code)

        # Reference code (from benchmark data)
        ref_whisper = task.get("whisper", "")
        ref_python = task.get("python", "")
        ref_whisper_tokens = count_tokens(tokenizer, ref_whisper) if ref_whisper else 0
        ref_python_tokens = count_tokens(tokenizer, ref_python) if ref_python else 0

        reduction = (1 - whisper_tokens / python_tokens) * 100 if python_tokens > 0 else 0

        results.append({
            "task": task_name,
            "instruction": instruction,
            "whisper_generated": whisper_code,
            "python_generated": python_code,
            "whisper_tokens": whisper_tokens,
            "python_tokens": python_tokens,
            "reduction_pct": reduction,
            "ref_whisper_tokens": ref_whisper_tokens,
            "ref_python_tokens": ref_python_tokens,
        })

        print(f"  Whisper: {whisper_tokens} tok | Python: {python_tokens} tok | Reduction: {reduction:+.1f}%")

    # Summary
    print(f"\n{'='*70}")
    print("SUMMARY")
    print(f"{'='*70}")

    whisper_total = sum(r["whisper_tokens"] for r in results)
    python_total = sum(r["python_tokens"] for r in results)
    avg_reduction = (1 - whisper_total / python_total) * 100 if python_total > 0 else 0

    print(f"Tasks:               {len(results)}")
    print(f"Whisper total tokens: {whisper_total}")
    print(f"Python total tokens:  {python_total}")
    print(f"Average reduction:    {avg_reduction:.1f}%")
    print(f"Whisper avg/task:     {whisper_total/len(results):.1f}")
    print(f"Python avg/task:      {python_total/len(results):.1f}")

    # Per-task breakdown
    print(f"\n{'='*70}")
    print("PER-TASK BREAKDOWN")
    print(f"{'='*70}")
    for r in results:
        print(f"  {r['task']:30s}  W:{r['whisper_tokens']:4d}  P:{r['python_tokens']:4d}  Δ:{r['reduction_pct']:+6.1f}%")

    # Save results
    output_path = os.path.join(args.output, "token_experiment.json")
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump({
            "fine_tuned_model": args.model,
            "base_model": args.base_model,
            "tasks": len(results),
            "summary": {
                "whisper_total_tokens": whisper_total,
                "python_total_tokens": python_total,
                "average_reduction_pct": avg_reduction,
                "whisper_avg_per_task": whisper_total / len(results),
                "python_avg_per_task": python_total / len(results),
            },
            "results": results,
        }, f, indent=2, ensure_ascii=False)

    print(f"\nResults saved to {output_path}")

    # Generate LaTeX table
    latex_path = os.path.join(args.output, "token_table.tex")
    with open(latex_path, "w", encoding="utf-8") as f:
        f.write("\\begin{table}[h]\n")
        f.write("\\centering\n")
        f.write("\\caption{Token Count: Fine-tuned Model Generated Code (Whisper vs Python)}\n")
        f.write("\\label{tab:token-comparison}\n")
        f.write("\\begin{tabular}{lrrr}\n")
        f.write("\\hline\n")
        f.write("Task & Whisper & Python & Reduction \\\\\n")
        f.write("\\hline\n")
        for r in results:
            f.write(f"{r['task']} & {r['whisper_tokens']} & {r['python_tokens']} & {r['reduction_pct']:+.1f}\\% \\\\\n")
        f.write("\\hline\n")
        f.write(f"\\textbf{{Total}} & \\textbf{{{whisper_total}}} & \\textbf{{{python_total}}} & \\textbf{{{avg_reduction:+.1f}\\%}} \\\\\n")
        f.write("\\hline\n")
        f.write("\\end{tabular}\n")
        f.write("\\end{table}\n")

    print(f"LaTeX table saved to {latex_path}")


if __name__ == "__main__":
    main()
