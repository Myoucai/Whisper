#!/usr/bin/env python3
"""
Step 1b: new-vocabulary training records (v1.0.1 IO + v1.0.2 KV/serve)
plus Forth-interference drills, all execution-verified where runnable.

Socket words (listen/accept/recv/send/serve-concurrent) cannot be verified
in a headless snippet; they mirror apps/webapp/server.ws patterns that
app-gate verifies end-to-end — emitted with pattern_verified: true.

Outputs:
    ../data/train_newvocab.jsonl   (append to train_v3 -> train_v4)
    ../data/eval_v2.jsonl          (held-out: parameters disjoint from train)
"""
import argparse
import json
import os
import subprocess
import tempfile

IO_ENV = {**os.environ, "WHISPER_ALLOW_IO": "1"}

NET_IMPORT = 'import "std/net"'


def run(code, bin_path, timeout=10):
    tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".ws",
                                      delete=False, encoding="utf-8")
    tmp.write(code)
    tmp.close()
    try:
        r = subprocess.run([bin_path, "run", tmp.name], capture_output=True,
                           text=True, timeout=timeout, env=IO_ENV,
                           cwd=os.path.dirname(os.path.abspath(__file__)))
        if r.returncode != 0:
            return None
        return " ".join(r.stdout.split())
    except subprocess.TimeoutExpired:
        return None
    finally:
        os.unlink(tmp.name)


