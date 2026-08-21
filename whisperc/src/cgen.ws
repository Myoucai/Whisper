// cgen.ws — C Code Generator (no @map/@fold, for self-hosting)
//
// ALL state passed as a LIST to avoid $N index shift bugs.
// mk3(a,b,c) = [c,b,a] (reverses).
//
// CRITICAL RULE: after any push onto the stack, 'over' ($1) shifts
// by +1. Always extract all needed fields from state BEFORE pushing,
// or use 'rot' to bring state back to the top.

// mk3 is defined in compiler.ws — do not redefine here (avoids C duplicate)

// ── C string escaping ─────────────────────────────────────────────
// Build C escape sequences via ctos to avoid Whisper lexer interpreting \n etc.
// Renamed from esc-char to avoid collision with lexer's esc-char in compiler.ws
: cgen-esc-char {
    dup 92 = ?? drop 92 ctos 92 ctos strcat return ]
    dup 34 = ?? drop 92 ctos 34 ctos strcat return ]
    dup 10 = ?? drop 92 ctos 110 ctos strcat return ]
    dup 9  = ?? drop 92 ctos 116 ctos strcat return ]
    dup 13 = ?? drop 92 ctos 114 ctos strcat return ]
    ctos
} ;

// esc-iter: state = [str, pos, acc]
: esc-iter {
    dup 1 @nth over 0 @nth strlen >= ??
        2 @nth return
    ]
    dup 0 @nth over 1 @nth strnth cgen-esc-char
    over 2 @nth swap strcat
    over 0 @nth
    rot 1 @nth 1 +
    swap
    mk3
    esc-iter
} ;

: escape-c-str {
    0 "" swap rot mk3
    esc-iter
} ;

// ── Safe C identifier ────────────────────────────────────────────
: scn-char {
    dup dup 48 >= swap 57 <= & ?? ctos return ]
    dup dup 65 >= swap 90 <= & ?? ctos return ]
    dup dup 97 >= swap 122 <= & ?? ctos return ]
    dup 95 = ?? ctos return ]
    drop "_"
} ;

: scn-iter {
    dup 1 @nth over 0 @nth strlen >= ??
        2 @nth return
    ]
    dup 0 @nth over 1 @nth strnth scn-char
    over 2 @nth swap strcat
    // Access state[0] BEFORE pushing pos+1 (same pattern as esc-iter)
    over 0 @nth
    rot 1 @nth 1 +
    swap
    mk3
    scn-iter
} ;

: safe-c-name {
    0 "" swap rot mk3
    scn-iter
} ;

// ── Label collection (first pass) ────────────────────────────────
: is-cf-op {
    dup list? ??
        0 @nth
        dup 80 = ?? drop 1 return ]
        dup 81 = ?? drop 1 return ]
        dup 82 = ?? drop 1 return ]
        drop 0
    ]
    drop 0
} ;

// collect-labels-iter: state = [bytecodes, pos, acc]
// Uses list-based state. Accesses state fields with dup/over @nth.
// After pushing new values, uses rot to bring state back to top.
: collect-labels-iter {
    dup 0 @nth len over 1 @nth <= ??
        2 @nth return
    ]
    dup 0 @nth over 1 @nth @nth
    dup is-cf-op ??
        // CF-OP: target = element[1] + pos + 1, append to acc
        // Stack: state, element
        1 @nth                        // state, offset
        over 1 @nth                   // state, offset, pos
        1 +                           // state, offset, pos+1
        +                             // state, target
        over 2 @nth                   // state, target, acc
        swap append                    // state, acc'
        // Build new state = [state[0], state[1]+1, acc']
        // Stack: state, acc' (only 2 items — use swap, not rot!)
        swap                          // acc', state
        dup 0 @nth                   // acc', state, bytecodes
        swap                          // acc', bytecodes, state
        dup 1 @nth 1 +              // acc', bytecodes, state, pos+1
        swap                          // acc', bytecodes, pos+1, state
        drop                          // acc', bytecodes, pos+1
        swap                          // acc', pos+1, bytecodes
        mk3                           // [bytecodes, pos+1, acc']
        collect-labels-iter
        return                        // prevent fall-through to non-cf-op
    ]
    // NON-CF-OP: just advance pos, keep same acc
    // Stack: state, element
    drop                              // state
    dup 0 @nth                       // state, bytecodes
    swap                              // bytecodes, state
    dup 1 @nth 1 +                  // bytecodes, state, pos+1
    swap                              // bytecodes, pos+1, state
    2 @nth                           // bytecodes, pos+1, acc
    swap                              // bytecodes, acc, pos+1
    rot                               // acc, pos+1, bytecodes
    mk3                               // [bytecodes, pos+1, acc]
    collect-labels-iter
} ;

