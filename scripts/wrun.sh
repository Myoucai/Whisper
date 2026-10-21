#!/usr/bin/env bash
# wrun — host-side policy launcher for Whisper WASI modules.
#
# Zero Rust: this script only drives a C runtime (WAMR iwasm by default).
# Defaults: no preopen, no env, no network, no exec, no FFI.
#
#   wrun [--dir GUEST::HOST]... [--env NAME]... app.wasm [args...]
#
# --dir uses WAMR's --map-dir; v1 has no read-only preopen (WAMR grants rw).
# --http-allow/--exec/--ffi are intentionally rejected until a host proxy /
# host exec exists.  See docs/wasm-wasi-plan.md W4.
set -euo pipefail

usage() {
    cat <<'EOF'
usage: wrun [options] app.wasm [args...]
  --dir GUEST::HOST   preopen HOST at GUEST (rw; repeatable)
  --env NAME          pass host env NAME (repeatable)
  --iwasm PATH        runtime path (default: $WRUN_IWASM or iwasm in PATH)
  --no-network        accepted no-op (default)
  --no-exec           accepted no-op (default)
  --no-ffi            accepted no-op (default)
  --http-allow HOST   NOT IMPLEMENTED in v1 (host proxy required)
  --exec              NOT IMPLEMENTED in v1
  --ffi               NOT IMPLEMENTED in v1
  --help
EOF
}

die() { echo "wrun: $*" >&2; exit 2; }

DIRS=()
ENVS=()
RT="${WRUN_IWASM:-}"
APP=""
while [ $# -gt 0 ]; do
    case "$1" in
        --dir)
            [ $# -ge 2 ] || die "--dir needs GUEST::HOST"
            DIRS+=("$2"); shift 2 ;;
        --env)
            [ $# -ge 2 ] || die "--env needs NAME"
            ENVS+=("$2"); shift 2 ;;
        --iwasm)
            [ $# -ge 2 ] || die "--iwasm needs PATH"
            RT="$2"; shift 2 ;;
        --no-network|--no-exec|--no-ffi)
            shift ;;
        --http-allow|--exec|--ffi)
            die "$1 is not implemented in wasm v1 (host proxy/exec required)" ;;
        --help|-h)
            usage; exit 0 ;;
        --)
            shift; break ;;
        -*)
            die "unknown option: $1" ;;
        *)
            APP="$1"; shift; break ;;
    esac
done
[ -n "$APP" ] || { usage >&2; exit 2; }
[ -f "$APP" ] || die "no such module: $APP"
[ -n "$RT" ] || RT="$(command -v iwasm || command -v wasm3 || true)"
[ -n "$RT" ] || die "no C WASM runtime found (set WRUN_IWASM or put iwasm in PATH)"

# Fail closed before load: only wasi_snapshot_preview1 imports are allowed.
PY="${PYTHON:-}"
if [ -z "$PY" ]; then
    for p in python3 python; do command -v "$p" >/dev/null 2>&1 && { PY="$p"; break; }; done
fi
[ -n "$PY" ] || die "python3/python is required for the import policy"
"$PY" "$(cd "$(dirname "$0")" && pwd)/wasm-imports.py" "$APP" || die "non-WASI import rejected"

APPDIR="$(cd "$(dirname "$APP")" && pwd)"
APPBASE="$(basename "$APP")"
ARGS=()
for d in "${DIRS[@]:-}"; do
    [ -n "$d" ] || continue
    case "$d" in
        *::*) ;;
        *) die "--dir wants GUEST::HOST (got: $d)" ;;
    esac
    guest="${d%%::*}"
    host="${d#*::}"
    [ -d "$host" ] || die "no such preopen dir: $host"
    command -v find >/dev/null 2>&1 || die "find is required for the preopen symlink check"
    badlink="$(find "$host" -type l -print -quit 2>/dev/null || true)"
    [ -z "$badlink" ] || die "preopen contains a symlink; refusing (escape risk): $badlink"
    host_win="$(cd "$host" && pwd -W 2>/dev/null || true)"
    [ -n "$host_win" ] || host_win="$(cd "$host" && pwd)"
    ARGS+=("--map-dir=$guest::$host_win")
done
for e in "${ENVS[@]:-}"; do
    [ -n "$e" ] || continue
    case "$e" in
        *[!A-Za-z0-9_]*) die "invalid env name: $e" ;;
    esac
    val="${!e-}"
    ARGS+=("--env=$e=$val")
done

# Run from the module's directory: WAMR on Windows drops --env when the
# module argument is an MSYS absolute /d/... path.
cd "$APPDIR"
exec "$RT" "${ARGS[@]}" "$APPBASE" "$@"
