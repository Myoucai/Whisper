# std/json — JSON parsing and generation
# No capabilities required (pure computation)

# JSON is represented as nested lists and strings
# Example: {"key": "value"} → ["key" "value"]

: json-parse { } ;                      # str → json-value (stub)
: json-stringify { } ;                  # json-value → str (stub)

export json-parse
export json-stringify