: collect-labels {
    [] 0 rot mk3
    collect-labels-iter
} ;

// ── Label lookup ──────────────────────────────────────────────────
// has-label?-iter: state = [labels, pos, target]
: has-label?-iter {
    dup 1 @nth over 0 @nth len >= ??
        drop 0 return
    ]
    dup 0 @nth over 1 @nth @nth over 2 @nth = ??
        drop 1 return
    ]
    [] over 0 @nth append over 1 @nth 1 + append over 2 @nth append swap drop
    has-label?-iter
} ;

: has-label? {
    0 rot mk3
    has-label?-iter
} ;

// ── Emit bare opcode (integer) → C string ────────────────────────
// Pattern: dup N = ?? drop "C" return ]
// ?? pops the Bool; on false the original value stays on stack for next check.
: emit-bare {
    dup 0  = ?? drop "w_dup();\n" return ]
    dup 1  = ?? drop "w_swap();\n" return ]
    dup 2  = ?? drop "w_drop();\n" return ]
    dup 3  = ?? drop "w_rot();\n" return ]
    dup 7  = ?? drop "{int64_t _n=(int64_t)w_pop();w_push((wval)rt_ctos(_n));}\n" return ]
    dup 8  = ?? drop "{int64_t _n=(int64_t)w_pop();int64_t _s=(int64_t)w_pop();wval*_l=(wval*)w_pop();w_push((wval)w_list_slice(_l,_s,_n));}\n" return ]
    dup 16 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a+(int64_t)_b));}\n" return ]
    dup 17 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a-(int64_t)_b));}\n" return ]
    dup 18 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a*(int64_t)_b));}\n" return ]
    dup 24 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a==(int64_t)_b?1L:0L));}\n" return ]
    dup 25 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a<(int64_t)_b?1L:0L));}\n" return ]
    dup 26 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a>(int64_t)_b?1L:0L));}\n" return ]
    dup 28 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a<=(int64_t)_b?1L:0L));}\n" return ]
    dup 29 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a>=(int64_t)_b?1L:0L));}\n" return ]
    dup 32 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a&&(int64_t)_b?1L:0L));}\n" return ]
    dup 33 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a||(int64_t)_b?1L:0L));}\n" return ]
    dup 34 = ?? drop "{wval _a=w_pop();w_push((wval)((int64_t)_a==0?1L:0L));}\n" return ]
    dup 52 = ?? drop "{int64_t _n=(int64_t)w_pop();wval*_l=w_list_new(_n);for(int64_t _i=_n-1;_i>=0;_i--)_l[1+_i]=w_pop();w_push((wval)_l);}\n" return ]
    dup 64 = ?? drop "{int64_t _i=(int64_t)w_pop();wval*_l=(wval*)w_pop();w_push(w_list_nth(_l,_i));}\n" return ]
    dup 65 = ?? drop "{wval _e=w_pop();wval*_l=(wval*)w_pop();w_push((wval)w_list_append(_l,_e));}\n" return ]
    dup 66 = ?? drop "{wval*_l=(wval*)w_pop();w_push((wval)w_list_len(_l));}\n" return ]
    dup 70 = ?? drop "{const char*_s=(const char*)w_pop();w_push((wval)(int64_t)w_str_len(_s));}\n" return ]
    dup 71 = ?? drop "{const char*_b=(const char*)w_pop();const char*_a=(const char*)w_pop();w_push((wval)w_str_concat(_a,_b));}\n" return ]
    dup 72 = ?? drop "{int64_t _n=(int64_t)w_pop();int64_t _s=(int64_t)w_pop();const char*_str=(const char*)w_pop();w_push((wval)w_str_slice(_str,_s,_n));}\n" return ]
    dup 73 = ?? drop "{const char*_b=(const char*)w_pop();const char*_a=(const char*)w_pop();w_push((wval)w_str_eq(_a,_b));}\n" return ]
    dup 77 = ?? drop "{const char*_s=(const char*)w_pop();w_push((wval)w_str_to_i64(_s));}\n" return ]
    dup 78 = ?? drop "{int64_t _n=(int64_t)w_pop();w_push((wval)w_i64_to_str(_n));}\n" return ]
    dup 79 = ?? drop "{int64_t _i=(int64_t)w_pop();const char*_s=(const char*)w_pop();w_push((wval)(int64_t)w_str_nth(_s,_i));}\n" return ]
    dup 97 = ?? drop "return;\n" return ]
    dup 144 = ?? drop "w_print_val(w_pop());\n" return ]
    dup 185 = ?? drop "{wval*_l=(wval*)w_pop();w_push((wval)w_charsstr(_l));}\n" return ]
    dup 186 = ?? drop "{const char*_s=(const char*)w_pop();w_striter(_s);}\n" return ]
    dup 228 = ?? drop "w_list_();\n" return ]
    dup 230 = ?? drop "w_str_();\n" return ]
    dup 232 = ?? drop "w_push((wval)w_read_all());\n" return ]
    dup 240 = ?? drop "{int64_t _start=(int64_t)w_pop();wval*_toks=(wval*)w_pop();wval*_th=0,*_el=0;int _next=0;split_cond(_toks,(int)_start,&_th,&_el,&_next);wval*_r=w_list_new(3);_r[1]=(wval)_th;_r[2]=(wval)_el;_r[3]=(wval)(int64_t)_next;w_push((wval)_r);}\n" return ]
    drop "// unknown bare opcode\n"
} ;

