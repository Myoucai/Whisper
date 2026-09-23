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
// 范围判定用 dup/over 取回原值。原写法 `_ 96 > _ 123 < &` 里第二个 `_` 复制的
// 是**上一步的布尔**而不是字符，于是条件退化成 `c > 96`——所有 >96 的字节都被
// 减 32，非 ASCII 一律被改坏（S-1 验证时发现；两链一致地错）。
: upper-char { dup 96 > over 123 < & ?? 32 - ] } ;
: lower-char { dup 64 > over 91 < & ?? 32 + ] } ;
: upper      { strchars { upper-char } @map charsstr } ;
: lower      { strchars { lower-char } @map charsstr } ;
: capitalize { _ strlen 0 = ?? drop "" | _ 0 strnth upper-char ctos ` 1 9999 strslice strcat ] } ;
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
