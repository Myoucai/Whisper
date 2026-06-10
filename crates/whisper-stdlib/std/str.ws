// Whisper String Standard Library
: strlen     { strlen } ;
: strcat     { strcat } ;
: strdup     { _ strcat } ;
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
: rev        { strchars [] { ` append } @fold charsstr } ;
: upper      { strchars { _ 97 >= ` 122 <= & ?? _ 32 - | ] } @map charsstr } ;
: lower      { strchars { _ 65 >= ` 90 <= & ?? _ 32 + | ] } @map charsstr } ;
: capitalize { 0 strnth 32 - ctos 1 strslice strcat } ;
: repeat     { _ 0 = ?? drop drop "" | _ 1 - over ` repeat strcat ] } ;
: join       { strjoin } ;
: chars      { strchars } ;
: from-chars { charsstr } ;
: lines      { "\n" strsplit } ;
: words      { " " strsplit } ;
: first      { 0 strnth } ;
: last       { _ strlen 1 - strnth } ;
: rest       { 1 9999 strslice } ;
: palindrome? { _ rev streq } ;

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
export rev
export upper
export lower
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
