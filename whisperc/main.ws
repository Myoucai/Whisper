
: tk-type { 0 @nth } ;
: tk-val  { 1 @nth } ;

: op-i64  { tk-val [48] ` append } ;
: op-f64  { tk-val [49] ` append } ;
: op-str  { tk-val [50] ` append } ;
: op-bool { tk-val [51] ` append } ;
: op-list { tk-val [52] ` append } ;
: op-op   { tk-val } ;
: op-call { tk-val [96] ` append } ;
: op-ref  { tk-val } ;

: compile-one {
    _ tk-type
    0  = ??op-i64
    |_ tk-type 1  = ??op-f64
    |_ tk-type 2  = ??op-str
    |_ tk-type 3  = ??op-op
    |_ tk-type 4  = ??op-call
    |_ tk-type 13 = ??op-bool
    |_ tk-type 14 = ??op-list
    |_ tk-type 18 = ??op-ref
    |drop drop ] ] ] ] ] ] ] ]
} ;

: compile {
    { compile-one } @map
} ;

export compile
