// JSON API Client
// Usage: whisper run --allow-http api_client.ws
// Fetches and displays a TODO item from JSONPlaceholder.

import "std/http"
import "std/json"

// Fetch a todo item
"https://jsonplaceholder.typicode.com/todos/1" http-get

// Parse JSON response
json-parse

// Display result
"API Response:" .
"Raw:" . .. .
