#!/bin/bash
# Shared: "are these two files byte-identical?"
#
# `cmp` is the natural tool, but coreutils is not guaranteed complete on every
# platform these scripts run on (msys2 ships only a subset — measured on the
# dev machine: `cmp` and `diff` are absent, `sha256sum` is present). Getting
# this wrong is not cosmetic: `if ! cmp -s a b` with a missing `cmp` evaluates
# 127 negated to *true*, i.e. a missing tool reads as "the fixed point broke".
# bootstrap.sh shipped with exactly that bug — it could never reach step 5.
same_bytes() {
    if command -v cmp >/dev/null 2>&1; then
        cmp -s "$1" "$2"
    elif command -v sha256sum >/dev/null 2>&1; then
        [ "$(sha256sum < "$1")" = "$(sha256sum < "$2")" ]
    else
        # Last resort: treat unequal sizes as a mismatch and equal sizes as
        # unverifiable-but-not-provably-broken.
        [ "$(wc -c < "$1")" = "$(wc -c < "$2")" ]
    fi
}
