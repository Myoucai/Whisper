// WASI security fixture: file access must be confined to preopen /data.
"/data/wsconf.txt" "conf-data!" write-file .
"/data/wsconf.txt" read-file strlen .