// ── Emit control flow instruction ────────────────────────────────
// Stack: (element, pos). Both consumed. Returns C string.
: emit-cf {
    over 0 @nth              // element, pos, opcode
    rot 1 @nth              // pos, opcode, offset
    rot                     // opcode, offset, pos
    1 +                     // opcode, offset, pos+1
    +                       // opcode, target
    i64tostr                // opcode, "target"
    over 80 = ??
        swap drop
        "{wval _c=w_pop();if(!(int64_t)_c)goto L_" swap ";}\n" strcat strcat
        return
    ]
    over 81 = ??
        swap drop
        "goto L_" swap ";\n" strcat strcat
        return
    ]
    swap drop
    "{wval _c=w_pop();if((int64_t)_c)goto L_" swap ";}\n" strcat strcat
} ;

// ── Emit single instruction → C string ───────────────────────────
: emit-one {
    dup list? ??
        dup 0 @nth
        dup 48 = ?? drop 1 @nth i64tostr "w_push((wval)(int64_t)" swap "L);\n" strcat strcat return ]
        dup 50 = ?? drop 1 @nth escape-c-str "w_push((wval)w_str_from_cstr(\"" swap "\"));\n" strcat strcat return ]
        dup 51 = ?? drop 1 @nth "w_push((wval)" swap "L);\n" strcat strcat return ]
        dup 96 = ?? drop 1 @nth safe-c-name "W_" swap "();\n" strcat strcat return ]
        dup 4  = ?? drop 1 @nth i64tostr "w_pick_n(" swap ");\n" strcat strcat return ]
        drop drop "// unknown list instruction\n"
    ]
    emit-bare
} ;

// Stack: (element, pos). Returns C string. Both consumed.
: emit-instr {
    over is-cf-op ?? emit-cf return ]
    drop emit-one
} ;

// ── Label prefix helper ───────────────────────────────────────────
// Takes (labels, pos) on stack. Returns "L_pos:\n" or "".
// has-label? consumes both labels and pos from the stack.
// ?? asymmetry: true consumes 0, false leaves it.
// Use 'return' in true branch to avoid stack depth mismatch.
: get-label-prefix {
    swap                               // pos, labels
    over over swap                     // pos, labels, labels, pos  (has-label? expects target=pos on top)
    has-label?                         // pos, labels, 0|1
    ??
        // True: ?? consumed 1. Stack: pos, labels.
        // Drop labels, build prefix from pos
        drop                           // pos
        i64tostr "L_" swap ":\n" strcat strcat
        return
    ]
    // False: stack = pos, labels. Drop them, push "".
    drop drop ""
} ;

