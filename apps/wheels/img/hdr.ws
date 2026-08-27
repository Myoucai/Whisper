import "std/ffi"
: TJ { "C:/msys64/mingw64/bin/libturbojpeg.dll" } ;
: hdr {
    read-file "j" ` kv-set
    TJ "tjInitDecompress" ffi-fn [] "i" rot ffi-call    // [h]
    8 buf-alloc 8 buf-alloc 8 buf-alloc 8 buf-alloc      // [h,wb,hb,fb,sb]
    TJ "tjDecompressHeader3" ffi-fn                      // [h,wb,hb,fb,sb,fn]
    [] $6 append "j" kv-get append "j" kv-get strlen append
       $5 append $4 append $3 append $2 append
    "ippppppp" rot ffi-call     // 只取 W 打印（不 kv-set，直接留栈上看）
    $4 buf-peek 4294967295 bitand .
    $3 buf-peek 4294967295 bitand .
    $2 buf-peek 4294967295 bitand .
    $1 buf-peek 4294967295 bitand .
    // 栈上剩 [h,wb,hb,fb,sb] 5 项——用 .. 数
    ..
} ;
"t.jpg" hdr
