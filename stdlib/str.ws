: strlen     { strlen } ;
: strcat     { strcat } ;
: strdup     { _ strcat } ;
: streq      { streq } ;
: strlt      { strlt } ;
: strfind    { strfind } ;
: strreplace { strreplace } ;
: strtoi64   { strtoi64 } ;
: i64tostr   { i64tostr } ;

export strlen
export strcat
export strdup
export streq
export strlt
export strfind
export strreplace
export strtoi64
export i64tostr
