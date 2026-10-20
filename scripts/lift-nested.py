#!/usr/bin/env python3
"""Lift GNU C nested functions emitted for Whisper quote bodies.

The c2/cgen emitter wraps every higher-order quote body in

    { auto void _qb(void) { <body> } <wrapper using _qb> }

GCC accepts this extension; clang/Zig (and therefore wasm32-wasi) do not.
This transformer moves each `_qb` to a file-scope function `_qN` and leaves
the wrapper behind.  It is a *build-time bridge*: the permanent fix belongs
in cgen (see docs/wasm-wasi-plan.md, W2.5).

Usage: python scripts/lift-nested.py input.c output.c
"""
import re
import sys


class Block:
    __slots__ = ("parts",)

    def __init__(self):
        self.parts = []


def parse(text):
    root = Block()
    stack = [root]
    buf = []
    i = 0
    n = len(text)

    def flush():
        if buf:
            stack[-1].parts.append("".join(buf))
            buf.clear()

    while i < n:
        c = text[i]
        if c == '"' or c == "'":
            quote = c
            buf.append(c)
            i += 1
            while i < n:
                d = text[i]
                buf.append(d)
                i += 1
                if d == "\\" and i < n:
                    buf.append(text[i])
                    i += 1
                    continue
                if d == quote:
                    break
            continue
        if c == "/" and i + 1 < n and text[i + 1] == "/":
            j = text.find("\n", i)
            if j < 0:
                j = n
            buf.append(text[i:j])
            i = j
            continue
        if c == "/" and i + 1 < n and text[i + 1] == "*":
            j = text.find("*/", i + 2)
            if j < 0:
                raise ValueError("unterminated comment")
            j += 2
            buf.append(text[i:j])
            i = j
            continue
        if c == "{":
            flush()
            child = Block()
            stack[-1].parts.append(child)
            stack.append(child)
            i += 1
            continue
        if c == "}":
            flush()
            if len(stack) == 1:
                raise ValueError("unbalanced '}'")
            stack.pop()
            i += 1
            continue
        buf.append(c)
        i += 1
    flush()
    if len(stack) != 1:
        raise ValueError("unbalanced '{'")
    return root


SIG = re.compile(r"auto\s+void\s+_qb\s*\(void\)\s*$")
IDENT = re.compile(r"\b_qb\b")


class Lifter:
    def __init__(self):
        self.n = 0
        self.defs = []
        self.protos = []

    def new_name(self):
        name = f"_q{self.n}"
        self.n += 1
        return name

    def replace_ident_in_block(self, block, old, new):
        for idx, part in enumerate(block.parts):
            if isinstance(part, str):
                block.parts[idx] = IDENT.sub(new, part)
            else:
                self.replace_ident_in_block(part, old, new)

    def transform(self, block):
        # Transform children first: a quote body may itself contain quotes.
        for part in block.parts:
            if isinstance(part, Block):
                self.transform(part)

        # Collect/remove `auto void _qb(void) { body }` in this block.
        # The body is the child Block immediately following the signature.
        new_parts = []
        lifted = None
        i = 0
        while i < len(block.parts):
            part = block.parts[i]
            if isinstance(part, str) and i + 1 < len(block.parts) and isinstance(block.parts[i + 1], Block):
                m = SIG.search(part)
                if m:
                    if lifted is not None:
                        raise ValueError("multiple _qb definitions in one block")
                    name = self.new_name()
                    body = self.render(block.parts[i + 1])
                    self.defs.append(f"static void {name}(void) {{\n{body}\n}}\n")
                    self.protos.append(f"static void {name}(void);\n")
                    lifted = name
                    new_parts.append(part[:m.start()])
                    i += 2  # skip signature text's tail and the body block
                    continue
            new_parts.append(part)
            i += 1
        block.parts = new_parts

        if lifted is not None:
            # All remaining `_qb` references in this scope belong to this quote.
            self.replace_ident_in_block(block, "_qb", lifted)
        return block

    def render(self, block):
        out = []
        for part in block.parts:
            if isinstance(part, str):
                out.append(part)
            else:
                out.append("{" + self.render(part) + "}")
        return "".join(out)


def main():
    if len(sys.argv) != 3:
        print("usage: lift-nested.py input.c output.c", file=sys.stderr)
        return 2
    src = open(sys.argv[1], "r", encoding="utf-8").read()
    tree = parse(src)
    lifter = Lifter()
    lifter.transform(tree)
    body = lifter.render(tree)
    if IDENT.search(body):
        raise SystemExit("lift-nested: leftover _qb identifier")
    anchor = '#include "runtime.c"\n'
    if anchor not in body:
        raise SystemExit('lift-nested: missing #include "runtime.c"')
    protos = "".join(lifter.protos)
    body = body.replace(anchor, anchor + "\n/* quote bodies lifted from GNU nested functions */\n" + protos, 1)
    body += "\n/* lifted quote bodies */\n" + "".join(lifter.defs)
    open(sys.argv[2], "w", encoding="utf-8", newline="\n").write(body)
    print(f"lift-nested: lifted {lifter.n} quote function(s)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
