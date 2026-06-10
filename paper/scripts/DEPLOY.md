# Whisper 微调部署指南 — AutoDL 算力平台

## 环境配置

### 1. 创建实例

- **GPU**: RTX A6000 48GB / RTX 6000 Ada 48GB / A40 48GB
- **镜像**: PyTorch 2.x + CUDA 12.x + Python 3.10
- **磁盘**: >= 50GB

### 2. 安装依赖

```bash
pip install transformers peft bitsandbytes datasets accelerate trl tabulate
```

### 3. 上传数据

```bash
# 从本地上传 paper/ 目录到服务器
# AutoDL 支持 Jupyter 文件上传或 scp
scp -r paper/ root@<autodl-ip>:~/whisper-paper/
```

## 微调步骤

### Step 1: 微调模型

```bash
cd ~/whisper-paper
python scripts/finetune.py \
    --model Qwen/Qwen2.5-Coder-7B-Instruct \
    --data data/train.jsonl \
    --output ./whisper-qwen-7b \
    --epochs 5 \
    --batch-size 8 \
    --grad-accum 2 \
    --lr 1e-4 \
    --lora-r 128 \
    --lora-alpha 256 \
    --max-seq-len 2048 \
    --quant 8bit
```

**参数说明（48GB 显存优化）**：

| 参数 | 值 | 说明 |
|------|-----|------|
| `--quant 8bit` | 8bit 量化 | 比 4bit 质量更好，48GB 足够 |
| `--batch-size 8` | 批次大小 | 48GB 可以开大 |
| `--grad-accum 2` | 梯度累积 | 有效 batch = 8×2 = 16 |
| `--lora-r 128` | LoRA rank | 更高 = 更多参数 = 更好质量 |
| `--lora-alpha 256` | LoRA alpha | 通常 = 2× rank |
| `--max-seq-len 2048` | 最大序列长度 | 覆盖大部分代码 |
| `--epochs 5` | 训练轮数 | 214 条数据，5 轮够了 |

**预计时间**: RTX A6000 约 20-40 分钟

### Step 2: Token 对比实验（微调后）

用微调后的模型分别生成 Whisper 和 Python 代码，对比 token 数：

```bash
python scripts/token_experiment.py \
    --model ./whisper-qwen-7b \
    --base-model Qwen/Qwen2.5-Coder-7B-Instruct \
    --data data/benchmark.jsonl \
    --output results/
```

输出：
- 每个任务的 Whisper vs Python token 对比
- 平均 token 减少百分比
- LaTeX 表格 `results/token_table.tex`

### Step 3: 评估代码生成质量

```bash
# 微调前（基线）
python scripts/evaluate.py \
    --model Qwen/Qwen2.5-Coder-7B-Instruct \
    --data data/eval.jsonl \
    --output results/eval_base.json

# 微调后
python scripts/evaluate.py \
    --model ./whisper-qwen-7b \
    --data data/eval.jsonl \
    --output results/eval_finetuned.json
```

## AutoDL 费用估算

| GPU | 单价 | 微调时间 | 总费用 |
|-----|------|---------|--------|
| RTX A6000 48GB | ¥3-4/时 | ~30 分钟 | ¥1.5-2 |
| A40 48GB | ¥4-5/时 | ~30 分钟 | ¥2-2.5 |
| A100 80GB | ¥8-12/时 | ~15 分钟 | ¥2-3 |

**推荐**: RTX A6000 48GB，性价比最高。

## 输出文件

```
whisper-qwen-7b/
├── adapter_model.bin      ← LoRA 权重（~200MB）
├── adapter_config.json    ← LoRA 配置
├── tokenizer.json         ← 分词器
├── train_config.json      ← 训练参数记录
└── checkpoint-*/          ← 训练中间检查点
```

## 常见问题

**Q: OOM (显存不足)**
```bash
# 降低 batch size 或用 4bit 量化
python finetune.py --batch-size 4 --quant 4bit ...
```

**Q: 训练不收敛**
```bash
# 降低学习率，增加 epochs
python finetune.py --lr 5e-5 --epochs 10 ...
```

**Q: 想合并 LoRA 权重到基础模型**
```bash
pip install peft
python -c "
from peft import PeftModel
from transformers import AutoModelForCausalLM, AutoTokenizer
model = AutoModelForCausalLM.from_pretrained('Qwen/Qwen2.5-Coder-7B-Instruct', torch_dtype='bfloat16')
model = PeftModel.from_pretrained(model, './whisper-qwen-7b')
model = model.merge_and_unload()
model.save_pretrained('./whisper-qwen-7b-merged')
AutoTokenizer.from_pretrained('./whisper-qwen-7b').save_pretrained('./whisper-qwen-7b-merged')
"
```
