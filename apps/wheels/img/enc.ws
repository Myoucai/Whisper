import "std/ffi"
import "imgj"
// 回环：t.jpg 解码 → 重编码 q=85 → 再解码 → 字节串写文件
"t.jpg" jpeg-decode
"pix" kv-get
"W" kv-get strtoi64
"H" kv-get strtoi64
85
jpeg-encode
"round.jpg" ` write-file drop
"ok" .
