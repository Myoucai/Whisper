// journal — 命令行日记
//   j add "一句话"     追加一条（自动加时间戳）
//   j list            列出全部
//   j search 关键词    检索
import "std/str"

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
" strsplit
    { dup "jq" kv-get strfind 0 >= ?? . | drop ] } @each
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
| drop "usage: j add text | j list | j search word" .
] ] ]
