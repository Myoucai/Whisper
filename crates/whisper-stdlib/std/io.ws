# std/io — File I/O operations
# Requires: @file_read, @file_write capabilities

: read-file { @0 ! } ;                  # path → content (uses @file_read)
: write-file { @1 ! } ;                 # path content → (uses @file_write)
: println { . "\n" . ; }               # value → (print with newline)

export read-file
export write-file
export println