// ── Emit loop: process bytecodes one at a time ───────────────────
// State = [bytecodes, labels, pos, acc]  (4-element flat list)
//
// Strategy: all $N picks BEFORE any pushes (pushes shift $N depths).
// 1. Emit instruction → push instr_str on top of state
// 2. Get prefix (consumes labels, pos from stack)
// 3. Concatenate: acc ++ prefix ++ instr_str (strat picks + strcat)
// 4. Build new state with append chain
//
: emit-iter {
    // Check termination: pos >= bytecodes_len
    dup 2 @nth over 0 @nth len >= ??
        // Before returning, emit any label at this position
        dup 1 @nth over 2 @nth           // state, labels, pos
        get-label-prefix                   // state, prefix
        over 3 @nth                        // state, prefix, acc
        swap strcat                        // state, acc'
        swap drop                          // acc'
        return
    ]
    // Emit instruction: state → state, instr_str
    dup 0 @nth over 2 @nth @nth       // state, element
    over 2 @nth emit-instr            // state, instr_str
    // Get prefix: pass labels, pos to get-label-prefix
    // Stack: [state, instr_str] (2 items).
    over 1 @nth                        // state, instr_str, labels
    $2 2 @nth                          // state, instr_str, labels, pos  ($2=state with 3 items, 2@nth=state[2])
    get-label-prefix                   // state, instr_str, prefix
    // Concatenate: acc ++ prefix → acc', then acc' ++ instr_str
    // Stack: state, instr_str, prefix. Need state[3]=acc.
    // $2 = Pick(2) = state (3 items on stack). state[3] = acc.
    $2 3 @nth                          // state, instr_str, prefix, acc
    // strcat(second, top) = second ++ top. Want acc ++ prefix.
    swap                                // state, instr_str, acc, prefix
    strcat                             // state, instr_str, acc'
    // Want acc' ++ instr_str.
    swap                                // state, acc', instr_str
    strcat                             // state, acc''
    // Build new state: [state[0], state[1], state[2]+1, acc'']
    swap                                // acc'', state
    [] over 0 @nth append              // acc'', state, [bytecodes]
    over 1 @nth append                 // acc'', state, [bytecodes, labels]
    over 2 @nth 1 + append             // acc'', state, [bytecodes, labels, pos+1]
    rot                                // state, [bc, lbls, pos+1], acc''
    append                             // state, new_state
    swap drop                          // new_state
    emit-iter
} ;

// ── Emit a complete C function body ──────────────────────────────
// Takes bytecodes on stack. Returns C function body string.
: emit-func-body {
    dup collect-labels
    // Stack: bytecodes, labels (labels on top, bytecodes deeper)
    // Build [bytecodes, labels, 0, ""] for emit-iter
    swap                               // labels, bytecodes (bytecodes on top)
    [] swap append swap append          // [bytecodes, labels]
    0 append "" append                  // [bytecodes, labels, 0, ""]
    emit-iter
} ;

// ── Emit a complete C function ───────────────────────────────────
// Stack: name, bytecodes (bytecodes on top). Both consumed.
// Returns C function string.
: emit-func {
    swap                               // bytecodes, name
    safe-c-name                        // bytecodes, safe_name
    "void W_" swap "(void) {\n" strcat strcat
    // Stack: bytecodes, header
    swap                               // header, bytecodes
    emit-func-body                     // header, body
    strcat                             // header_body
    "}\n\n" strcat
} ;

// ── Emit all function definitions ────────────────────────────────
// State = [defs, pos, acc]
: emit-defs-iter {
    dup 0 @nth len over 1 @nth <= ??
        2 @nth return
    ]
    // Get def = [name, bytecodes] at current position
    dup 0 @nth over 1 @nth @nth       // state, def
    dup 0 @nth over 1 @nth            // state, def, name, bytecodes
    emit-func                          // state, def, func_str
    // Drop def, append func_str to acc
    swap drop                          // state, func_str
    over 2 @nth                        // state, func_str, acc
    swap                               // state, acc, func_str
    strcat                             // state, acc'
    // Build new state = [defs, pos+1, acc']
    swap                               // acc', state
    dup 0 @nth                        // acc', state, defs
    swap                               // acc', defs, state
    dup 1 @nth 1 +                    // acc', defs, state, pos+1
    swap                               // acc', defs, pos+1, state
    drop                               // acc', defs, pos+1
    swap                               // acc', pos+1, defs
    mk3                                // [defs, pos+1, acc']
    emit-defs-iter
} ;

: emit-defs {
    0 "" swap rot mk3
    emit-defs-iter
} ;

// ── Main entry: compile-full result → C code ─────────────────────
// Stack: [main_bc, defs]. Returns C code string.
: cgen {
    dup 1 @nth emit-defs              // [main_bc, defs], defs_str
    over 0 @nth                       // [main_bc, defs], defs_str, main_bc
    "main" swap emit-func             // [main_bc, defs], defs_str, main_str
    strcat                             // [main_bc, defs], combined
    swap drop                          // combined
} ;

export cgen
export emit-func
export emit-func-body
export emit-defs
export escape-c-str
export safe-c-name
export mk3
