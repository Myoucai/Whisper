// Whisper Math Standard Library — Token-optimized utilities
// Import with: import std/math

// ── Basic arithmetic shortcuts ──
: sq        { _ * } ;              // square: n → n²        (2→1 tokens, saves 50%)
: cube      { _ sq * } ;           // cube: n → n³
: double    { _ + } ;              // double: n → 2n        (2→1 tokens)
: halve     { 2 / } ;              // halve: n → n/2
: inc       { 1 + } ;              // increment: n → n+1    (2→1 tokens)
: dec       { 1 - } ;              // decrement: n → n-1    (2→1 tokens)
: neg       { 0 swap - } ;         // negate: n → -n        (3→1 tokens, saves 67%)

// ── Absolute value ──
: abs       { _ 0 < ?? 0 swap - ] } ;  // abs: n → |n|     (8→1 tokens, saves 87%)

// ── Min/max ──
: max       { _ over > ?? ] drop | drop ] } ;   // max: a b → max(a,b)  (7→1)
: min       { _ over < ?? ] drop | drop ] } ;   // min: a b → min(a,b)  (7→1)

// ── Clamp ──
: clamp     { _ over > ?? drop swap | _ over < ?? drop ] | ] ] } ;  // clamp: val lo hi → clamped

// ── Predicates ──
: even?     { _ 2 % 0 = } ;        // is even?               (5→1 tokens)
: odd?      { _ 2 % 1 = } ;        // is odd?                (5→1 tokens)
: zero?     { _ 0 = } ;            // is zero?
: positive? { _ 0 > } ;            // is positive?
: negative? { _ 0 < } ;            // is negative?

// ── Power ──
: pow       { _ 0 = ?? drop 1 | _ 1 - ` _ ` pow * ] } ;  // pow: base exp → base^exp

// ── Bit operations ──
: bit-not   { -1 ^ } ;             // bitwise NOT
: lshift    { << } ;               // left shift (alias)
: rshift    { >> } ;               // right shift (alias)

// ── Trig / Float (wrap core ops) ──
: sqrt      { fsqrt } ;            // square root
: f2i       { f64toi64 } ;         // float to int
: i2f       { i64tof64 } ;         // int to float

// ── Range helpers ──
: between?  { rot drop _ over <= swap _ >= & } ;  // between?: val lo hi → bool

export sq
export cube
export double
export halve
export inc
export dec
export neg
export abs
export max
export min
export clamp
export even?
export odd?
export zero?
export positive?
export negative?
export pow
export bit-not
export lshift
export rshift
export sqrt
export f2i
export i2f
export between?
