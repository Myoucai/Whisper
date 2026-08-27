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
    drop drop drop drop drop   // 清 [h,wb,hb,fb,sb]（db 已被 buf->str 消费）
} ;

// rawRGB W H q → jpeg 串（[pix,W,H,q]）
: jpeg-encode {
    i64tostr "q" ` kv-set i64tostr "H" ` kv-set i64tostr "W" ` kv-set
    "pix" ` kv-set
    TJ "tjInitCompress" ffi-fn [] "i" rot ffi-call      // [h]
    8 buf-alloc 8 buf-alloc                              // [h,ob,zb]
    TJ "tjCompress2" ffi-fn
    [] $4 append "pix" kv-get append
       "W" kv-get strtoi64 append 0 append
       "H" kv-get strtoi64 append 0 append
       $3 append $2 append 0 append
       "q" kv-get strtoi64 append 0 append
    "ippiiiippiii" rot ffi-call drop
    // [h,ob,zb]：zb顶=size槽 ob次=缓冲槽
    dup buf-peek          // size（zb 槽内容）
    $2 buf-peek          // ob 槽内容 = turbojpeg 分配的缓冲指针（二次解引用）
    ` buf->str
    ` drop ` drop ` drop  // [h,ob,zb,jpeg]→[jpeg]（三次换删）
} ;
export TJ
export jpeg-decode
export jpeg-encode
