// Whisper String Standard Library
// Built-in operators (strlen, strcat, streq, strfind, strreplace, strtoi64,
// i64tostr, strnth, strslice, strchars, charsstr, strjoin) are available directly.

: strdup     { "" strcat } ;
: str-empty? { strlen 0 = } ;
: str-contains? { strfind 0 >= } ;
: starts-with? { 0 ` strlen strslice ` streq } ;
: ends-with? { _ strlen over strlen - ` strslice ` streq } ;

// Case conversion helpers (isolated @map body sees only char code)
: upper-char { _ _ 96 > _ 123 < & ?? _ 32 - | ] } ;
: lower-char { _ _ 64 > _ 91 < & ?? _ 32 + | ] } ;

: upper      { strchars { upper-char } @map charsstr } ;
: lower      { strchars { lower-char } @map charsstr } ;
: capitalize { 0 strnth 32 - ctos 1 strslice strcat } ;
: join       { strjoin } ;
: chars      { strchars } ;
: from-chars { charsstr } ;
: lines      { "
" strsplit } ;
: words      { " " strsplit } ;
: first-char { 0 strnth ctos } ;
: last-char  { _ strlen 1 - strnth ctos } ;
: rest       { 1 9999 strslice } ;

export strdup str-empty? str-contains? starts-with? ends-with?
export upper lower upper-char lower-char capitalize
export join chars from-chars lines words first-char last-char rest
