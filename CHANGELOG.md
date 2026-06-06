# Changelog

## v0.2.0 (2026-06-06)

### Breaking Changes
- `%` is now always Modulo (arithmetic). Use `drop` keyword for stack discard.
- `mod` keyword added as alias for `%`.
- `drop` keyword added for explicit stack discard.

### New Features
- **Performance optimizer**: constant folding, peephole optimization, strength reduction
- **Self-hosting compiler** (`whisperc`): Whisper compiler written in Whisper
- **Bootstrap pipeline**: `whisper bootstrap <file.ws>` — Rust Lexer → Whisper Compiler → VM Execute
- **Package manager**: `whisper install <github.com/user/repo>` with Git clone + capability review
- **VS Code extension**: syntax highlighting, bracket matching, code folding for `.ws` files
- **WASM end-to-end verification**: bytecode roundtrip, section validation, 5 verification tests
- **WASM opcodes**: Cond, Jump, Swap, PushStr support in WASM interpreter
- **Conditional branching**: `??true|false]` syntax fully working with correct offset calculation
- **Recursive functions**: factorial, fibonacci with proper stack management
- **List operations**: @map with correct element ordering (count-after-elements fix)
- **Standard library**: sq, cube, abs, factorial, fib, even, odd in std/math.ws
- `whisper bootstrap` command for self-hosting compiler pipeline
- `whisper install --local` for local package installation
- `whisper install --list` to show installed packages

### Bug Fixes
- List element ordering: count emitted after elements for correct LIFO pop
- Conditional offset calculation: `then_len + 1` instead of `then_len + 2`
- Recursive fib: removed unnecessary drop in else branch (Cond already pops bool)
- Parser: fixed infinite recursion on `[` and `{` parsing
- Word definitions: two-pass compilation with proper word_dict inheritance
- Call/Return: VM now looks up word_dict at runtime for Call(String)

### Technical
- 50 tests: 49 passed, 1 ignored
- 5 working examples: factorial, fib, sum, map, hello
- Remove clap dependency (hand-written argument parser for Windows GNU compat)
- Remove WASM encoder dependency (direct WASM binary generation)
- Opcode::Call(String) for named word calls with VM word_dict lookup
- Opcode::PushRef(Vec<Opcode>) for inline ref bytecode

## v0.1.0 (initial)

- Core stack-based VM with 56+ opcodes
- Recursive descent lexer + parser
- Type checker with Union-Find constraint solving
- Bytecode compiler with .wbin binary format (LEB128)
- WASM code generator (minimal interpreter)
- Capability-based security sandbox
- CLI: run, build, check, repl, fmt
- Standard library: math, str, list, io, json, test
- Package manager framework
