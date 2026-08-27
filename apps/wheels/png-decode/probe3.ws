import "pngdec"
"grad.png" read-file "s" ` kv-set
"s" kv-get 16 be32-of .
"s" kv-get strlen .
