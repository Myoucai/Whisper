: println   { . } ;
: read-file  { @0 ! } ;   // requires @file_read capability bound to slot 0
: write-file { @1 ! } ;   // requires @file_write capability bound to slot 1

export println
export read-file
export write-file
