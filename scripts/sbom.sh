#!/usr/bin/env bash
# Generate a deterministic-ish CycloneDX-style SBOM for Whisper.
# Usage: scripts/sbom.sh [output.json]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-$ROOT/target/sbom.json}"
PY="${PYTHON:-}"
if [ -z "$PY" ]; then
    for p in python3 python; do command -v "$p" >/dev/null 2>&1 && { PY="$p"; break; }; done
fi
[ -n "$PY" ] || { echo "sbom: python3/python required" >&2; exit 1; }
mkdir -p "$(dirname "$OUT")"
"$PY" - "$ROOT" "$OUT" <<'PY'
import json
import os
import subprocess
import sys

root, out = sys.argv[1], sys.argv[2]
try:
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=root, stderr=subprocess.PIPE, text=True))
except subprocess.CalledProcessError:
    try:
        meta = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            cwd=root, stderr=subprocess.PIPE, text=True))
    except subprocess.CalledProcessError as e:
        sys.stderr.write(e.stderr or str(e))
        raise SystemExit("sbom: cargo metadata failed")

def tool_version(cmd):
    try:
        return subprocess.check_output(cmd, stderr=subprocess.DEVNULL, text=True).splitlines()[0]
    except Exception:
        return "unavailable"

packages = []
root_pkg = None
for p in meta.get("packages", []):
    name, version = p.get("name"), p.get("version")
    if p.get("source") is None and name == "whisper":
        root_pkg = {"name": name, "version": version}
    packages.append({
        "type": "library",
        "name": name,
        "version": version,
        "license": p.get("license") or "UNKNOWN",
        "source": p.get("source") or "workspace",
    })
if root_pkg is None:
    for p in meta.get("packages", []):
        if p.get("source") is None:
            root_pkg = {"name": "whisper", "version": p.get("version")}
            break
packages.sort(key=lambda x: (x["name"].lower(), x["version"]))
root_pkg = root_pkg or {"name": "whisper", "version": "unknown"}

components = list(packages)
components.append({
    "type": "application",
    "name": "WAMR iwasm",
    "version": os.environ.get("WAMR_VERSION", "2.4.5"),
    "license": "Apache-2.0",
    "source": "release/pinned",
})
components.append({
    "type": "library",
    "name": "OpenSSL (dynamic, optional)",
    "version": "system",
    "license": "Apache-2.0",
    "source": "runtime/dlopen",
    "scope": "optional",
})
components.sort(key=lambda x: (x["name"].lower(), x["version"]))

doc = {
    "bomFormat": "CycloneDX",
    "specVersion": "1.4",
    "version": 1,
    "metadata": {
        "component": {"type": "application", **root_pkg},
        "tools": [
            {"name": "cargo", "version": tool_version(["cargo", "--version"])},
            {"name": "rustc", "version": tool_version(["rustc", "--version"])},
            {"name": "gcc", "version": tool_version(["gcc", "--version"])},
            {"name": "wrun", "version": root_pkg["version"]},
        ],
    },
    "components": components,
    "notes": [
        "WASM guest execution uses WAMR iwasm (pinned by CI).",
        "OpenSSL is loaded dynamically at runtime and is not linked.",
        "Native C output is trusted and not sandboxed.",
    ],
}
with open(out, "w", encoding="utf-8") as f:
    json.dump(doc, f, ensure_ascii=False, indent=2, sort_keys=True)
    f.write("\n")
print("sbom: wrote %s (%d components)" % (out, len(components)))
PY
