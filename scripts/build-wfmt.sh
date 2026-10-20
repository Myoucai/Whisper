#!/usr/bin/env bash
# 用零 Rust 的 wd 构建 wfmt（纯 Whisper 写的格式化器）。
#
#   bash scripts/build-wfmt.sh [out]
#
# 默认输出 target/wfmt/wfmt；wd 不存在时先 bash scripts/build-wd.sh。
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WD="${WD:-$ROOT/target/wd/wd}"
[ -f "$WD" ] || WD="$WD.exe"
OUT="${1:-$ROOT/target/wfmt/wfmt}"
mkdir -p "$(dirname "$OUT")"
if [ ! -f "$WD" ]; then
    echo "build-wfmt: missing $WD (run scripts/build-wd.sh first)" >&2
    exit 1
fi
cd "$ROOT"
WHISPER_ROOT="$ROOT" "$WD" build "$ROOT/whisperc/fmt.ws" "$OUT"
