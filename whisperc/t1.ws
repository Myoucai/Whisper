// Whisper Parser v1.0 — pair-based state [tokens, pos]
// State is a single value on the stack: a [tokens_list, position] pair.

: tk-type  { 0 @nth } ;
: tk-val   { 1 @nth } ;
: st-toks  { 0 @nth } ;   // state → tokens
