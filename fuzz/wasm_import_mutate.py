#!/usr/bin/env python3
"""Mutation smoke test for scripts/wasm-imports.py.

This is deliberately dependency-free (no atheris/libFuzzer): it mutates a
valid import section and asserts the validator only exits 0/1, never crashes
or hangs. It complements the libFuzzer HTTP harness.
"""
import os
import random
import subprocess
import sys
import tempfile


def uleb(n: int) -> bytes:
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def base_module() -> bytes:
    mod = b"wasi_snapshot_preview1"
    field = b"fd_write"
    payload = (
        uleb(1)
        + uleb(len(mod)) + mod
        + uleb(len(field)) + field
        + b"\x00"          # func import
        + uleb(0)          # type index
    )
    return b"\0asm\x01\0\0\0" + b"\x02" + uleb(len(payload)) + payload


def main() -> int:
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    validator = os.path.join(root, "scripts", "wasm-imports.py")
    base = bytearray(base_module())
    rng = random.Random(1234)
    iterations = int(os.environ.get("WASM_FUZZ_ITERS", "300"))
    with tempfile.TemporaryDirectory(prefix="whisper-wasm-fuzz-") as td:
        path = os.path.join(td, "m.wasm")
        for i in range(iterations):
            data = bytearray(base)
            op = rng.randrange(4)
            if op == 0 and data:
                data[rng.randrange(len(data))] ^= 1 << rng.randrange(8)
            elif op == 1 and len(data) > 9:
                del data[rng.randrange(8, len(data)):]
            elif op == 2:
                data.insert(rng.randrange(len(data) + 1), rng.randrange(256))
            else:
                for _ in range(rng.randrange(1, 6)):
                    if data:
                        data[rng.randrange(len(data))] = rng.randrange(256)
            with open(path, "wb") as f:
                f.write(data)
            try:
                r = subprocess.run([sys.executable, validator, path],
                                   stdout=subprocess.DEVNULL,
                                   stderr=subprocess.DEVNULL,
                                   timeout=2)
            except subprocess.TimeoutExpired:
                print("wasm-fuzz-gate: validator hung on iteration %d" % i)
                return 1
            if r.returncode not in (0, 1):
                print("wasm-fuzz-gate: validator crashed/errored (%d) on iteration %d"
                      % (r.returncode, i))
                return 1
    print("wasm-fuzz-gate: PASS (%d mutations)" % iterations)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
