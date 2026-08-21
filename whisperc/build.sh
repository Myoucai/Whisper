#!/bin/bash
# Build the Whisper native compiler
# Run this on Linux: bash build.sh

echo "Building Whisper native compiler..."
gcc -O2 -o whisper compiler.c -lm

if [ $? -eq 0 ]; then
    echo "SUCCESS: ./whisper is ready"
    echo ""
    echo "Test it:"
    echo "  echo '42 .' | ./whisper"
    echo "  echo ': sq { _ * } ; 5 sq .' | ./whisper"
    echo ""
    echo "Compile a Whisper program to native:"
    echo "  echo '42 .' | ./whisper > out.c"
    echo "  gcc -O2 -o out out.c -lm"
    echo "  ./out"
else
    echo "FAILED: gcc compilation error"
fi
