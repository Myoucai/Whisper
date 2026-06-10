// Whisper String Standard Library — Token-optimized utilities
// Import with: import std/str

// ── Aliases (shorter names for common string ops) ──
: len       { strlen } ;            // string length
: cat       { strcat } ;            // concatenate (shorter alias)
: eq?       { streq } ;             // string equality check
: lt?       { strlt } ;             // string less-than
: find      { strfind } ;           // find substring
: replace   { strreplace } ;        // replace substring
: to-int    { strtoi64 } ;          // parse to int
: from-int  { i64tostr } ;          // format int to string
: nth       { strnth } ;            // char code at index
: slice     { strslice } ;          // substring

// ── Predicates ──
: empty?    { strlen 0 = } ;        // is string empty?       (5→1, saves 80%)
: contains? { strfind 0 >= } ;      // contains substring?    (4→1, saves 75%)
: starts-with? { 0 swap strlen strslice swap streq } ;  // starts with?
: ends-with? { _ strlen over strlen - swap strslice swap streq } ;  // ends with?

// ── Transform ──
: rev       { strchars [] { swap append } @fold charsstr } ;  // reverse string
: upper     { strchars { _ 97 >= swap 122 <= & ?? _ 32 - | ] } @map charsstr } ;  // uppercase
: lower     { strchars { _ 65 >= swap 90 <= & ?? _ 32 + | ] } @map charsstr } ;  // lowercase
: capitalize { 0 strnth 32 - ctos 1 strslice strcat } ;       // capitalize first letter
: trim      { _ 0 strnth 32 = ?? 1 strslice trim | _ strlen 1 - strnth 32 = ?? _ strlen 1 - 0 swap strslice trim | ] ] } ;  // trim spaces
: repeat    { _ 0 = ?? drop drop "" | _ 1 - over ` repeat strcat ] } ;  // repeat s n times

// ── Split / Join ──
: join      { strjoin } ;           // join list of strings
: chars     { strchars } ;          // string → char codes
: from-chars { charsstr } ;         // char codes → string
: lines     { "\n" strsplit } ;     // split by newline (if available)
: words     { " " strsplit } ;      // split by space (if available)

// ── Access ──
: first     { 0 strnth } ;          // first char code
: last      { _ strlen 1 - strnth } ; // last char code
: rest      { 1 9999 strslice } ;   // all but first char

// ── Format ──
: format    { swap strcat } ;       // format: "prefix" val → "prefix{val}"
: pad-left  { _ strlen - 0 > ?? { _ " " strcat } @times strcat | drop ] } ;  // pad with spaces

// ── Palindrome check ──
: palindrome? { _ rev streq } ;     // is palindrome?

export len
export cat
export eq?
export lt?
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
export trim
export repeat
export join
export chars
export from-chars
export lines
export words
export first
export last
export rest
export format
export pad-left
export palindrome?
