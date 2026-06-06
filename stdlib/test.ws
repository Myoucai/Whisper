: assert-true  { ??"PASS"|"FAIL"] . } ;
: assert-false { ! assert-true } ;
: assert-eq    { = assert-true } ;

export assert-true assert-false assert-eq
