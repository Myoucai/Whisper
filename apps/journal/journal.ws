// journal — 命令行日记
//   j add "一句话"     追加一条（自动加时间戳）
//   j list            列出全部
//   j search 关键词    检索
import "std/str"
import "std/math"

: JFILE { "journal.txt" } ;

// full argv → user args (whisper run 走 3..，原生 exe 走 1..)
: j-args {
    args
    dup 1 @nth "run" streq $1 len 1 > & ?? 3 | 1 ]
    over len $1 - list-slice
} ;

// 检索：q 走 KV（引语沙箱不可捕获栈，但可访问进程级 KV）
: j-foreach {
    JFILE read-file "
" strsplit { dup strlen 0 > } @filter
    { dup "jq" kv-get strfind 0 >= ?? . | drop ] } @each
} ;

: j-tail {
    j-args dup len 2 >= ?? 1 @nth | drop "5" ] "jq" ` kv-set
    JFILE read-file "
" strsplit { dup strlen 0 > } @filter
    dup len "jq" kv-get strtoi64 - 0 max
    "jq" kv-get strtoi64
    list-slice
    { dup strlen 0 > ?? . | drop ] } @each
} ;
: j-del {
    j-args 1 @nth "jq" ` kv-set
    "ji" "0" kv-set
    "out" "" kv-set
    JFILE read-file "
" strsplit
    { "ji" kv-get strtoi64 1 + i64tostr "ji" ` kv-set
      "ji" kv-get strtoi64 "jq" kv-get strtoi64 - 0 = ?? drop
      | "out" kv-get ` strcat 10 ctos strcat "out" ` kv-set ] } @each
    "out" kv-get JFILE ` write-file drop
    "deleted" .
} ;
: j-add {
    now "%Y-%m-%d %H:%M" strftime
    " | " strcat
    j-args 1 @nth strcat
    10 ctos strcat
    JFILE read-file ` strcat
    JFILE ` write-file drop
    "ok" .
} ;

: j-list { "jq" "" kv-set j-foreach } ;
: j-search { "jq" j-args 1 @nth kv-set j-foreach } ;

j-args dup len 0 > ?? 0 @nth | drop "" ]
dup "add" streq ?? drop j-add
| dup "list" streq ?? drop j-list
| dup "search" streq ?? drop j-search
| dup "tail" streq ?? drop j-tail
| dup "del" streq ?? drop j-del
| drop "usage: j add text | j list | j tail [n] | j search word | j del N" .
] ] ] ]
