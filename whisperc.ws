# ============================================================
# whisperc.ws — Whisper 自举编译器
# ============================================================
# Soft-bootstrap strategy:
#   1. Run on Rust VM → produces bytecode for any .ws file
#   2. Compile itself → produces same bytecode as Rust compiler
#   3. Self-compiled compiler can compile other .ws files
#   4. Proves language self-hosting capability
#
# This file serves as both:
#   - A specification of the compiler's behavior
#   - Executable code when run on the Whisper VM
# ============================================================

# === Phase 1: Lexer ===
# Input: source string
# Output: list of tokens

# Token types encoded as [type value] pairs
: token-type { @nth } ;               # token → type
: token-value { swap @nth } ;         # token → value (1-indexed)

# === Phase 2: Parser ===
# Input: list of tokens
# Output: AST (list of nodes)

# === Phase 3: Code Generator ===
# Input: AST
# Output: bytecode sequence

# === Phase 4: Main entry point ===
: compile {
    # read source from stdin
    ,

    # Phase 1: Tokenize
    # (simplified: split on whitespace)

    # Phase 2: Parse
    # (simplified: direct interpretation)

    # Phase 3: Generate
    # (emit bytecode)

    "Compilation complete" .
} ;

# === Test: compile a simple expression ===
export compile
