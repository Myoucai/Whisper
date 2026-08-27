import "std/ffi"
: be32-of { 4 strslice dup 3 1 strslice over 2 1 strslice strcat over 1 1 strslice strcat ` 0 1 strslice strcat "<I" unpack 0 @nth } ;
: z-inflate {
    buf-alloc "d" ` kv-set
    8 buf-alloc dup "e" ` kv-set "l" ` kv-set
    "C:/msys64/mingw64/bin/zlib1.dll" "uncompress" ffi-fn "f" kv-set drop
    "d" kv-get "l" kv-get $2 strlen
    [] swap append ` append ` append ` append
    "ipppi" swap ffi-call
    "e" kv-get buf-peek
    "d" kv-get ` buf->str
    ` drop
} ;
"ztest.bin" read-file 13 z-inflate strlen .
