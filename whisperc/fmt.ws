// ══════════════════════════════════════════════════════════════════════
// fmt.ws — Step 4 的第一片：Whisper 自己写的布局规范化器
//
//   wfmt <src>            格式化到 stdout
//   wfmt <src> <out>      格式化到 <out>（可以等于 <src>，即就地格式化）
//
// v1 只做**布局**，不做 AST 重排：
//   - CRLF -> LF
//   - 字符串外的行首缩进去掉
//   - 字符串外的行尾空白去掉
//   - token 间距、字符串内容、注释正文不动
//
// 为什么不做 AST 重排：Rust 侧 `whisper fmt` 会丢注释、把多行定义压成一行。
// v1 先保证"语义和注释一个字节都不丢"，AST printer 留到 v2。判据见
// tests/step4_fmt_preregistered.md。
//
// 跨行状态用 KV（键 fmt.in / fmt.esc / fmt.slash / fmt.cm），值一律 "0"/"1"：
// KV 存不了布尔，cgen 会把布尔按 ptr 传，直接存会 expected ptr。
// ══════════════════════════════════════════════════════════════════════

: fmt-get { "fmt." swap strcat kv-get } ;          // [name] → value
: fmt-set { swap "fmt." swap strcat swap kv-set } ;     // [name value] → —

: fmt-init {
    "in" "0" fmt-set
    "esc" "0" fmt-set
    "slash" "0" fmt-set
    "cm" "0" fmt-set
} ;

// [code] → —。逐字节更新四个标志；注释里的字节不再参与字符串判定。
: fmt-char {
    "cm" fmt-get "1" streq ??
        drop return ]

    "in" fmt-get "1" streq ??
        // 字符串内：先处理转义，再处理闭合引号
        "esc" fmt-get "1" streq ??
            drop "esc" "0" fmt-set return ]
        dup 92 = ??
            drop "esc" "1" fmt-set return ]
        dup 34 = ??
            drop "in" "0" fmt-set return ]
        drop return
    ]

    // 字符串外
    dup 47 = ??
        // 两个连续的 / 才是注释；单个 / 是除法
        "slash" fmt-get "1" streq ??
            drop "cm" "1" fmt-set "slash" "0" fmt-set return
        |   drop "slash" "1" fmt-set return ]
    |   "slash" "0" fmt-set
        dup 34 = ??
            drop "in" "1" fmt-set "esc" "0" fmt-set return
        |   drop ]
    ]
} ;

// [s] → 去掉行首空格/制表符
: fmt-lstrip {
    dup strlen 0 = ?? return ]
    dup 0 strnth
    dup 32 = ?? drop 1 9999 strslice fmt-lstrip return ]
    dup 9 = ?? drop 1 9999 strslice fmt-lstrip return ]
    drop
} ;

// [line] → line'。行首在字符串内时整行保守处理，不碰首尾空白。
: fmt-line {
    "slash" "0" fmt-set
    "cm" "0" fmt-set
    "in" fmt-get "1" streq ??
        dup strchars { fmt-char } @each
        return ]
    fmt-lstrip
    dup strchars { fmt-char } @each
    "in" fmt-get "1" streq ??
        return ]
    str-strip
} ;

// [s] → 去掉末尾一个换行（若有）
: fmt-chomp {
    dup strlen 0 = ?? return ]
    dup dup strlen 1 - strnth 10 = ??
        dup strlen 1 - 0 swap strslice return ]
} ;

// [text] → formatted
: fmt-source {
    "\r\n" "\n" strreplace
    "\n" strsplit
    fmt-init
    ""
    { fmt-line "\n" strcat strcat } @fold
    fmt-chomp
} ;

: fmt-main {
    args
    dup len 2 < ??
        drop "usage: wfmt <src> [out]" . 2 exit ]

    dup 1 @nth file-exists 0 = ??
        "wfmt: cannot read: " over 1 @nth strcat . 1 exit ]

    dup 1 @nth read-file fmt-source       // args formatted

    over len 3 >= ??
        over 2 @nth swap write-file drop
        dup 1 @nth "Formatted: " swap strcat .
    |   print
    ]
    drop
} ;

fmt-main
