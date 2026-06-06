# std/test — Simple testing framework
# No capabilities required (pure computation)

: assert-true { "PASS" "FAIL" ?? .|.]] } ;
: assert-false { "PASS" "FAIL" ?? .|.]] ! } ;
: assert-eq { = "PASS" "FAIL" ?? .|.]] } ;

export assert-true
export assert-false
export assert-eq
