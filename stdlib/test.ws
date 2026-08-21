// Whisper Test Framework
: assert-eq    { = ?? "PASS" . | "FAIL" . ] } ;
: assert-neq   { != ?? "PASS" . | "FAIL" . ] } ;
: assert-true  { ?? "PASS" . | "FAIL" . ] } ;
: assert-false { ! ?? "PASS" . | "FAIL" . ] } ;
: assert-throws { try ` drop ?? "FAIL" . | drop "PASS" . ] } ;
: test-pass    { "PASS" . } ;
: test-fail    { "FAIL" . } ;
: test-group   { "=== " ` strcat " ===" strcat . } ;

export assert-eq
export assert-neq
export assert-true
export assert-false
export assert-throws
export test-pass
export test-fail
export test-group
