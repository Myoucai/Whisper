#!/usr/bin/env python3
"""
Server-side rejection-sampling generation: sample k completions per
training instruction from the current fine-tuned adapter. Verification
(filtering by execution) happens locally afterwards — this script only
generates. Output: /root/autodl-tmp/results/reject_samples.jsonl
"""
import argparse
import json
import os
import torch
from transformers import (AutoModelForCausalLM, AutoTokenizer,
                          BitsAndBytesConfig)
from peft import PeftModel

SYSTEM = (
    "You are a Whisper programming language expert. "
    "Whisper is an AI-native, stack-based, postfix language. "
    "Write only Whisper code unless asked otherwise. "
    "Use minimal tokens: _ for dup, ` for swap, @ for rot, "
    "?? for conditional, # for loop, : for definition."
)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", required=True, help="LoRA adapter dir")
    ap.add_argument("--data", default="../data/train_v4.jsonl")
    ap.add_argument("--out", default="/root/autodl-tmp/results/reject_samples.jsonl")
    ap.add_argument("--k", type=int, default=3)
    ap.add_argument("--temperature", type=float, default=0.8)
    ap.add_argument("--max-tokens", type=int, default=128)
    ap.add_argument("--limit", type=int, default=0, help="0 = all")
    args = ap.parse_args()

    with open(args.model + "/adapter_config.json", encoding="utf-8") as f:
        base_name = json.load(f).get("base_model_name_or_path")
    print(f"base: {base_name}")

    bnb = BitsAndBytesConfig(
        load_in_4bit=True, bnb_4bit_compute_dtype=torch.bfloat16,
        bnb_4bit_quant_type="nf4", bnb_4bit_use_double_quant=True)
    base = AutoModelForCausalLM.from_pretrained(
        base_name, quantization_config=bnb, device_map={"": 0},
        trust_remote_code=True, torch_dtype=torch.bfloat16,
        local_files_only=True)
    model = PeftModel.from_pretrained(base, args.model)
    tokenizer = AutoTokenizer.from_pretrained(
        base_name, trust_remote_code=True, local_files_only=True)
    model.eval()

    with open(args.data, encoding="utf-8") as f:
        records = [json.loads(l) for l in f if l.strip()]
    if args.limit:
        records = records[:args.limit]
    print(f"sampling k={args.k} for {len(records)} instructions...")

    with open(args.out, "w", encoding="utf-8") as fout:
        for i, rec in enumerate(records):
            user_msg = rec["instruction"]
            if rec.get("input"):
                user_msg += f"\n\nInput: {rec['input']}"
            messages = [{"role": "system", "content": SYSTEM},
                        {"role": "user", "content": user_msg}]
            text = tokenizer.apply_chat_template(
                messages, tokenize=False, add_generation_prompt=True)
            inputs = tokenizer(text, return_tensors="pt").to(model.device)
            with torch.no_grad():
                out = model.generate(
                    **inputs, max_new_tokens=args.max_tokens,
                    temperature=args.temperature, top_p=0.95,
                    do_sample=True, num_return_sequences=args.k)
            prompt_len = inputs["input_ids"].shape[1]
            samples = []
            for seq in out:
                gen = tokenizer.decode(seq[prompt_len:],
                                       skip_special_tokens=True)
                if "</think>" in gen:
                    gen = gen.split("</think>", 1)[1]
                samples.append(gen.strip())
            fout.write(json.dumps(
                {"instruction": rec["instruction"], "input": rec.get("input", ""),
                 "reference_output": rec.get("output", ""),
                 "samples": samples}, ensure_ascii=False) + "\n")
            fout.flush()
            if (i + 1) % 25 == 0:
                print(f"  {i+1}/{len(records)}")
    print("done ->", args.out)


if __name__ == "__main__":
    main()
