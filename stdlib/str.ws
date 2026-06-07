: strlen   { strlen } ;
: strcat   { strcat } ;
: strdup   { _ strcat } ;

export strlen
export strcat
export strdup
