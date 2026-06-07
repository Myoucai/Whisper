// OS utilities via capability tokens.
// Requires --allow-env / --allow-exec flags to bind capabilities.
//   Cap 4: @env  (name → value)
//   Cap 5: @exec (command → [status stdout stderr])

: getenv  { @4 } ;    // name → value (empty string if unset)
: exec    { @5 } ;    // command → [status stdout stderr]

export getenv
export exec
