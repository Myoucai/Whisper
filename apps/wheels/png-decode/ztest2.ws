import "std/ffi"
// src exp → raw（纯栈式；KV 存不了 i64 指针）
: z-inflate {
    dup buf-alloc                 // [src, exp, dst]
    8 buf-alloc dup               // [src, exp, dst, dlp, dlp]
    $3 buf-poke                   // dlp[0] = exp（$3=exp，dup 留住 dlp）
    "C:/msys64/mingw64/bin/zlib1.dll" "uncompress" ffi-fn
    [] $3 append $2 append $5 append $5 strlen append
    "ipppi" rot ffi-call drop // [src,exp,dst,dlp] → drop rc
    drop                          // drop dlp
    swap buf->str                 // dst+exp → raw
    ` drop
} ;
"ztest.bin" read-file 13 z-inflate strlen .
