// WASI security fixture: preopen directory ops.
"/data/sub" mkdir .
"/data/x" "hi" write-file .
"/data/x" file-exists .
