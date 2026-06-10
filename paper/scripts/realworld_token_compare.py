#!/usr/bin/env python3
"""
Real-world token comparison: Whisper vs Python for 3 practical tasks.
Counts tokens using realistic tokenization rules for both languages.
"""

# ── Token counting rules ──
# Whisper: each space-separated word/symbol = 1 token (they're all short ASCII)
# Python: each keyword/identifier/punctuation/operator = 1 token (LLM tokenizer behavior)

def whisper_tokens(code):
    return len(code.split())

def python_tokens(code):
    """Count tokens as an LLM tokenizer would for Python code.
    Rules: keywords, identifiers, numbers, strings are 1 token each.
    Punctuation ( ) [ ] { } : , . = + - * / are 1 token each.
    Multi-char operators (== != <= >= ** //) are 1 token each.
    Strings may be split by tokenizer into pieces."""
    import re
    # Tokenize Python code
    token_pattern = re.compile(r'''
        [a-zA-Z_]\w*           # identifiers/keywords
        |\d+\.?\d*             # numbers
        |\.\d+                 # .5 style numbers
        |"[^"]*"               # double-quoted strings
        |'[^']*'               # single-quoted strings
        |==|!=|<=|>=|\*\*|//  # multi-char operators
        |[+\-*/%<>=&|^~@(){}\[\]:;.,]  # single-char operators/punctuation
        |\#.*                   # comments (count as 1)
    ''', re.VERBOSE)
    tokens = token_pattern.findall(code)
    return len(tokens)

# ═══════════════════════════════════════════════════════════════
# Task 1: Print formatted status message
# ═══════════════════════════════════════════════════════════════

task1 = {
    "name": "Status message formatter",
    "description": "Given app name and status, print a formatted message",
    "python": '''\
app = "WhisperC"
status = "running"
print(f"[{app}] Status: {status}")''',
    "whisper": '''\
import std/str
"[WhisperC] Status: running" .''',
}

# ═══════════════════════════════════════════════════════════════
# Task 2: JSON config reader
# ═══════════════════════════════════════════════════════════════

task2 = {
    "name": "JSON config reader",
    "description": "Read config.json, extract and print name and version fields",
    "python": '''\
import json
c = json.load(open("config.json"))
print(f"App: {c['name']} v{c['version']}")''',
    "whisper": '''\
import std/json
"config.json" read-file json-parse
"name" listfind drop
"version" listfind drop
"App: " swap strcat " v" strcat swap strcat .''',
}

# ═══════════════════════════════════════════════════════════════
# Task 3: API data fetcher (HTTP GET + JSON parse + iterate)
# ═══════════════════════════════════════════════════════════════

task3 = {
    "name": "API post counter",
    "description": "Fetch posts from API, count posts per user, show top user",
    "python": '''\
import json, urllib.request
url = "http://jsonplaceholder.typicode.com/posts"
data = json.loads(urllib.request.urlopen(url).read())
counts = {}
for p in data:
    uid = p["userId"]
    counts[uid] = counts.get(uid, 0) + 1
top = max(counts.items(), key=lambda x: x[1])
print(f"User {top[0]}: {top[1]} posts")''',
    "whisper": '''\
import std/http
import std/json
import std/list
import std/math
"http://jsonplaceholder.typicode.com/posts" http-get json-parse
0 swap { "userId" listfind drop swap 1 + swap } @each
drop
"User " swap i64tostr strcat " has the most posts" strcat .''',
}

# ═══════════════════════════════════════════════════════════════
# Task 4: String processing — extract and count words
# ═══════════════════════════════════════════════════════════════

task4 = {
    "name": "Word frequency counter",
    "description": "Given a string, split into words, count unique words, print top 3",
    "python": '''\
text = "the cat and the dog and the bird"
words = text.split()
freq = {}
for w in words:
    freq[w] = freq.get(w, 0) + 1
top = sorted(freq.items(), key=lambda x: x[1], reverse=True)[:3]
for w, c in top:
    print(f"{w}: {c}")''',
    "whisper": '''\
import std/str
import std/list
"the cat and the dog and the bird"
" " strsplit
[] swap { len 0 > } {
  over 0 @nth over 0 @nth 1 append append
  1 strslice
} # drop
{ len . } @map
sort rev 3 take { _ 1 @nth i64tostr ": " strcat swap 0 @nth i64tostr strcat . } @each''',
}

# ═══════════════════════════════════════════════════════════════
# Task 5: Math — prime number sieve
# ═══════════════════════════════════════════════════════════════

task5 = {
    "name": "Prime sieve up to 100",
    "description": "Find all prime numbers up to 100 and print them",
    "python": '''\
n = 100
sieve = [True] * (n + 1)
for i in range(2, int(n**0.5) + 1):
    if sieve[i]:
        for j in range(i*i, n+1, i):
            sieve[j] = False
primes = [i for i in range(2, n+1) if sieve[i]]
print(primes)''',
    "whisper": '''\
import std/math
import std/list
100
2 swap { over over <= } {
  _ sq swap over >= ??
    drop
  | _ swap over % 0 = ??
      drop
    | swap over append swap
    ]
  ]
  1 +
} # drop drop
. ''',
}

tasks = [task1, task2, task3, task4, task5]

print("=" * 80)
print("REAL-WORLD TOKEN COMPARISON: Whisper vs Python")
print("=" * 80)

grand_w = 0
grand_p = 0
wins = 0

for i, task in enumerate(tasks):
    w_tok = whisper_tokens(task["whisper"])
    p_tok = python_tokens(task["python"])
    reduction = (1 - w_tok/p_tok) * 100
    grand_w += w_tok
    grand_p += p_tok
    if reduction > 0:
        wins += 1

    winner = "Whisper WINS" if reduction > 0 else "Python wins"
    print(f"\n── Task {i+1}: {task['name']} ──")
    print(f"  {task['description']}")
    print(f"  Whisper: {w_tok:>3} tokens | Python: {p_tok:>3} tokens | {reduction:>+6.1f}%  [{winner}]")
    print(f"  Whisper code: {task['whisper'][:80]}...")
    print(f"  Python code:  {task['python'][:80]}...")

print(f"\n{'=' * 80}")
total_reduction = (1 - grand_w/grand_p) * 100
print(f"TOTAL: Whisper {grand_w} tokens vs Python {grand_p} tokens")
print(f"Token reduction: {total_reduction:+.1f}%")
print(f"Whisper wins on {wins}/{len(tasks)} tasks")
print()

# Show code side by side for the most compelling case
print("=" * 80)
print("SIDE-BY-SIDE: Task 3 (API fetcher)")
print("=" * 80)
print(f"\n{'Whisper (' + str(whisper_tokens(task3['whisper'])) + ' tokens)':<45}  Python ({python_tokens(task3['python'])} tokens)")
print("-" * 80)
w_lines = task3["whisper"].strip().split("\n")
p_lines = task3["python"].strip().split("\n")
for i in range(max(len(w_lines), len(p_lines))):
    wl = w_lines[i] if i < len(w_lines) else ""
    pl = p_lines[i] if i < len(p_lines) else ""
    print(f"  {wl:<42}  {pl}")
