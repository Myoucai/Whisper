#!/usr/bin/env bash
# Validate the generated SBOM (works offline with --no-deps fallback).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
PY="${PYTHON:-}"
if [ -z "$PY" ]; then
    for p in python3 python; do command -v "$p" >/dev/null 2>&1 && { PY="$p"; break; }; done
fi
[ -n "$PY" ] || { echo "sbom-gate: python required" >&2; exit 1; }
OUT="$ROOT/target/sbom.json"
bash scripts/sbom.sh "$OUT" >/dev/null
"$PY" - "$OUT" <<'PY'
import json
import sys

doc = json.load(open(sys.argv[1], encoding="utf-8"))
assert doc.get("bomFormat") == "CycloneDX", "bad bomFormat"
assert doc.get("specVersion") == "1.4", "bad specVersion"
root = doc["metadata"]["component"]
assert root["name"] == "whisper", root
assert root["version"] not in ("unknown", ""), root
names = {c["name"] for c in doc["components"]}
assert "WAMR iwasm" in names, "WAMR component missing"
assert "whisper-cli" in names, "workspace components missing"
assert len(doc["components"]) >= 5, "too few components"
print("sbom-gate: PASS (%d components, whisper %s)" % (
    len(doc["components"]), root["version"]))
PY
