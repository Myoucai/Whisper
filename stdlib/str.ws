: strlen   { strlen } ;
: strcat   { strcat } ;
: strdup   { _ strcat } ;   # dup string then concatenate

export strlen strcat strdup
