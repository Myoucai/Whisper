// expect: 10|10|0
// Relative path: the gate runs in a temp cwd, so this is portable.
"wsconf.txt" "conf-data!" write-file .
"wsconf.txt" read-file strlen .
"WHISPER_NO_SUCH_ENV" getenv strlen .
