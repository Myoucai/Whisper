#!/usr/bin/env python3
"""Reject non-WASI imports before wrun starts a guest.

WASI Preview 1 has no sockets/exec/FFI; the only legitimate import module
for the v1 profile is `wasi_snapshot_preview1`.  Unknown modules (including
`env.*`) are refused up front instead of relying on the runtime to fail
only when the import is called.
"""
import sys

# Preview 1 functions the Whisper C runtime is allowed to import.  Notably
# absent: sock_* (WAMR socket extension), path_symlink/path_link, proc_raise.
WASI_ALLOWED = {
    "args_get", "args_sizes_get",
    "environ_get", "environ_sizes_get",
    "clock_res_get", "clock_time_get",
    "fd_advise", "fd_allocate", "fd_close", "fd_datasync",
    "fd_fdstat_get", "fd_fdstat_set_flags", "fd_filestat_get",
    "fd_filestat_set_size", "fd_filestat_set_times",
    "fd_pread", "fd_prestat_get", "fd_prestat_dir_name", "fd_pwrite",
    "fd_read", "fd_readdir", "fd_seek", "fd_sync", "fd_tell", "fd_write",
    "path_create_directory", "path_filestat_get", "path_filestat_set_times",
    "path_open", "path_remove_directory", "path_rename", "path_unlink_file",
    "poll_oneoff", "proc_exit", "random_get", "sched_yield",
}


class Reader:
    def __init__(self, data):
        self.data = data
        self.i = 0

    def u8(self):
        if self.i >= len(self.data):
            raise ValueError("truncated wasm")
        b = self.data[self.i]
        self.i += 1
        return b

    def uleb(self):
        shift = 0
        out = 0
        while True:
            b = self.u8()
            out |= (b & 0x7F) << shift
            if not (b & 0x80):
                return out
            shift += 7
            if shift > 63:
                raise ValueError("uleb too long")

    def bytes(self, n):
        if self.i + n > len(self.data):
            raise ValueError("truncated wasm")
        b = self.data[self.i:self.i + n]
        self.i += n
        return b

    def name(self):
        n = self.uleb()
        return self.bytes(n).decode("utf-8", "replace")


def limits(r):
    flags = r.u8()
    r.uleb()
    if flags & 1:
        r.uleb()


def skip_import_desc(r, kind):
    if kind == 0:       # func
        r.uleb()
    elif kind == 1:     # table
        r.u8()
        limits(r)
    elif kind == 2:     # memory
        limits(r)
    elif kind == 3:     # global
        r.u8()
        r.u8()
    elif kind == 4:     # tag
        r.u8()
        r.uleb()
    else:
        raise ValueError(f"unknown import kind {kind}")


def main():
    if len(sys.argv) != 2:
        print("usage: wasm-imports.py <module.wasm>", file=sys.stderr)
        return 2
    path = sys.argv[1]
    data = open(path, "rb").read()
    if data[:4] != b"\0asm":
        print(f"wasm-imports: not a wasm module: {path}", file=sys.stderr)
        return 1
    r = Reader(data)
    r.bytes(8)
    imports = []
    while r.i < len(data):
        sec_id = r.u8()
        size = r.uleb()
        payload = r.bytes(size)
        if sec_id != 2:
            continue
        ir = Reader(payload)
        count = ir.uleb()
        for _ in range(count):
            mod = ir.name()
            field = ir.name()
            kind = ir.u8()
            skip_import_desc(ir, kind)
            imports.append((mod, field, kind))
    bad = [x for x in imports if x[0] != "wasi_snapshot_preview1"]
    if bad:
        for mod, field, _ in bad:
            print(f"wasm-imports: refusing non-WASI import {mod}.{field}", file=sys.stderr)
        return 1
    bad = [x for x in imports if x[2] != 0 or x[1] not in WASI_ALLOWED]
    if bad:
        for mod, field, _ in bad:
            print(f"wasm-imports: refusing non-allowlisted WASI import {mod}.{field}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
