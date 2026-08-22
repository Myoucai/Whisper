// expect: 10|10|0
"D:/tmp/wsconf.txt" "conf-data!" write-file .
"D:/tmp/wsconf.txt" read-file strlen .
"D:/tmp" "sub" strcat getenv strlen .
