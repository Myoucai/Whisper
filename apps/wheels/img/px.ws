import "std/ffi"
import "imgj"
"t.jpg" jpeg-decode
: jrow {
    "W" kv-get strtoi64 "x" kv-get strtoi64 <= ?? return |
        "pix" kv-get
        "y" kv-get strtoi64 "W" kv-get strtoi64 * 3 * "x" kv-get strtoi64 3 * +
        3 strslice
        dup 0 strnth i64tostr " " strcat over 1 strnth i64tostr strcat " " strcat
        ` 2 strnth i64tostr strcat .
        "x" kv-get strtoi64 1 + i64tostr "x" ` kv-set
        jrow
    ]
} ;
: jall {
    "H" kv-get strtoi64 "y" kv-get strtoi64 <= ?? return |
        "x" "0" kv-set
        jrow
        "y" kv-get strtoi64 1 + i64tostr "y" ` kv-set
        jall
    ]
} ;
"y" "0" kv-set
jall
