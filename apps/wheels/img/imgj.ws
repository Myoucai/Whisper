// img 轮子：JPEG 编解码（libturbojpeg FFI，全栈式）
// 实证布局：header3 调用后栈为 [h,wb,hb,fb,sb,rc]，rc 顶上不挡 $N
import "std/ffi"

: TJ { "C:/msys64/mingw64/bin/libturbojpeg.dll" } ;

// path →（KV: "W"/"H"/"pix"）
: jpeg-decode {
    read-file "j" ` kv-set
    TJ "tjInitDecompress" ffi-fn [] "i" rot ffi-call
    8 buf-alloc 8 buf-alloc 8 buf-alloc 8 buf-alloc      // [h,wb,hb,fb,sb]
    TJ "tjDecompressHeader3" ffi-fn
    [] $6 append "j" kv-get append "j" kv-get strlen append
       $5 append $4 append $3 append $2 append
    "ippppppp" rot ffi-call
    // 此刻 [h,wb,hb,fb,sb,rc]：W=$4 H=$3（实证）
    $4 buf-peek 4294967295 bitand i64tostr "W" ` kv-set
    $3 buf-peek 4294967295 bitand i64tostr "H" ` kv-set
    drop                                                // 清 rc → [h,wb,hb,fb,sb]
    "W" kv-get strtoi64 "H" kv-get strtoi64 * 3 * buf-alloc
    TJ "tjDecompress2" ffi-fn                           // [h,wb,hb,fb,sb,db,fn]
    [] $7 append "j" kv-get append "j" kv-get strlen append
       $2 append "W" kv-get strtoi64 append 0 append
       "H" kv-get strtoi64 append 0 append 0 append
    "ippiiiiiii" rot ffi-call drop                 // rc+残留
    "W" kv-get strtoi64 "H" kv-get strtoi64 * 3 *
    buf->str "pix" ` kv-set
    // 句柄/缓冲随进程退出回收（脚本场景；长驻服务需补 destroy/free）
} ;

// rawRGB W H q → jpeg 串（[pix,W,H,q]）
: jpeg-encode {
    i64tostr "q" ` kv-set i64tostr "H" ` kv-set i64tostr "W" ` kv-set
    "pix" ` kv-set
    TJ "tjInitCompress" ffi-fn [] "i" rot ffi-call      // [h]
    8 buf-alloc 8 buf-alloc                              // [h,ob,zb]
    TJ "tjCompress2" ffi-fn
    [] $5 append "pix" kv-get append
       "W" kv-get strtoi64 append "W" kv-get strtoi64 append
       "H" kv-get strtoi64 append 0 append
       $3 append $2 append 0 append
       "q" kv-get strtoi64 append 0 append
    "ippiippiiii" rot ffi-call drop
    // ob 槽=输出缓冲指针（$2），zb 槽=长度（$1）→ buf->str（长度在下）
    $1 buf-peek
    $2 buf-peek
    $1 buf-peek
    $2 buf-peek
    ` buf->str
    // 泄漏同上
} ;
export TJ
export jpeg-decode
export jpeg-encode
