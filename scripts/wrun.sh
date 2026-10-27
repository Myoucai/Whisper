#!/usr/bin/env bash
# wrun — host-side policy launcher for Whisper WASI modules.
#
# Zero Rust: this script only drives a C runtime (WAMR iwasm by default).
# Defaults: no preopen, no env, no network, no exec, no FFI.
#
#   wrun [--dir GUEST::HOST]... [--dir-ro GUEST::HOST]... [--dir-ro-strict GUEST::HOST]... [--env NAME]... app.wasm [args...]
#
# --dir uses WAMR's --map-dir (rw).  --dir-ro copies the host tree to a
# temporary snapshot and discards guest writes; the host tree is never exposed.
# --dir-ro-strict (Linux only) bind-mounts HOST read-only inside a private
# user/mount namespace and fails closed if the host cannot provide it.
# --http-allow/--exec/--ffi are intentionally rejected until a host proxy /
# host exec exists.  See docs/wasm-wasi-plan.md W4.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

usage() {
    cat <<'EOF'
usage: wrun [options] app.wasm [args...]
  --dir GUEST::HOST   preopen HOST at GUEST (rw; repeatable)
  --dir-ro GUEST::HOST
                       copy HOST to a snapshot, preopen it ro-ish; writes are
                       discarded and the host tree is never exposed
  --dir-ro-strict GUEST::HOST
                       Linux: bind-mount HOST read-only in a private user/mount
                       namespace; fails if the OS cannot provide a real RO mount
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
RODIRS=()
ROSTRICT=()
ENVS=()
RT="${WRUN_IWASM:-}"
APP=""
RO_TMP=""
cleanup() { if [ -n "$RO_TMP" ]; then rm -rf "$RO_TMP"; fi; return 0; }
trap cleanup EXIT
while [ $# -gt 0 ]; do
    case "$1" in
        --dir)
            [ $# -ge 2 ] || die "--dir needs GUEST::HOST"
            DIRS+=("$2"); shift 2 ;;
        --dir-ro)
            [ $# -ge 2 ] || die "--dir-ro needs GUEST::HOST"
            RODIRS+=("$2"); shift 2 ;;
        --dir-ro-strict)
            [ $# -ge 2 ] || die "--dir-ro-strict needs GUEST::HOST"
            ROSTRICT+=("$2"); shift 2 ;;
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
# Read-only view: copy-on-write snapshot.  WAMR cannot mount rw as ro, so
# the guest writes into a private copy that is removed on exit.
ro_n=0
for d in "${RODIRS[@]:-}"; do
    [ -n "$d" ] || continue
    case "$d" in
        *::*) ;;
        *) die "--dir-ro wants GUEST::HOST (got: $d)" ;;
    esac
    guest="${d%%::*}"
    host="${d#*::}"
    [ -d "$host" ] || die "no such preopen dir: $host"
    badlink="$(find "$host" -type l -print -quit 2>/dev/null || true)"
    [ -z "$badlink" ] || die "ro preopen contains a symlink; refusing: $badlink"
    if [ -z "$RO_TMP" ]; then
        mkdir -p "$ROOT/target"
        RO_TMP="$(mktemp -d "$ROOT/target/wrun-ro.XXXXXX")" || die "cannot create ro snapshot"
    fi
    ro_n=$((ro_n + 1))
    snap="$RO_TMP/$ro_n"
    mkdir -p "$snap"
    cp -a "$host"/. "$snap"/ || die "cannot snapshot ro preopen: $host"
    badlink="$(find "$snap" -type l -print -quit 2>/dev/null || true)"
    [ -z "$badlink" ] || die "ro snapshot contains a symlink; refusing: $badlink"
    host_win="$(cd "$snap" && pwd -W 2>/dev/null || true)"
    [ -n "$host_win" ] || host_win="$(cd "$snap" && pwd)"
    ARGS+=("--map-dir=$guest::$host_win")
done

# Real read-only preopen: bind mount inside a private user/mount namespace.
# Unlike --dir-ro this does not copy data and the guest sees EROFS, not a
# writable snapshot.  It is intentionally strict: refusal is better than a
# silent downgrade to the snapshot path.
STRICT_HOSTS=()
STRICT_MNTS=()
if [ "${#ROSTRICT[@]}" -gt 0 ]; then
    [ "$(uname -s 2>/dev/null)" = "Linux" ] || die "--dir-ro-strict requires Linux"
    command -v unshare >/dev/null 2>&1 || die "--dir-ro-strict requires unshare"
    command -v mount >/dev/null 2>&1 || die "--dir-ro-strict requires mount"
    unshare -r -m true >/dev/null 2>&1 || die "--dir-ro-strict requires unprivileged user namespaces"
    if [ -z "$RO_TMP" ]; then
        mkdir -p "$ROOT/target"
        RO_TMP="$(mktemp -d "$ROOT/target/wrun-ro.XXXXXX")" || die "cannot create strict ro mountpoint"
    fi
    strict_n=0
    for d in "${ROSTRICT[@]:-}"; do
        [ -n "$d" ] || continue
        case "$d" in
            *::*) ;;
            *) die "--dir-ro-strict wants GUEST::HOST (got: $d)" ;;
        esac
        guest="${d%%::*}"
        host="${d#*::}"
        [ -d "$host" ] || die "no such strict ro preopen dir: $host"
        badlink="$(find "$host" -type l -print -quit 2>/dev/null || true)"
        [ -z "$badlink" ] || die "strict ro preopen contains a symlink; refusing: $badlink"
        host_abs="$(cd "$host" && pwd)"
        strict_n=$((strict_n + 1))
        mnt="$RO_TMP/strict$strict_n"
        mkdir -p "$mnt" || die "cannot create strict ro mountpoint"
        STRICT_HOSTS+=("$host_abs")
        STRICT_MNTS+=("$mnt")
        ARGS+=("--map-dir=$guest::$mnt")
    done
fi

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
if [ "${#STRICT_MNTS[@]}" -gt 0 ]; then
    HELPER="$RO_TMP/strict-ro.sh"
    {
        echo '#!/usr/bin/env bash'
        echo 'set -e'
        i=0
        while [ "$i" -lt "${#STRICT_MNTS[@]}" ]; do
            printf 'mount --bind %q %q\n' "${STRICT_HOSTS[$i]}" "${STRICT_MNTS[$i]}"
            printf 'mount -o remount,bind,ro %q\n' "${STRICT_MNTS[$i]}"
            i=$((i + 1))
        done
        printf 'cd %q\n' "$APPDIR"
        echo 'exec "$@"'
    } > "$HELPER" || die "cannot write strict ro helper"
    chmod +x "$HELPER" || die "cannot chmod strict ro helper"
    unshare -r -m --propagation private bash "$HELPER" "$RT" "${ARGS[@]}" "$APPBASE" "$@"
    status=$?
else
    cd "$APPDIR"
    "$RT" "${ARGS[@]}" "$APPBASE" "$@"
    status=$?
fi
exit "$status"
