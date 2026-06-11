#!/usr/bin/env python3
"""Train a model on given data and evaluate it."""
import sys, os, subprocess

def run(cmd):
    print(f"  $ {cmd[:100]}...")
    return subprocess.run(cmd, shell=True, check=True)

def main():
    if len(sys.argv) < 3:
        print("Usage: python train_and_eval.py <data_path> <output_dir>")
        print("Example: python train_and_eval.py ../data/train_250.jsonl ./whisper-qwen-250")
        sys.exit(1)

    data_path = sys.argv[1]
    output_dir = sys.argv[2]
    tag = output_dir.split('-')[-1]  # e.g. "250", "no-stdlib"

    print(f"\n{'='*60}")
    print(f"Training: {data_path} -> {output_dir}")
    print(f"{'='*60}")

    # Train
    cmd = (
        f"python finetune.py"
        f" --model Qwen/Qwen2.5-Coder-7B-Instruct"
        f" --data {data_path}"
        f" --output {output_dir}"
        f" --epochs 5 --batch-size 8 --grad-accum 2"
        f" --lr 1e-4 --lora-r 128 --lora-alpha 256"
        f" --max-seq-len 2048 --quant 8bit"
    )
    run(cmd)

    # Evaluate
    result_path = f"../results/eval_{tag}.json"
    print(f"\n{'='*60}")
    print(f"Evaluating: {output_dir} -> {result_path}")
    print(f"{'='*60}")

    cmd = (
        f"python evaluate.py"
        f" --model {output_dir}"
        f" --data ../data/eval.jsonl"
        f" --output {result_path}"
    )
    run(cmd)

    print(f"\nDone. Results: {result_path}")

if __name__ == "__main__":
    main()
