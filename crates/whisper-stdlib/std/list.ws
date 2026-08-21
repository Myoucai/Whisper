// Whisper List Standard Library
: length    { len } ;
: nth       { @nth } ;
: push      { append } ;
: map       { @map } ;
: each      { @each } ;
: fold      { @fold } ;
: sum       { 0 { + } @fold } ;
: prod      { 1 { * } @fold } ;
: product   { prod } ;
: max-val   { _ 0 @nth ` { _ over > ?? ` | drop ] } @each drop } ;
: min-val   { _ 0 @nth ` { _ over < ?? ` | drop ] } @each drop } ;
: mean      { _ sum ` length / } ;
: first     { 0 @nth } ;
: second    { 1 @nth } ;
: third     { 2 @nth } ;
: last      { _ length 1 - @nth } ;
: tail      { _ 1 ` len 1 - list-slice } ;
: init      { _ 0 ` len 1 - list-slice } ;
: empty?    { length 0 = } ;
: contains? { #f ` { _ $2 = ?? drop #t | ] } @each } ;
: all?      { { } @map 0 { & } @fold } ;
: any?      { { } @map 0 { | } @fold } ;
: rev       { [] { ` append } @fold } ;
: sort      { [] { insert-sorted } @fold } ;
: insert-sorted { _ len 0 = ?? ` append | ` _ $2 >= ?? append | _ last ` init $3 insert-sorted ` append append ] ] } ;
: take      { [] ` { _ 0 > } { ` _ over @ 0 @nth ` ` append ` 1 ` len 1 - list-slice ` 1 - } # drop drop rev } ;
: drop-n    { _ 0 = ?? drop | ` _ len 0 = ?? drop | _ 1 ` len 1 - list-slice ` 1 - drop-n ] ] } ;
: dedup     { [] { insert-dedup } @fold } ;
: insert-dedup { _ len 0 = ?? ` append | ` _ last = ?? drop | ` ` init $2 insert-dedup ` last append append ] ] } ;
: range     { [] ` { _ 0 > } { over over append ` 1 - ` } # drop } ;
: range-to  { [] ` 1 ` { over over <= } { over over append 1 + } # drop drop } ;

export length
export nth
export push
export map
export each
export fold
export sum
export prod
export product
export max-val
export min-val
export mean
export first
export second
export third
export last
export tail
export init
export empty?
export contains?
export all?
export any?
export rev
export sort
export insert-sorted
export take
export drop-n
export dedup
export insert-dedup
export range
export range-to
