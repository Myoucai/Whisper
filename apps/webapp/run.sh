#!/bin/bash
# Build and run the Whisper web backend on the self-hosted chain.
set -e
APP="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$APP/../.." && pwd)"
BIN="${BIN:-$ROOT/target/debug/whisper.exe}"
PORT="${WS_PORT:-8097}"
cd "$APP"
"$BIN" build server.ws -t c2 -o server.exe >/dev/null
echo "listening on :$PORT (WS_PORT to change)"
exec ./server.exe