def make(records, instr, code, output=None, extra=None, bin_path=None):
    """Add a record; derive output by execution unless given (sockets)."""
    rec = {"instruction": instr, "input": "", "whisper": code}
    if output is None:
        output = run(code, bin_path)
        if output is None:
            return None  # caller reports
    rec["output"] = output
    if extra:
        rec.update(extra)
    records.append(rec)
    return rec


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--whisper-bin", required=True)
    args = ap.parse_args()
    W = args.whisper_bin
    tr, ev = [], []
    failed = []

    def add(target, instr, code, output=None, extra=None):
        if make(target, instr, code, output, extra, W) is None and output is None:
            failed.append(instr)

    # ── KV store (v1.0.2) ────────────────────────────────────────────
    add(tr, "Store a value in the shared KV store", '"greeting" "hello" kv-set "greeting" kv-get .')
    add(tr, "Set and get a KV pair", '"k1" "v1" kv-set "k1" kv-get .')
    add(tr, "Overwrite an existing KV key", '"k" "old" kv-set "k" "new" kv-set "k" kv-get .')
    add(tr, "Delete a key from the KV store", '"k" "v" kv-set "k" kv-del "k" kv-get strlen .')
    add(tr, "Getting a missing KV key returns an empty string", '"nope" kv-get strlen .')
    add(tr, "Count the keys in the KV store", '"a" "1" kv-set "b" "2" kv-set kv-keys len .')
    add(tr, "List all KV keys and count them", '"x" "1" kv-set "y" "2" kv-set "z" "3" kv-set kv-keys len .')
    add(tr, "Use KV as a visit counter", '"n" "0" kv-set "n" kv-get strtoi64 1 + i64tostr "n" ` kv-set "n" kv-get .')
    add(tr, "Check whether a KV key exists", '"k" "v" kv-set "k" kv-get strlen 0 > .')
    add(tr, "Delete a missing KV key is harmless", '"ghost" kv-del "ok" .')
    add(ev, "Store and retrieve a config value in KV", '"port" "8080" kv-set "port" kv-get .')

    # ── file IO (v1.0.1) ─────────────────────────────────────────────
    add(tr, "Write a file then read it back", '"ft_a.txt" "whisper io" write-file drop "ft_a.txt" read-file .')
    add(tr, "Write text to a file", '"ft_b.txt" "data" write-file drop "ft_b.txt" read-file .')
    add(tr, "Reading a missing file returns an empty string", '"ft_missing.txt" read-file strlen .')
    add(tr, "Measure the length of a file's content", '"ft_c.txt" "hello file" write-file drop "ft_c.txt" read-file strlen .')
    add(tr, "Overwrite a file changes its content", '"ft_d.txt" "one" write-file drop "ft_d.txt" "two" write-file drop "ft_d.txt" read-file .')
    add(tr, "Chain file write and string ops", '"ft_e.txt" "abc" write-file drop "ft_e.txt" read-file "abc" streq .')
    add(ev, "Persist a value to disk and read it", '"ft_ev.txt" "stored" write-file drop "ft_ev.txt" read-file .')

    # ── env (v1.0.1) ─────────────────────────────────────────────────
    add(tr, "Read an environment variable exists", '"PATH" getenv strlen 0 > .')
    add(tr, "A missing environment variable yields an empty string", '"WHISPER_NO_SUCH_VAR" getenv strlen .')
    add(tr, "Read env and measure it", '"PATH" getenv strlen 0 >= .')

    # ── stdlib/net string words ─────────────────────────────────────
    add(tr, "URL-decode a percent-encoded string", f'{NET_IMPORT}\n"Li%20Ming" url-decode .')
    add(tr, "Decode %XX escapes in a URL", f'{NET_IMPORT}\n"a%20b" url-decode .')
    add(tr, "URL-decode leaves plain text unchanged", f'{NET_IMPORT}\n"plain" url-decode .')
    add(tr, "Extract the query string of a request path", f'{NET_IMPORT}\n"/hi?x=1" query-of .')
    add(tr, "Get a form value from a urlencoded body", f'{NET_IMPORT}\n"name=Ali" "name" form-get .')
    add(tr, "Extract the request method", f'{NET_IMPORT}\n"GET /x HTTP/1.0" req-method .')
    add(tr, "Extract the request path", f'{NET_IMPORT}\n"GET /kv/x HTTP/1.0" req-path .')
    add(tr, "Strip the query from a path", f'{NET_IMPORT}\n"/a/b?x=1" path-only .')
    add(ev, "Decode multibyte percent escapes", f'{NET_IMPORT}\n"a%20b%20c" url-decode .')

    # ── Forth-interference drills ────────────────────────────────────
    add(tr, "Swap two numbers and subtract (10 3 -> 3-10)", '10 3 ` - .')
    add(tr, "Exchange the top two values then add", '1 2 ` + .')
    add(tr, "Duplicate the top value and multiply (square)", '5 _ * .')
    add(tr, "Duplicate the top of the stack", '7 _ + .')
    add(tr, "Copy the second value with over", '2 3 over * .')
    add(tr, "Rotate three values", '1 2 3 @ drop + .')
    add(tr, "Drop the top value", '1 2 drop .')
    add(tr, "Bitwise AND (not logical)", '12 10 bitand .')
    add(tr, "Logical AND of two booleans", '#t #f & .')
    add(tr, "Logical OR of two booleans", '#t #f | .')
    add(tr, "Conditional with ?? | ]", '5 3 > ?? "yes" . | "no" . ]')
    add(tr, "Even/odd check with a conditional", '4 2 % 0 = ?? "even" . | "odd" . ]')
    add(ev, "Swap and divide (20 4 -> 4/20)", '20 4 ` / .')

    # ── socket words (pattern-verified only) ────────────────────────
    sock = [
        ("Read one HTTP request from a connection", '_ recv req-path .'),
        ("Send a response on a connection",
         '_ recv drop "HTTP/1.0 200 OK\\r\\nContent-Length: 2\\r\\n\\r\\nhi" send drop'),
        ("Serve requests with a handler word",
         ': handle { _ recv drop "HTTP/1.0 200 OK\\r\\n\\r\\n" send drop } ;\n8080 "handle" serve-concurrent'),
        ("Start a concurrent web server on a port",
         ': handle { _ recv req-path path-only . send drop } ;\n8080 "handle" serve-concurrent'),
        ("Read a request's method and path",
         '_ recv dup req-method . req-path .'),
    ]
    for instr, code in sock:
        tr.append({"instruction": instr, "input": "",
                   "whisper": code, "output": "",
                   "pattern_verified": True})

    # cleanup scratch files
    for f in os.listdir("."):
        if f.startswith("ft_") and f.endswith(".txt"):
            os.unlink(f)

    with open("../data/train_newvocab.jsonl", "w", encoding="utf-8") as f:
        for r in tr:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    with open("../data/eval_v2.jsonl", "w", encoding="utf-8") as f:
        for r in ev:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")

    print(f"train_newvocab : {len(tr)} records ({sum(1 for r in tr if r.get('pattern_verified'))} pattern-verified sockets)")
    print(f"eval_v2        : {len(ev)} records")
    if failed:
        print("FAILED TO EXECUTE:")
        for i in failed:
            print(f"  - {i}")


if __name__ == "__main__":
    main()
