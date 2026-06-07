// HTTP GET example
// Usage: whisper run --allow-http http_get.ws
// Fetches a public API and displays the result.

import "std/http"
import "std/json"

// Fetch data from JSONPlaceholder test API
"https://jsonplaceholder.typicode.com/posts/1" http-get

// Parse and display
"Response:" .
json-parse .
