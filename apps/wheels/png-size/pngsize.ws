// png-size 轮子：解析 PNG IHDR 取宽高（纯 Whisper，零依赖）
// PNG: 魔数8B + IHDR len4 type4 + 宽4B 高4B（大端，偏移16/20）
// 4B 大端切片 → 逐字节反转拼串 → unpack "<I"（小端）
: be32-of {
    // s off → i64   （s[off..off+4] 大端）
    4 strslice               // [s, w]（s off 4 → strslice 弹 len/start/s 正序）
    dup 3 1 strslice        // [s, w, b3]
    over 2 1 strslice strcat // [s, w, b32]
    over 1 1 strslice strcat // [s, w, b321]
    ` 0 1 strslice strcat   // [s, b3210]
    "<I" unpack 0 @nth
} ;
// path → "WxH" | ""
: png-size {
    read-file
    dup strlen 24 < ?? drop "" | ]
    dup 1 3 strslice "PNG" streq ?? | drop "" ]
    dup 16 be32-of i64tostr
    over 20 be32-of i64tostr
    "x" ` strcat strcat
} ;
export be32-of
export png-size
