// OS utilities via capability tokens.
// Requires --allow-env / --allow-exec flags to bind capabilities.
//   Cap 4:  @env  (name → value)
//   Cap 5:  @exec (command → [status stdout stderr]), streams trimmed
//   Cap 10: @exec-raw — same, streams verbatim (for byte-exact forwarding)

: getenv  { @4 } ;    // name → value (empty string if unset)
: exec    { @5 } ;    // command → [status stdout stderr]
: exec-raw { @10 } ;  // 同上，但两个流**不 trim**（逐字节转发用）

export getenv
export exec
export exec-raw
