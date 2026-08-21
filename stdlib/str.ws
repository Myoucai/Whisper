// Whisper String Standard Library
: strlen     { strlen } ;
: strcat     { strcat } ;
: strdup     { "" _ strcat } ;
: streq      { streq } ;
: strlt      { strlt } ;
: find       { strfind } ;
: replace    { strreplace } ;
: to-int     { strtoi64 } ;
: from-int   { i64tostr } ;
: nth        { strnth } ;
: slice      { strslice } ;
: empty?     { strlen 0 = } ;
: contains?  { strfind 0 >= } ;
: starts-with? { 0 ` strlen strslice ` streq } ;
: ends-with? { _ strlen over strlen - ` strslice ` streq } ;
: str-rev    { strchars rev charsstr } ;
: upper-char { _ 96 > _ 123 < & ?? _ 32 - ` drop | ] } ;
: lower-char { _ 64 > _ 91 < & ?? _ 32 + ` drop | ] } ;
: upper      { strchars { upper-char } @map charsstr } ;
: lower      { strchars { lower-char } @map charsstr } ;
: capitalize { _ strlen 0 = ?? "" | _ 0 strnth upper-char ctos ` 1 9999 strslice strcat ] } ;
: repeat     { _ 0 = ?? drop drop "" | _ 1 - over ` repeat strcat ] } ;
: join       { strjoin } ;
: chars      { strchars } ;
: from-chars { charsstr } ;
: lines      { "\n" strsplit } ;
: words      { " " strsplit } ;
: first      { 0 strnth ctos } ;
: last       { _ strlen 1 - strnth ctos } ;
: rest       { 1 9999 strslice } ;
: palindrome? { _ str-rev streq } ;
export strlen
export strcat
export strdup
export streq
export strlt
export find
export replace
export to-int
export from-int
export nth
export slice
export empty?
export contains?
export starts-with?
export ends-with?
export str-rev
export upper
export lower
export upper-char
export lower-char
export capitalize
export repeat
export join
export chars
export from-chars
export lines
export words
export first
export last
export rest
export palindrome?
