import re
def count_py(code):
    tokens = re.findall(
        r"[a-zA-Z_]\w*|\d+\.?\d*|[+\-*/%=<>!&|^~@(){}\[\]:;.,]|"
        r'"[^"]*"|' r"'[^']*'|"
        r"==|!=|<=|>=|\*\*|//|<<|>>",
        code
    )
    return len(tokens), tokens

tests = [
    "print(abs(-7))",
    "print(max(3,7))",
    "print(3+4)",
    "print('Hello, World!')",
    "print(2**10)",
    "import math; print(math.factorial(5))",
    "a,b=0,1\nfor _ in[0]*10:a,b=b,a+b\nprint(a)",
]

for code in tests:
    n, toks = count_py(code)
    print(f"{n:>4}t  {code[:60]}")
    print(f"       {toks}\n")
