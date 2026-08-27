// png-decode 轮子：完整解码（colortype 2，8-bit，全 5 种滤波）
// chunk 递归遍历 + zlib FFI 解压 + 滤波反解 + 像素输出
import "std/ffi"

: ZDLL { "C:/msys64/mingw64/bin/zlib1.dll" } ;

: be32-of {
    4 strslice
    dup 3 1 strslice
    over 2 1 strslice strcat
    over 1 1 strslice strcat
    ` 0 1 strslice strcat
    "<I" unpack 0 @nth
} ;

: z-inflate {
    dup buf-alloc
    8 buf-alloc dup
    $3 buf-poke
    ZDLL "uncompress" ffi-fn
    [] $3 append $2 append $5 append $5 strlen append
    "ipppi" rot ffi-call drop
    drop
    swap buf->str
    ` drop
} ;

: do-idat {
    "s" kv-get "pos" kv-get strtoi64 8 +
    "cl" kv-get strtoi64 strslice
    "ID" kv-get strcat "ID" ` kv-set
} ;

: walk-r {
    "len" kv-get strtoi64 "pos" kv-get strtoi64 <= ??
        return
    |
        "s" kv-get "pos" kv-get strtoi64 be32-of i64tostr "cl" ` kv-set
        "s" kv-get "pos" kv-get strtoi64 4 + 4 strslice "ty" ` kv-set
        "ty" kv-get "IDAT" streq ?? do-idat | ]
        "pos" kv-get strtoi64 "cl" kv-get strtoi64 + 12 + i64tostr
        "pos" ` kv-set
        walk-r
    ]
} ;

// ── 滤波反解（按字节递归；未滤波图像积累在 KV "img"）──

: pabs { _ 0 < ?? -1 * | ] } ;

// A B C → Paeth 预测值（寄存器走 KV）
: paeth {
    i64tostr "C" ` kv-set i64tostr "B" ` kv-set i64tostr "A" ` kv-set
    "A" kv-get strtoi64 "B" kv-get strtoi64 + "C" kv-get strtoi64 - i64tostr "p" ` kv-set
    "p" kv-get strtoi64 "A" kv-get strtoi64 - pabs i64tostr "pa" ` kv-set
    "p" kv-get strtoi64 "B" kv-get strtoi64 - pabs i64tostr "pb" ` kv-set
    "p" kv-get strtoi64 "C" kv-get strtoi64 - pabs i64tostr "pc" ` kv-set
    "pa" kv-get strtoi64 "pb" kv-get strtoi64 <= "pa" kv-get strtoi64 "pc" kv-get strtoi64 <= & ??
        "A" kv-get strtoi64
    | "pb" kv-get strtoi64 "pc" kv-get strtoi64 <= ?? "B" kv-get strtoi64 | "C" kv-get strtoi64 ] ]
} ;


// → v：滤波反解（f/A/B/C/F 全走 KV）
: recon {
    "f" kv-get strtoi64 0 = ??
        "F" kv-get strtoi64
    | "f" kv-get strtoi64 1 = ??
        "F" kv-get strtoi64 "A" kv-get strtoi64 +
    | "f" kv-get strtoi64 2 = ??
        "F" kv-get strtoi64 "B" kv-get strtoi64 +
    | "f" kv-get strtoi64 3 = ??
        "F" kv-get strtoi64 "A" kv-get strtoi64 "B" kv-get strtoi64 + 2 / +
    | "F" kv-get strtoi64
      "A" kv-get strtoi64 "B" kv-get strtoi64 "C" kv-get strtoi64 paeth +
    ] ] ] ]
} ;

// 图内取字节：absidx → byte（"img" 未滤波）
: byte-at { "img" kv-get ` strnth } ;

// i → 反解第 i 字节并追加（"i" 为全局字节序号）
: dec-byte {
    i64tostr "i" ` kv-set
    // y = i/st, x = i%st
    "i" kv-get strtoi64 "st" kv-get strtoi64 / i64tostr "y" ` kv-set
    "i" kv-get strtoi64 "st" kv-get strtoi64 % i64tostr "x" ` kv-set
    // 滤波类型 = raw[y*(st+1)]
    "raw" kv-get
    "y" kv-get strtoi64 "st" kv-get strtoi64 1 + * strnth
    i64tostr "f" ` kv-set
    // A（左）
    "x" kv-get strtoi64 3 >= ??
        "img" kv-get "y" kv-get strtoi64 "st" kv-get strtoi64 * "x" kv-get strtoi64 + 3 - strnth
    | 0 ] i64tostr "A" ` kv-set
    // B（上）
    "y" kv-get strtoi64 1 >= ??
        "img" kv-get "y" kv-get strtoi64 1 - "st" kv-get strtoi64 * "x" kv-get strtoi64 + strnth
    | 0 ] i64tostr "B" ` kv-set
    // C（左上）
    "y" kv-get strtoi64 1 >= "x" kv-get strtoi64 3 >= & ??
        "img" kv-get "y" kv-get strtoi64 1 - "st" kv-get strtoi64 * "x" kv-get strtoi64 + 3 - strnth
    | 0 ] i64tostr "C" ` kv-set
    // F（滤波字节）
    "raw" kv-get
    "y" kv-get strtoi64 "st" kv-get strtoi64 1 + * 1 + "x" kv-get strtoi64 + strnth
    i64tostr "F" ` kv-set
    // v = F + recon；mod 256 追加
    recon 256 % ctos
    "img" kv-get ` strcat "img" ` kv-set
} ;

: dec-all {
    "i" kv-get strtoi64 "tot" kv-get strtoi64 < ??
        "i" kv-get strtoi64 dec-byte
        "i" kv-get strtoi64 1 + i64tostr "i" ` kv-set
        dec-all
    | ]
} ;

: px-row {
    "W" kv-get strtoi64 "x" kv-get strtoi64 <= ?? return |
        "img" kv-get
        "y" kv-get strtoi64 "st" kv-get strtoi64 * "x" kv-get strtoi64 3 * +
        3 strslice
        dup 0 strnth i64tostr " " strcat
        over 1 strnth i64tostr strcat " " strcat
        ` 2 strnth i64tostr strcat .
        "x" kv-get strtoi64 1 + i64tostr "x" ` kv-set
        px-row
    ]
} ;

: px-all {
    "H" kv-get strtoi64 "y" kv-get strtoi64 <= ?? return |
        "x" "0" kv-set
        px-row
        "y" kv-get strtoi64 1 + i64tostr "y" ` kv-set
        px-all
    ]
} ;

: png-decode {
    read-file "s" ` kv-set
    "s" kv-get strlen i64tostr "len" ` kv-set
    "s" kv-get 16 be32-of i64tostr "W" ` kv-set
    "s" kv-get 20 be32-of i64tostr "H" ` kv-set
    "ID" "" kv-set
    "pos" "8" kv-set
    walk-r
    "ID" kv-get
    "W" kv-get strtoi64 3 * 1 + "H" kv-get strtoi64 * i64tostr strtoi64
    z-inflate "raw" ` kv-set
    // st = W*3; tot = W*H*3
    "W" kv-get strtoi64 3 * i64tostr "st" ` kv-set
    "W" kv-get strtoi64 "H" kv-get strtoi64 * 3 * i64tostr "tot" ` kv-set
    "img" "" kv-set
    "i" "0" kv-set
    dec-all
    "y" "0" kv-set
    px-all
} ;
export be32-of
export z-inflate
export png-decode
export ZDLL
export walk-r
export do-idat
export pabs
export paeth
export recon
export byte-at
export dec-byte
export dec-all
export px-row
export px-all
