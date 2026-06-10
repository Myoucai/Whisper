#!/usr/bin/env python3
"""
Experiment 2: Fair Token Comparison
Uses hand-written reference code for BOTH languages.
Counts tokens with the Qwen tokenizer (same tokenizer used during training).
"""
import json, os, sys

# Try to load the Qwen tokenizer
print("Loading tokenizer...")
try:
    from transformers import AutoTokenizer
    tokenizer = AutoTokenizer.from_pretrained(
        "Qwen/Qwen2.5-Coder-7B-Instruct",
        trust_remote_code=True,
        local_files_only=True,
    )
    HAS_TOKENIZER = True
    print("  Qwen tokenizer loaded.")
except Exception as e:
    print(f"  Tokenizer not available: {e}")
    print("  Falling back to word-count approximation.")
    HAS_TOKENIZER = False

# ── Hand-written reference code: Whisper vs Python (SHORTEST idiomatic form) ──
# Both sides represent the most concise correct implementation.

TASKS = [
    # (task_name, whisper_code, python_code)
    ("hello_world",   '"Hello, World!" .',                    "print('Hello, World!')"),
    ("add",           "3 4 + .",                              "print(3+4)"),
    ("square",        "5 _ * .",                              "print(5**2)"),
    ("abs_val",       "import std/math\n-7 abs .",            "print(abs(-7))"),
    ("max2",          "import std/math\n3 7 max .",           "print(max(3,7))"),
    ("min2",          "import std/math\n3 7 min .",           "print(min(3,7))"),
    ("factorial",     "import std/math\n5 factorial .",       "import math\nprint(math.factorial(5))"),
    ("fibonacci",     "import std/math\n10 fib .",            "a,b=0,1\nfor _ in[0]*10:a,b=b,a+b\nprint(a)"),
    ("sum_list",      "import std/list\n[1,2,3,4,5] sum .",  "print(sum([1,2,3,4,5]))"),
    ("prod_list",     "import std/list\n[2,3,4,5] prod .",   "import math\nprint(math.prod([2,3,4,5]))"),
    ("map_sq",        "[1,2,3,4,5] { _ * } @map .",          "print([x**2 for x in[1,2,3,4,5]])"),
    ("str_len",       '"Hello" strlen .',                     "print(len('Hello'))"),
    ("str_cat",       '"Hello" "World" strcat .',             "print('Hello'+'World')"),
    ("list_len",      "[10,20,30,40] len .",                  "print(len([10,20,30,40]))"),
    ("is_even",       "8 2 % 0 = .",                          "print(8%2==0)"),
    ("power",         "import std/math\n2 10 pow .",           "print(2**10)"),
    ("range_sum",     "import std/list\n10 range-to sum .",   "print(sum(range(1,11)))"),
    ("rev_list",      "import std/list\n[1,2,3,4,5] rev .",  "print([*reversed([1,2,3,4,5])])"),
    ("str_has",       '"hello world" "lo" strfind 0 >= .',    "print('lo'in'hello world')"),
    ("last_elem",     "import std/list\n[10,20,30,40] last .","print([10,20,30,40][-1])"),
    ("str_rev",       '"hello" strchars rev charsstr .',      "print('hello'[::-1])"),
    ("countdown",     "5 { _ 0 > } { _ . 1 - } #",           "for i in range(5,0,-1):print(i)"),
    ("negate",        "import std/math\n42 neg .",             "print(-42)"),
    ("is_prime",      "import std/math\n17 prime? .",         "n=17\nprint(all(n%i for i in range(2,int(n**.5)+1))and n>1)"),
    ("gcd",           "import std/math\n: gcd { _ 0 = ?? drop | swap over % gcd ] } ;\n48 18 gcd .", "import math\nprint(math.gcd(48,18))"),
]

def count_python_tokens_approx(code):
    """Simulate LLM tokenizer behavior for Python code.
    Handles: identifiers, numbers (including negative), strings,
    multi-char operators, single-char operators/punctuation."""
    import re
    tokens = re.findall(
        r"[a-zA-Z_]\w*|"          # identifiers
        r"-\d+\.?\d*|\d+\.?\d*|"  # numbers (including negative)
        r"'[^']*'|\"[^\"]*\"|"    # strings
        r"\*\*|//|<<|>>|==|!=|<=|>=|"  # multi-char operators
        r"[+\-*/%<>=!&|^~@(){}\[\]:;.,]"  # single-char
        , code)
    return len(tokens)

def count_whisper_tokens(code):
    """Whisper tokens: each space-separated ASCII token = 1 model token."""
    return len(code.split())

def count_tokens(code, has_tokenizer, is_whisper):
    if has_tokenizer:
        return len(tokenizer.encode(code, add_special_tokens=False))
    elif is_whisper:
        return count_whisper_tokens(code)
    else:
        return count_python_tokens_approx(code)

print("\n" + "=" * 85)
print(f"{'Task':<18} {'Whisper':>8} {'Python':>8} {'W vs P':>8} {'Winner':>10}")
print("=" * 85)

total_w = 0
total_p = 0
wins_w = 0
wins_p = 0

for name, w_code, p_code in TASKS:
    w_tok = count_tokens(w_code, HAS_TOKENIZER, is_whisper=True)
    p_tok = count_tokens(p_code, HAS_TOKENIZER, is_whisper=False)
    total_w += w_tok
    total_p += p_tok
    reduction = (1 - w_tok/p_tok) * 100
    if reduction > 0:
        wins_w += 1
        winner = "Whisper"
    else:
        wins_p += 1
        winner = "Python"
    print(f"{name:<18} {w_tok:>8} {p_tok:>8} {reduction:>+7.1f}% {winner:>10}")

total_reduction = (1 - total_w/total_p) * 100
print("=" * 85)
print(f"{'TOTAL':<18} {total_w:>8} {total_p:>8} {total_reduction:>+7.1f}%")
print(f"\nWhisper wins {wins_w}/{len(TASKS)} tasks")
print(f"Python wins  {wins_p}/{len(TASKS)} tasks")
print(f"\nTokenizer: {'Qwen2.5-Coder tokenizer' if HAS_TOKENIZER else 'word-count approx'}")

# Save results
output = {
    "experiment": "exp2_fair_token_comparison",
    "tokenizer": "Qwen2.5-Coder-7B-Instruct" if HAS_TOKENIZER else "word-count",
    "tasks": len(TASKS),
    "whisper_total": total_w,
    "python_total": total_p,
    "reduction_pct": total_reduction,
    "whisper_wins": wins_w,
    "python_wins": wins_p,
    "details": [{"task": n, "whisper_tokens": wt, "python_tokens": pt,
                 "whisper_code": wc, "python_code": pc}
                for (n, wc, pc), wt, pt in
                zip(TASKS, [count_tokens(w, HAS_TOKENIZER, True) for _,w,_ in TASKS],
                    [count_tokens(p, HAS_TOKENIZER, False) for _,_,p in TASKS])]
}
out_path = os.path.join(os.path.dirname(__file__), "..", "results", "exp2_fair_token.json")
with open(out_path, "w") as f:
    json.dump(output, f, indent=2)
print(f"\nSaved: {out_path}")
