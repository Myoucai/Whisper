#!/usr/bin/env bash
# Compile the full PSL (embedded data) checks against runtime.c.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
CC="${CC:-cc}"
mkdir -p target
case "$(uname -s 2>/dev/null || echo Windows)" in
    *NT*|*MINGW*|*MSYS*) LIBS="-lws2_32 -lm" ;;
    *) LIBS="-ldl -lpthread -lm" ;;
esac
"$CC" -O2 -I whisperc fuzz/psl_test.c -o target/psl-test $LIBS
./target/psl-test 2>/dev/null || ./target/psl-test.exe
