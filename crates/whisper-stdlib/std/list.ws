// Whisper List Standard Library — Token-optimized utilities
// Import with: import std/list

// ── Aliases (shorter names for common ops) ──
: len       { len } ;              // length of list
: nth       { @nth } ;             // get element at index (alias)
: push      { append } ;           // append to list (alias)
: map       { @map } ;             // map over list
: each      { @each } ;            // iterate list
: fold      { @fold } ;            // fold list

// ── Aggregate ──
: sum       { 0 { + } @fold } ;          // sum of list        (5→1, saves 80%)
: prod      { 1 { * } @fold } ;          // product of list    (5→1, saves 80%)
: max-val   { _ 0 @nth swap { _ over > ?? swap | drop ] } @each drop } ;  // max in list
: min-val   { _ 0 @nth swap { _ over < ?? swap | drop ] } @each drop } ;  // min in list
: mean      { _ sum swap len / } ;       // mean of list

// ── Access ──
: first     { 0 @nth } ;                 // first element       (2→1)
: second    { 1 @nth } ;                 // second element      (2→1)
: third     { 2 @nth } ;                 // third element       (2→1)
: last      { _ len 1 - @nth } ;         // last element        (6→1, saves 83%)
: tail      { 1 swap len 1 - strslice } ; // all but first      (→1)
: init      { _ len 1 - 0 swap strslice } ; // all but last

// ── Predicates ──
: empty?    { len 0 = } ;                // is list empty?
: contains? { 0 swap { over over len < } { over over @nth over = ?? drop drop #t | 1 + ] } # drop drop #f } ;  // contains? elem → bool
: all?      { { } @map 0 { & } @fold } ; // all elements satisfy predicate
: any?      { { } @map 0 { | } @fold } ; // any element satisfies predicate

// ── Transform ──
: rev       { [] { swap append } @fold } ;   // reverse list    (→1)
: sort      { _ len 1 <= ?? ] _ 0 @nth over { _ over < } @filter sort swap over { _ over >= } @filter sort append } ;  // quicksort
: take      { swap 0 swap strslice } ;       // take first n
: drop-n    { swap swap len swap - swap strslice } ; // drop first n
: dedup     { [] swap { len 0 > } { over over 0 @nth { over = } @filter len 0 = ?? over 0 @nth append ] 1 strslice } # drop } ;  // remove duplicates

// ── Build ──
: range     { [] swap { _ 0 > } { over over append swap 1 - swap } # drop } ;  // range n → [n n-1 ... 1]
: range-to  { [] swap 1 swap { over over <= } { over over append 1 + } # drop drop } ;  // range-to n → [1 2 ... n]

export len
export nth
export push
export map
export each
export fold
export sum
export prod
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
