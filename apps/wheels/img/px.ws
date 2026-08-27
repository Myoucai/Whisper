import "std/ffi"
import "imgj"
"t.jpg" jpeg-decode
// 打印像素
"y" "0" kv-set
{ "y" kv-get strtoi64 "H" kv-get strtoi64 < } {
  "x" "0" kv-set
  { "x" kv-get strtoi64 "W" kv-get strtoi64 < } {
    "pix" kv-get "y" kv-get strtoi64 "W" kv-get strtoi64 * "x" kv-get strtoi64 3 * + 3 strslice
    dup 0 strnth i64tostr " " strcat over 1 strnth i64tostr strcat " " strcat ` 2 strnth i64tostr strcat .
    "x" kv-get strtoi64 1 + i64tostr "x" ` kv-set
  } #
  "y" kv-get strtoi64 1 + i64tostr "y" ` kv-set
} #
