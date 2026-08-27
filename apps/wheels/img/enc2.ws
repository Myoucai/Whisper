import "std/ffi"
import "imgj"
"t.jpg" jpeg-decode
"pix" kv-get "W" kv-get strtoi64 "H" kv-get strtoi64 85 jpeg-encode strlen .
