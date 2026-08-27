import "std/ffi"
: TJ { "C:/msys64/mingw64/bin/libturbojpeg.dll" } ;
"t.jpg" read-file "j" ` kv-set
TJ "tjInitDecompress" ffi-fn [] "i" rot ffi-call
8 buf-alloc 8 buf-alloc 8 buf-alloc 8 buf-alloc
TJ "tjDecompressHeader3" ffi-fn
[] $6 append "j" kv-get append "j" kv-get strlen append $5 append $4 append $3 append $2 append
"ippppppp" rot ffi-call
$4 buf-peek 4294967295 bitand i64tostr "W" ` kv-set
$3 buf-peek 4294967295 bitand i64tostr "H" ` kv-set
drop
"W" kv-get strtoi64 "H" kv-get strtoi64 * 3 * buf-alloc
TJ "tjDecompress2" ffi-fn
[] $7 append "j" kv-get append "j" kv-get strlen append $2 append "W" kv-get strtoi64 append 0 append "H" kv-get strtoi64 append 0 append 0 append
"ippiiiiiii" rot ffi-call
"rc=" . .
dup buf-peek 255 bitand .
drop
