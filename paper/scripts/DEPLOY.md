# Whisper 微调部署指南

## 环境要求

- **GPU**: >= 24GB VRAM (RTX 4090 / A100 / L40S)
- **RAM**: >= 32GB
- **Disk**: >= 50GB
- **Python**: 3.10+

## 一键安装

```bash
pip install transformers peft bitsandbytes datasets accelerate trl tabulate
```

## 微调步骤

### 1. 上传数据

```bash
# 将 paper/data/ 目录上传到服务器
scp -r paper/data/ user@server:~/whisper-paper/data/
```

### 2. 运行微调

```bash
cd ~/whisper-paper
python scripts/finetune.py \
    --model Qwen/Qwen2.5-Coder-7B-Instruct \
    --data data/train.jsonl \
    --output ./whisper-qwen-7b \
    --epochs 3 \
    --batch-size 4 \
    --lr 2e-4 \
    --lora-r 64 \
    --lora-alpha 128 \
    --max-seq-len 1024
```

**预计时间**: RTX 4090 约 30-60 分钟，A100 约 15-30 分钟。

### 3. 运行 Token 对比实验

```bash
python scripts/token_experiment.py \
    --model Qwen/Qwen2.5-Coder-7B-Instruct \
    --data data/benchmark.jsonl \
    --output results/
```

### 4. 评估微调模型

```bash
python scripts/evaluate.py \
    --model ./whisper-qwen-7b \
    --data data/eval.jsonl \
    --output results/eval.json
```

## 云端平台选择

| 平台 | GPU | 价格(约) | 备注 |
|------|-----|---------|------|
| AutoDL | RTX 4090 24GB | ¥2-3/时 | 国内，便宜 |
| AutoDL | A100 80GB | ¥8-12/时 | 国内，大模型 |
| Lambda | A100 80GB | $1.10/时 | 海外，稳定 |
| RunPod | RTX 4090 | $0.40/时 | 海外，按需 |
| Vast.ai | 各种 | $0.20-1/时 | 海外，最便宜 |

## 推荐配置

**最小配置**: RTX 4090 24GB + 32GB RAM
- batch_size=4, gradient_accumulation=4
- QLoRA 4bit
- max_seq_len=1024

**推荐配置**: A100 80GB
- batch_size=8, gradient_accumulation=2
- QLoRA 4bit 或 8bit
- max_seq_len=2048

## 输出文件

微调完成后:
- `whisper-qwen-7b/adapter_model.bin` — LoRA 权重
- `whisper-qwen-7b/adapter_config.json` — LoRA 配置
- `whisper-qwen-7b/tokenizer.json` — 分词器

## 合并权重 (可选)

```bash
python scripts/merge.py \
    --base Qwen/Qwen2.5-Coder-7B-Instruct \
    --adapter ./whisper-qwen-7b \
    --output ./whisper-qwen-7b-merged
```
