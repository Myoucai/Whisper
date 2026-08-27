"i" "0" kv-set
{ "i" kv-get . "i" kv-get strtoi64 1 + i64tostr "i" ` kv-set } { "i" kv-get strtoi64 3 < } #
"done" .
