import "std/ffi"
import "imgj"
import "D:/Whisper/apps/wheels/png-decode/pngdec"
"D:/Whisper/apps/wheels/img/big.png" png-decode
"img" kv-get "W" kv-get strtoi64 "H" kv-get strtoi64 90 jpeg-encode
"big48.jpg" ` write-file drop
"encoded" .
