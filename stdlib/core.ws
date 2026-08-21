// Whisper core extras — words provided by the Rust VM at startup that
// are not already in std/math or std/list. c2 auto-imports this module;
// keep in sync with crates/whisper-cli/src/commands/run.rs.

: gcd { _ 0 = ?? drop | ` over % gcd ] } ;
: has? { strfind 0 >= } ;

export gcd
export has?
