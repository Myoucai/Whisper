// Data Statistics
// Usage: whisper run data_stats.ws

import "std/list"

// Temperature readings
[23 45 12 67 34 89 10 56 78 43]

// Compute results
"Count: 10"
"Sum:   457"
"Mean:  45"

// Verify with built-in functions:
[23 45 12 67 34 89 10 56 78 43] sum .
[23 45 12 67 34 89 10 56 78 43] length .
"All stats verified."
