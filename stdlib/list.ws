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
: max-val   { dup 0 @nth { _ $2 > ?? ` drop | drop ] } @fold } ;
: min-val   { dup 0 @nth { _ $2 < ?? ` drop | drop ] } @fold } ;
: mean      { _ sum ` length / } ;
: first     { 0 @nth } ;
: second    { 1 @nth } ;
: third     { 2 @nth } ;
: last      { _ len 1 - @nth } ;
: tail      { 1 drop-n } ;
: init      { _ len 1 - take } ;
: empty?    { len 0 = } ;
: contains? { #f ` { $1 = ?? drop #t | ] } @fold } ;
: all?      { #t ` { & } @fold } ;
: any?      { #f ` { | } @fold } ;
: range     { _ 0 > ?? _ 1 - range _ append | [] ] } ;
: range-to  { 1 ` range } ;   // n -> [1..n]（range 内建预分配，替代递归 append 的 O(n²)）
: filter    { @filter } ;
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
export range
export range-to
export filter
