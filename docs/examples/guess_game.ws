// Number Guessing Game
// Usage: whisper repl — then paste the definitions and play interactively.
// Type:  50 guess  (and follow hints)

import "std/str"
import "std/test"

// Secret number
: secret { 42 } ;

// One guess: actual → hint
: guess {
    _ secret = ??"CORRECT!"
    |_ secret < ??"Too low, try higher"
    |"Too high, try lower"] ]
    .
} ;

// Example guesses (uncomment to test):
// 50 guess    → Too high, try lower
// 30 guess    → Too low, try higher
// 42 guess    → CORRECT!

// Test the game logic
50 guess
30 guess
42 guess
"Game over."
