: tok-type { 0 @nth } ;
: tok-val  { 1 @nth } ;

: compile-int { tok-val [48] ` append } ;
: compile-str { tok-val [50] ` append } ;
: compile-op  { tok-val } ;
: compile-wrd { tok-val [96] ` append } ;
: compile-pushlist { tok-val [52] ` append } ;

: dispatch {
    _ tok-type 0 = ??compile-int
    |_ tok-type 2 = ??compile-str
    |_ tok-type 3 = ??compile-op
    |_ tok-type 4 = ??compile-wrd
    |_ tok-type 14 = ??compile-pushlist
    |drop 0 ] ] ] ] ]
} ;

: compile { { dispatch } @map } ;

export compile
