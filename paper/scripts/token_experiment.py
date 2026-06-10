#!/usr/bin/env python3
"""
Token Reduction Experiment: Whisper vs Python vs JavaScript vs Java.

Measures token counts for equivalent programs across languages using
the same tokenizer (Qwen-Coder-7B's tokenizer).

Usage:
    pip install transformers tabulate
    python token_experiment.py --model Qwen/Qwen2.5-Coder-7B-Instruct --data ../data/benchmark.jsonl --output ../results/
"""

import argparse
import json
import os
from collections import defaultdict
from tabulate import tabulate


def count_tokens(tokenizer, text: str) -> int:
    """Count tokens for a given text using the specified tokenizer."""
    return len(tokenizer.encode(text, add_special_tokens=False))


def main():
    parser = argparse.ArgumentParser(description="Token reduction experiment")
    parser.add_argument("--model", default="Qwen/Qwen2.5-Coder-7B-Instruct", help="Tokenizer model")
    parser.add_argument("--data", default="../data/benchmark.jsonl", help="Benchmark data")
    parser.add_argument("--output", default="../results", help="Output directory")
    args = parser.parse_args()

    # Load tokenizer
    print(f"Loading tokenizer: {args.model}")
    from transformers import AutoTokenizer
    tokenizer = AutoTokenizer.from_pretrained(args.model, trust_remote_code=True)

    # Load benchmark data
    print(f"Loading benchmark: {args.data}")
    tasks = []
    with open(args.data, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                tasks.append(json.loads(line))

    print(f"Loaded {len(tasks)} benchmark tasks\n")

    # Count tokens for each task and language
    results = []
    totals = defaultdict(int)
    lang_counts = defaultdict(list)

    for task in tasks:
        task_name = task["task"]
        row = {"task": task_name}

        for lang in ["whisper", "python", "javascript", "java"]:
            code = task.get(lang, "")
            if code:
                tok_count = count_tokens(tokenizer, code)
                row[lang] = tok_count
                totals[lang] += tok_count
                lang_counts[lang].append(tok_count)
            else:
                row[lang] = None

        results.append(row)

    # Print detailed results
    headers = ["Task", "Whisper", "Python", "JavaScript", "Java"]
    table_data = []
    for r in results:
        row = [r["task"]]
        whisper_tok = r.get("whisper")
        for lang in ["whisper", "python", "javascript", "java"]:
            tok = r.get(lang)
            if tok is not None:
                if lang != "whisper" and whisper_tok:
                    reduction = (1 - whisper_tok / tok) * 100
                    row.append(f"{tok} ({reduction:+.0f}%)")
                else:
                    row.append(str(tok))
            else:
                row.append("-")
        table_data.append(row)

    print("=" * 80)
    print("TOKEN COUNT COMPARISON")
    print("=" * 80)
    print(tabulate(table_data, headers=headers, tablefmt="grid"))

    # Summary statistics
    print("\n" + "=" * 80)
    print("SUMMARY STATISTICS")
    print("=" * 80)

    summary_table = []
    for lang in ["whisper", "python", "javascript", "java"]:
        counts = lang_counts.get(lang, [])
        if counts:
            total = sum(counts)
            avg = total / len(counts)
            reduction = ""
            if lang != "whisper":
                whisper_avg = sum(lang_counts["whisper"]) / len(lang_counts["whisper"])
                pct = (1 - whisper_avg / avg) * 100
                reduction = f"{pct:.1f}% fewer"
            summary_table.append([lang, total, f"{avg:.1f}", min(counts), max(counts), reduction])

    print(tabulate(summary_table,
                   headers=["Language", "Total Tokens", "Avg/Task", "Min", "Max", "Reduction vs Whisper"],
                   tablefmt="grid"))

    # Pairwise comparisons
    print("\n" + "=" * 80)
    print("PAIRWISE TOKEN REDUCTION (Whisper vs X)")
    print("=" * 80)

    pairwise = []
    whisper_counts = lang_counts.get("whisper", [])
    for lang in ["python", "javascript", "java"]:
        other_counts = lang_counts.get(lang, [])
        if whisper_counts and other_counts:
            # Calculate per-task reduction
            reductions = []
            for w, o in zip(whisper_counts, other_counts):
                if o > 0:
                    reductions.append((1 - w / o) * 100)

            if reductions:
                avg_reduction = sum(reductions) / len(reductions)
                min_reduction = min(reductions)
                max_reduction = max(reductions)
                pairwise.append([
                    f"Whisper vs {lang}",
                    f"{avg_reduction:.1f}%",
                    f"{min_reduction:.1f}%",
                    f"{max_reduction:.1f}%",
                    len(reductions),
                ])

    print(tabulate(pairwise,
                   headers=["Comparison", "Avg Reduction", "Min", "Max", "Tasks"],
                   tablefmt="grid"))

    # Save results to JSON
    os.makedirs(args.output, exist_ok=True)
    output_path = os.path.join(args.output, "token_experiment.json")
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump({
            "model": args.model,
            "tasks": len(tasks),
            "results": results,
            "summary": {
                lang: {
                    "total": totals[lang],
                    "avg": totals[lang] / len(lang_counts[lang]) if lang_counts[lang] else 0,
                    "min": min(lang_counts[lang]) if lang_counts[lang] else 0,
                    "max": max(lang_counts[lang]) if lang_counts[lang] else 0,
                }
                for lang in ["whisper", "python", "javascript", "java"]
            },
        }, f, indent=2, ensure_ascii=False)

    print(f"\nResults saved to {output_path}")

    # Generate LaTeX table for paper
    latex_path = os.path.join(args.output, "token_table.tex")
    with open(latex_path, "w", encoding="utf-8") as f:
        f.write("\\begin{table}[h]\n")
        f.write("\\centering\n")
        f.write("\\caption{Token Count Comparison: Whisper vs Popular Languages}\n")
        f.write("\\label{tab:token-comparison}\n")
        f.write("\\begin{tabular}{lrrrr}\n")
        f.write("\\hline\n")
        f.write("Task & Whisper & Python & JavaScript & Java \\\\\n")
        f.write("\\hline\n")
        for r in results:
            row = f"{r['task']}"
            for lang in ["whisper", "python", "javascript", "java"]:
                tok = r.get(lang)
                row += f" & {tok if tok else '-'}"
            f.write(row + " \\\\\n")
        f.write("\\hline\n")
        f.write("\\end{tabular}\n")
        f.write("\\end{table}\n")

    print(f"LaTeX table saved to {latex_path}")


if __name__ == "__main__":
    main()
