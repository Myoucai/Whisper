: length  { len } ;
: push    { append } ;
: map     { @map } ;
: each    { @each } ;
: fold    { @fold } ;
: sum     { 0 { + } @fold } ;
: product { 1 { * } @fold } ;

export length push map each fold sum product
