#!/usr/bin/env python3
"""
Fine-tune Qwen-Coder-7B on Whisper code generation.

Optimized for: 48GB VRAM (RTX A6000 / RTX 6000 Ada / A40) on AutoDL.

Usage:
    # AutoDL 一键环境
    pip install transformers peft bitsandbytes datasets accelerate trl

    # 运行微调
    python finetune.py --data ../data/train.jsonl --output ./whisper-qwen-7b
"""

import argparse
import json
import os
import torch
from datasets import Dataset
from transformers import (
    AutoModelForCausalLM,
    AutoTokenizer,
    BitsAndBytesConfig,
    TrainingArguments,
)
from peft import LoraConfig, get_peft_model, prepare_model_for_kbit_training
from trl import SFTTrainer


def load_dataset(path: str) -> Dataset:
    """Load JSONL training data and format as instruction-following pairs."""
    records = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            instruction = rec.get("instruction", "")
            input_data = rec.get("input", "")
            output = rec.get("whisper", "")

            if input_data:
                user_msg = f"{instruction}\n\nInput: {input_data}"
            else:
                user_msg = instruction

            messages = [
                {
                    "role": "system",
                    "content": (
                        "You are a Whisper programming language expert. "
                        "Whisper is an AI-native, stack-based, postfix language. "
                        "Write only Whisper code unless asked otherwise. "
                        "Use minimal tokens: _ for dup, ` for swap, @ for rot, "
                        "?? for conditional, # for loop, : for definition."
                    ),
                },
                {"role": "user", "content": user_msg},
                {"role": "assistant", "content": output},
            ]
            records.append({"messages": messages})

    return Dataset.from_list(records)


def main():
    parser = argparse.ArgumentParser(description="Fine-tune Qwen-Coder-7B on Whisper")
    parser.add_argument("--model", default="Qwen/Qwen2.5-Coder-7B-Instruct", help="Base model")
    parser.add_argument("--data", default="../data/train.jsonl", help="Training JSONL")
    parser.add_argument("--output", default="./whisper-qwen-7b", help="Output dir")
    parser.add_argument("--epochs", type=int, default=5, help="Training epochs")
    parser.add_argument("--batch-size", type=int, default=8, help="Per-device batch size")
    parser.add_argument("--grad-accum", type=int, default=2, help="Gradient accumulation steps")
    parser.add_argument("--lr", type=float, default=1e-4, help="Learning rate")
    parser.add_argument("--lora-r", type=int, default=128, help="LoRA rank")
    parser.add_argument("--lora-alpha", type=int, default=256, help="LoRA alpha")
    parser.add_argument("--max-seq-len", type=int, default=2048, help="Max sequence length")
    parser.add_argument("--quant", default="8bit", choices=["4bit", "8bit", "none"],
                        help="Quantization mode (8bit recommended for 48GB)")
    args = parser.parse_args()

    print(f"Model: {args.model}")
    print(f"Data: {args.data}")
    print(f"Output: {args.output}")
    print(f"Quantization: {args.quant}")
    print(f"Batch: {args.batch_size} x {args.grad_accum} = {args.batch_size * args.grad_accum}")
    print(f"LoRA: r={args.lora_r}, alpha={args.lora_alpha}")
    print()

    # Quantization config
    if args.quant == "4bit":
        bnb_config = BitsAndBytesConfig(
            load_in_4bit=True,
            bnb_4bit_quant_type="nf4",
            bnb_4bit_compute_dtype=torch.bfloat16,
            bnb_4bit_use_double_quant=True,
        )
    elif args.quant == "8bit":
        bnb_config = BitsAndBytesConfig(
            load_in_8bit=True,
        )
    else:
        bnb_config = None

    # Load tokenizer
    tokenizer = AutoTokenizer.from_pretrained(args.model, trust_remote_code=True)
    if tokenizer.pad_token is None:
        tokenizer.pad_token = tokenizer.eos_token

    # Load model
    print("Loading model...")
    model_kwargs = {
        "trust_remote_code": True,
        "torch_dtype": torch.bfloat16,
        "device_map": "auto",
    }
    if bnb_config:
        model_kwargs["quantization_config"] = bnb_config

    model = AutoModelForCausalLM.from_pretrained(args.model, **model_kwargs)

    if bnb_config:
        model = prepare_model_for_kbit_training(model)

    # LoRA config — target all linear layers for best quality
    lora_config = LoraConfig(
        r=args.lora_r,
        lora_alpha=args.lora_alpha,
        target_modules=[
            "q_proj", "k_proj", "v_proj", "o_proj",
            "gate_proj", "up_proj", "down_proj",
        ],
        lora_dropout=0.05,
        bias="none",
        task_type="CAUSAL_LM",
    )
    model = get_peft_model(model, lora_config)
    model.print_trainable_parameters()

    # Load dataset
    print(f"Loading dataset: {args.data}")
    dataset = load_dataset(args.data)
    print(f"Loaded {len(dataset)} examples")

    # Training arguments — optimized for 48GB VRAM
    training_args = TrainingArguments(
        output_dir=args.output,
        num_train_epochs=args.epochs,
        per_device_train_batch_size=args.batch_size,
        gradient_accumulation_steps=args.grad_accum,
        learning_rate=args.lr,
        weight_decay=0.01,
        warmup_ratio=0.1,
        lr_scheduler_type="cosine",
        logging_steps=5,
        save_strategy="epoch",
        save_total_limit=3,
        bf16=True,
        dataloader_pin_memory=False,
        report_to="none",
        max_grad_norm=1.0,
        gradient_checkpointing=True,  # Save VRAM
        optim="paged_adamw_8bit",     # Memory-efficient optimizer
    )

    # Trainer
    trainer = SFTTrainer(
        model=model,
        args=training_args,
        train_dataset=dataset,
        tokenizer=tokenizer,
        max_seq_length=args.max_seq_len,
    )

    # Train
    print("\nStarting training...")
    trainer.train()

    # Save
    print(f"\nSaving to {args.output}")
    trainer.save_model(args.output)
    tokenizer.save_pretrained(args.output)

    # Save training config
    config = {
        "base_model": args.model,
        "quantization": args.quant,
        "lora_r": args.lora_r,
        "lora_alpha": args.lora_alpha,
        "epochs": args.epochs,
        "batch_size": args.batch_size,
        "learning_rate": args.lr,
        "max_seq_len": args.max_seq_len,
        "training_examples": len(dataset),
    }
    with open(os.path.join(args.output, "train_config.json"), "w") as f:
        json.dump(config, f, indent=2)

    print("Done!")


if __name__ == "__main__":
    main()
