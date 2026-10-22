#!/usr/bin/env bash
# WASM core conformance: every tests/conformance/*.ws through wd c -> zig cc
# -> WAMR, compared against its `// expect:` line.  No Rust runtime involved.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
export PATH="/mingw64/bin:/usr/bin:/c/msys64/clang64/bin:$PATH"
if [ -z "${WRUN_IWASM:-}" ]; then
    for r in "$ROOT/target/toolchain/wamr/iwasm" "$ROOT/target/toolchain/wamr/iwasm.exe"; do
        [ -x "$r" ] && { export WRUN_IWASM="$r"; break; }
    done
fi
TMP="$ROOT/target/wasm-conformance"
rm -rf "$TMP"; mkdir -p "$TMP/bin" "$TMP/root"
PASS=0; FAIL=0; SKIP=0
norm() { tr -d '\r' | awk 'NF { if (n++) printf "|"; printf "%s", $0 } END { if (n) printf "\n" }'; }

for f in tests/conformance/*.ws; do
    name="$(basename "$f" .ws)"
    expected="$(head -1 "$f" | sed 's|^// expect: ||')"
    out="$TMP/bin/$name.wasm"
    if ! bash scripts/build-wasm.sh "$f" "$out" >"$TMP/build.log" 2>&1; then
        FAIL=$((FAIL+1)); echo "FAIL $name: build"; tail -3 "$TMP/build.log"; continue
    fi
    if [ "$name" = "26_file_io" ]; then
        # The fixture uses a relative path; give it a private root preopen.
        bash scripts/wrun.sh --dir "/::$TMP/root" "$out" >"$TMP/run.out" 2>"$TMP/run.err"
    else
        bash scripts/wrun.sh "$out" >"$TMP/run.out" 2>"$TMP/run.err"
    fi
    got="$(norm < "$TMP/run.out")"
    if [ "$got" = "$expected" ]; then
        PASS=$((PASS+1))
    else
        FAIL=$((FAIL+1))
        echo "FAIL $name: expected [$expected] got [$got]"
        tail -2 "$TMP/run.err" 2>/dev/null
    fi
done
echo "=== wasm conformance: PASS=$PASS FAIL=$FAIL SKIP=$SKIP ==="
[ "$FAIL" -eq 0 ]
