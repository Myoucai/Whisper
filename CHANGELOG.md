# Changelog

## v1.0.0 (2026-06-06) — First Stable Release

### Language Syntax (Frozen)

| Category | Operators |
|----------|-----------|
| Stack | `_` dup, `` ` `` swap, `drop`, `@` rot, `$n` pick |
| Arithmetic | `+` `-` `*` `/` `%` (mod) |
| Comparison | `=` `<` `>` `!=` `<=` `>=` |
| Logic | `&` and, `\|` or, `!` not |
| Control | `??...\|...]` if/else, `#` loop, `?->` single-branch |
| Lists | `[ ... ]` literal, `@nth` `append` `len` `@map` `@each` `@fold` |
| Words | `: name { body } ;` define, `import` `export` |
| Capability | `@n` call, `!` exec |
| Confidence | `:0.xx` label, `?\|` probabilistic choice |
| IO | `.` output top, `..` output all, `,` read input |

### Core Features

- **VM**: 56+ opcodes, stack-based, call/return frames, recursion
- **Parser**: Recursive descent lexer + Pratt parser for full .ws syntax
- **Type Checker**: Stack-effect validation at compile time
- **Compiler**: Two-pass bytecode generation with constant folding & peephole optimization
- **.wbin Format**: LEB128-encoded binary with magic header
- **WASM Target**: Direct compilation to browser-executable .wasm (90 bytes for hello world)
- **Capability Sandbox**: File I/O, HTTP GET/POST with host/path whitelists

### Self-Hosting

- `whisperc/main.ws`: Whisper compiler written in Whisper (10 lines)
- `whisper bootstrap`: Rust Lexer → Whisper Compiler → VM Execute pipeline
- hello.ws self-compilation verified

### CLI Commands

```
whisper run       file.ws    Execute
whisper build     file.ws    Compile to .wbin/.wasm
whisper check     file.ws    Type check
whisper serve     file.ws    HTTP server
whisper repl                 Interactive REPL
whisper fmt       file.ws    Format
whisper install   pkg        Package manager
whisper bootstrap file.ws    Self-hosting pipeline
```

### Standard Library

| Module | Functions |
|--------|-----------|
| `std/math` | sq, cube, abs, factorial, fib, even, odd |
| `std/str` | strlen, strcat, strdup |
| `std/list` | length, push, map, each, fold, sum, product, reverse |
| `std/io` | read-file, write-file, println |
| `std/json` | json-parse, json-stringify |
| `std/test` | assert-true, assert-false, assert-eq |

### Tooling

- VS Code extension: syntax highlighting, bracket matching, code folding
- Web Playground: interactive editor with built-in examples
- CI/CD: GitHub Actions (test + lint + release)
- Package registry: Git-based with capability review

### Testing

- 63 unit/integration tests
- 2500+ fuzz tests (random program generation)
- 6 working examples: hello, factorial, fib, sum, map, http_get

### Performance

- Constant folding: `3 4 +` → `7` at compile time
- Peephole optimization: `dup drop` → (nothing), `0 add` → (nothing)
- Strength reduction: negative add → subtract
- Direct WASM compilation: 90 bytes vs 2800+ with embedded interpreter

## v0.2.0

- Breaking: `%` changed to Modulo, `drop` keyword added for stack discard
- Self-hosting compiler (whisperc), bootstrap pipeline
- Package manager (whisper install), VS Code extension
- WASM end-to-end verification, conditional branching fixes
- List ordering fix, recursive functions, standard library expansion

## v0.1.0

- Initial release: core VM, parser, compiler, .wbin format, basic CLI
