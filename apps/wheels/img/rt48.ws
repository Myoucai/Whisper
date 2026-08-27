import "std/ffi"
import "imgj"
"big48.jpg" jpeg-decode
"pix" kv-get
"W" kv-get strtoi64
"H" kv-get strtoi64
92
jpeg-encode
"round48.jpg" ` write-file drop
"done" .
