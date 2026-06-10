#!/usr/bin/env python3
"""
Fine-tune Qwen-Coder-7B on Whisper code generation.

Usage:
    pip install transformers peft bitsandbytes datasets accelerate
    python finetune.py --data ../data/train.jsonl --output ./whisper-qwen-7b

Cloud GPU (e.g. AutoDL, Lambda, RunPod):
    1. Rent a GPU with >= 24GB VRAM (RTX 4090, A100, etc.)
    2. pip install transformers peft bitsandbytes datasets accelerate
    3. python finetune.py --data ../data/train.jsonl --output ./whisper-qwen-7b
"""

import argparse
import json
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

            # Format as chat-style prompt for Qwen
            if input_data:
                user_msg = f"{instruction}\n\nInput: {input_data}"
            else:
                user_msg = instruction

            messages = [
                {"role": "system", "content": "You are a Whisper programming language expert. Whisper is an AI-native, stack-based, postfix language. Write only Whisper code unless asked otherwise."},
                {"role": "user", "content": user_msg},
                {"role": "assistant", "content": output},
            ]
            records.append({"messages": messages})

    return Dataset.from_list(records)


def main():
    parser = argparse.ArgumentParser(description="Fine-tune Qwen-Coder-7B on Whisper")
    parser.add_argument("--model", default="Qwen/Qwen2.5-Coder-7B-Instruct", help="Base model name or path")
    parser.add_argument("--data", default="../data/train.jsonl", help="Training data JSONL path")
    parser.add_argument("--output", default="./whisper-qwen-7b", help="Output directory")
    parser.add_argument("--epochs", type=int, default=3, help="Number of training epochs")
    parser.add_argument("--batch-size", type=int, default=4, help="Per-device batch size")
    parser.add_argument("--lr", type=float, default=2e-4, help="Learning rate")
    parser.add_argument("--lora-r", type=int, default=64, help="LoRA rank")
    parser.add_argument("--lora-alpha", type=int, default=128, help="LoRA alpha")
    parser.add_argument("--max-seq-len", type=int, default=1024, help="Max sequence length")
    args = parser.parse_args()

    print(f"Loading model: {args.model}")

    # 4-bit quantization config
    bnb_config = BitsAndBytesConfig(
        load_in_4bit=True,
        bnb_4bit_quant_type="nf4",
        bnb_4bit_compute_dtype=torch.bfloat16,
        bnb_4bit_use_double_quant=True,
    )

    # Load tokenizer
    tokenizer = AutoTokenizer.from_pretrained(args.model, trust_remote_code=True)
    if tokenizer.pad_token is None:
        tokenizer.pad_token = tokenizer.eos_token

    # Load model
    model = AutoModelForCausalLM.from_pretrained(
        args.model,
        quantization_config=bnb_config,
        device_map="auto",
        trust_remote_code=True,
        torch_dtype=torch.bfloat16,
    )
    model = prepare_model_for_kbit_training(model)

    # LoRA config
    lora_config = LoraConfig(
        r=args.lora_r,
        lora_alpha=args.lora_alpha,
        target_modules=["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj"],
        lora_dropout=0.05,
        bias="none",
        task_type="CAUSAL_LM",
    )
    model = get_peft_model(model, lora_config)
    model.print_trainable_parameters()

    # Load dataset
    print(f"Loading dataset: {args.data}")
    dataset = load_dataset(args.data)
    print(f"Loaded {len(dataset)} training examples")

    # Training arguments
    training_args = TrainingArguments(
        output_dir=args.output,
        num_train_epochs=args.epochs,
        per_device_train_batch_size=args.batch_size,
        gradient_accumulation_steps=4,
        learning_rate=args.lr,
        weight_decay=0.01,
        warmup_ratio=0.1,
        lr_scheduler_type="cosine",
        logging_steps=10,
        save_strategy="epoch",
        save_total_limit=2,
        bf16=True,
        dataloader_pin_memory=False,
        report_to="none",
        max_grad_norm=1.0,
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
    print("Starting training...")
    trainer.train()

    # Save
    print(f"Saving model to {args.output}")
    trainer.save_model(args.output)
    tokenizer.save_pretrained(args.output)
    print("Done!")


if __name__ == "__main__":
    main()
