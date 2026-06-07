// Data Processing Demo
// Usage: whisper run pipeline.ws

import "std/list"
import "std/math"
import "std/str"

// Demo 1: Map — square each element
"Squares of [1 2 3 4 5]:"
[1 2 3 4 5] { _ * } @map .
// Output: [1 4 9 16 25]

// Demo 2: Fold — sum
"Sum of 1-5:"
[1 2 3 4 5] 0 { + } @fold .
// Output: 15

// Demo 3: Fibonacci
"Fibonacci(10):"
10 fib .
// Output: 55

// Demo 4: String pipeline
"Hello World length:"
"Hello World" strlen .
// Output: 11

"Pipeline complete."
