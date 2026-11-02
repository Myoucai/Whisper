#!/usr/bin/env bash
# Validate deployment templates where the host provides systemd/PyYAML.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1
UNIT="$ROOT/deploy/systemd/whisper-tenant@.service"
COMPOSE="$ROOT/deploy/compose.yaml"
if command -v systemd-analyze >/dev/null 2>&1; then
    systemd-analyze verify "$UNIT"
    echo "deploy-gate: systemd unit ok"
else
    echo "deploy-gate: SKIP systemd-analyze"
fi
PY="${PYTHON:-}"
if [ -z "$PY" ]; then
    for p in python3 python; do command -v "$p" >/dev/null 2>&1 && { PY="$p"; break; }; done
fi
if [ -n "$PY" ] && "$PY" -c 'import yaml' >/dev/null 2>&1; then
    "$PY" - "$COMPOSE" <<'PY'
import sys
import yaml
svc = yaml.safe_load(open(sys.argv[1], encoding="utf-8"))["services"]["tenant"]
assert svc["read_only"] is True
assert svc["network_mode"] == "none"
assert svc["cap_drop"] == ["ALL"]
assert svc["mem_limit"] == "128m"
assert svc["pids_limit"] == 16
print("deploy-gate: compose ok")
PY
else
    echo "deploy-gate: SKIP compose (PyYAML missing)"
fi
