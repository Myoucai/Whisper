#!/usr/bin/env python3
"""Experiment 2: Fair Token Comparison — hand-written code for both languages."""
import json, os

# Try Qwen tokenizer, fall back to regex approximation
print("Loading tokenizer...")
try:
    from transformers import AutoTokenizer
    tokenizer = AutoTokenizer.from_pretrained(
        "Qwen/Qwen2.5-Coder-7B-Instruct",
        trust_remote_code=True, local_files_only=True
    )
    def count_tokens(code, is_whisper):
        return len(tokenizer.encode(code, add_special_tokens=False))
    TOKENIZER = "Qwen2.5-Coder"
except Exception as e:
    print(f"  Tokenizer unavailable ({e}), using approximation")
    import re
    def count_tokens(code, is_whisper):
        if is_whisper:
            return len(code.split())
        return len(re.findall(
            r"[a-zA-Z_]\w*|-\d+\.?\d*|\d+\.?\d*|'[^']*'|\"[^\"]*\"|"
            r"\*\*|//|<<|>>|==|!=|<=|>=|"
            r"[+\-*/%<>=!&|^~@(){}\[\]:;.,]",
            code))
    TOKENIZER = "regex-approx"

# Hand-written reference code for both languages
TASKS = [
    ("hello_world",   '"Hello, World!" .',                    "print('Hello, World!')"),
    ("add",           "3 4 + .",                              "print(3+4)"),
    ("square",        "5 _ * .",                              "print(5**2)"),
    ("abs_val",       "-7 abs .",                             "print(abs(-7))"),
    ("max2",          "3 7 max .",                            "print(max(3,7))"),
    ("min2",          "3 7 min .",                            "print(min(3,7))"),
    ("factorial",     "5 factorial .",                        "import math\nprint(math.factorial(5))"),
    ("fibonacci",     "10 fib .",                             "a,b=0,1\nfor _ in[0]*10:a,b=b,a+b\nprint(a)"),
    ("sum_list",      "[1 2 3 4 5] sum .",                   "print(sum([1,2,3,4,5]))"),
    ("prod_list",     "[2 3 4 5] prod .",                     "import math\nprint(math.prod([2,3,4,5]))"),
    ("map_sq",        "[1 2 3 4 5] { _ * } @map .",          "print([x**2 for x in[1,2,3,4,5]])"),
    ("str_len",       '"Hello" strlen .',                     "print(len('Hello'))"),
    ("str_cat",       '"Hello" "World" strcat .',             "print('Hello'+'World')"),
    ("list_len",      "[10 20 30 40] len .",                  "print(len([10,20,30,40]))"),
    ("is_even",       "8 2 % 0 = .",                          "print(8%2==0)"),
    ("power",         "import std/math\n2 10 pow .",          "print(2**10)"),
    ("range_sum",     "import std/list\n10 range-to sum .",   "print(sum(range(1,11)))"),
    ("rev_list",      "[1 2 3 4 5] rev .",                   "print([*reversed([1,2,3,4,5])])"),
    ("str_has",       '"hello world" "lo" has? .',            "print('lo'in'hello world')"),
    ("last_elem",     "[10 20 30 40] last .",                "print([10,20,30,40][-1])"),
    ("str_rev",       '"hello" strrev .',                     "print('hello'[::-1])"),
    ("countdown",     "5 { _ 0 > } { _ . 1 - } #",           "for i in range(5,0,-1):print(i)"),
    ("negate",        "42 neg .",                             "print(-42)"),
    ("is_prime",      "import std/math\n17 prime? .",         "n=17\nprint(all(n%i for i in range(2,int(n**.5)+1))and n>1)"),
    ("gcd",           "48 18 gcd .",                         "import math\nprint(math.gcd(48,18))"),
]

print(f"\n{'='*70}")
print(f"Token Comparison ({TOKENIZER})")
print(f"{'='*70}")
print(f"{'Task':<18} {'Whisper':>7} {'Python':>7} {'W vs P':>8} {'Winner':>10}")
print("-" * 70)

total_w, total_p, wins_w, wins_p = 0, 0, 0, 0
details = []

for name, w_code, p_code in TASKS:
    w_tok = count_tokens(w_code, True)
    p_tok = count_tokens(p_code, False)
    total_w += w_tok
    total_p += p_tok
    reduction = (1 - w_tok/p_tok) * 100
    if reduction > 0:
        wins_w += 1; winner = "Whisper"
    else:
        wins_p += 1; winner = "Python"
    print(f"{name:<18} {w_tok:>7} {p_tok:>7} {reduction:>+7.1f}% {winner:>10}")
    details.append({"task": name, "whisper_tokens": w_tok, "python_tokens": p_tok,
                    "whisper_code": w_code, "python_code": p_code})

total_reduction = (1 - total_w/total_p) * 100
print("-" * 70)
print(f"{'TOTAL':<18} {total_w:>7} {total_p:>7} {total_reduction:>+7.1f}%")
print(f"\nWhisper wins {wins_w}/{len(TASKS)} tasks  |  Python wins {wins_p}/{len(TASKS)}")
print(f"Token reduction: {total_reduction:+.1f}%")

output = {
    "experiment": "exp2_fair_token",
    "tokenizer": TOKENIZER,
    "tasks": len(TASKS),
    "whisper_total": total_w, "python_total": total_p,
    "reduction_pct": total_reduction,
    "whisper_wins": wins_w, "python_wins": wins_p,
    "details": details,
}
os.makedirs("../results", exist_ok=True)
with open("../results/exp2_fair_token.json", "w") as f:
    json.dump(output, f, indent=2, ensure_ascii=False)
print(f"\nResults saved to ../results/exp2_fair_token.json")
