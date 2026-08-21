"""Per-def bracket balance for whisperc June sources (Rust-VM grammar).

Splits source into defs (single- or multi-line), strips comments/strings,
stack-simulates [ ?? { vs ] }. Reports each def whose stack is non-empty
(missing closers) or underflows (extra closers).
"""
import io
import re
import sys


def strip_line(line: str) -> str:
    line = re.sub(r'"(?:[^"\\]|\\.)*"', '""', line)
    return re.sub(r"//.*", "", line)


def split_defs(src: str):
    """Yield (name, start_line, body_text). Handles ': n { .. } ;' on any layout."""
    lines = src.split("\n")
    i = 0
    while i < len(lines):
        m = re.match(r"^:\s+(\S+)", lines[i])
        if not m:
            i += 1
            continue
        name, start = m.group(1), i + 1
        # def ends at first comment-stripped line ending in '} ;'
        # (comments like "// (: name {body} ;)" must not end a def)
        j = i
        while j < len(lines):
            if re.search(r"\}\s*;", strip_line(lines[j])):
                break
            j += 1
        yield name, start, "\n".join(lines[i : j + 1])
        i = j + 1


def balance(body: str):
    stack = []
    for ln, line in enumerate(body.split("\n"), 1):
        for tok in re.finditer(r"\?\?|\[|\]|\{|\}", strip_line(line)):
            t = tok.group(0)
            if t in ("]", "}"):
                if not stack:
                    return f"extra {t} at body line {ln}"
                stack.pop()
            else:
                stack.append((t, ln))
    if stack:
        kinds = "".join(k for k, _ in stack)
        return f"unclosed {kinds} (opened at body lines {[l for _, l in stack]})"
    return None


def run(paths):
    bad = 0
    for path in paths:
        src = io.open(path, encoding="utf-8").read()
        for name, start, body in split_defs(src):
            msg = balance(body)
            if msg:
                print(f"{path}:{start}: def '{name}': {msg}")
                bad += 1
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(run(sys.argv[1:]))
