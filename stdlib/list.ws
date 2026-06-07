: length  { len } ;
: push    { append } ;
: map     { @map } ;
: each    { @each } ;
: fold    { @fold } ;
: sum     { 0 { + } @fold } ;
: product { 1 { * } @fold } ;

export length
export push
export map
export each
export fold
export sum
export product
