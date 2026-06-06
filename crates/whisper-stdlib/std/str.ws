# std/str — String processing
# No capabilities required (pure computation)

: strlen { len } ;                      # str → length
: strcat { append } ;                   # str1 str2 → str1+str2
: strdup { _ append } ;                 # str → str+str

export strlen
export strcat
export strdup
