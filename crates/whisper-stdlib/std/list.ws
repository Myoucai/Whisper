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
: tail      { 1 ` length 1 - strslice } ;
: init      { _ length 1 - 0 ` strslice } ;
: empty?    { length 0 = } ;
: contains? { 0 ` { over over length < } { over over @nth over = ?? drop drop #t | 1 + ] } # drop drop #f } ;
: all?      { { } @map 0 { & } @fold } ;
: any?      { { } @map 0 { | } @fold } ;
: rev       { [] { ` append } @fold } ;
: sort      { _ length 1 <= ?? | _ 0 @nth over { _ over < } @filter sort ` over { _ over >= } @filter sort append ] } ;
: take      { ` 0 ` strslice } ;
: drop-n    { ` swap length ` - ` strslice } ;
: dedup     { [] ` { length 0 > } { over over 0 @nth { over = } @filter length 0 = ?? over 0 @nth append | ] 1 strslice } # drop } ;
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
export take
export drop-n
export dedup
export range
export range-to
