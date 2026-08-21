// Minimal test: compile a simple program through cgen
// Load cgen module
"cgen.ws" file-read eval
"tools.ws" file-read eval

// Test bytecodes: 1 + 2 → output
// [50, 1, 50, 2, 16, 144]  (PushInt 1, PushInt 2, Add, Print)
// PushInt = [50, value], Add = [16], Print = [144]
// These are all bare opcodes, no control flow
: test-bytecodes [[50 1] [50 2] [16] [144]] ;

// Test emit-func-body with simple bytecodes
: test-emit-func-body {
    test-bytecodes emit-func-body
} ;

// Test emit-func
: test-emit-func {
    "test_add" test-bytecodes emit-func
} ;

// Run
test-emit-func-body print
