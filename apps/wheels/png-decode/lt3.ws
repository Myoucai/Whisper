"a" "5" kv-set
"b" "5" kv-set
{ "eq" . } { "a" kv-get strtoi64 "b" kv-get strtoi64 > } #
"after" .
