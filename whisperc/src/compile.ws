// compile.ws — bytecode compiler
// Token types: 0=i64 1=f64 2=str 3=op 4=word 5=quote 6=list
//              7=cond 8=loop 13=bool 15=pick
//
// Every compile-one helper returns a LIST of bytecode instructions.
// Each instruction is either an integer (opcode) or [opcode, operand].
// compile-loop accumulates them via concat-bc.

: tk-type { 0 @nth } ;
: tk-val  { 1 @nth } ;

// ── Compile one token → list of instructions ──

: op-i64   { tk-val [48] swap append 1 PushList } ;   // [[48, value]]
: op-f64   { tk-val [49] swap append 1 PushList } ;   // [[49, bits]]
: op-str   { tk-val [50] swap append 1 PushList } ;   // [[50, string]]
: op-bool  { tk-val [51] swap append 1 PushList } ;   // [[51, value]]
: op-call  { tk-val [96] swap append 1 PushList } ;    // [[96, name]]
: op-pick  { tk-val [4] swap append 1 PushList } ;     // [[4, N]]
: op-op    { tk-val 1 PushList } ;                      // [[opcode]]

// Quote: compile inner body, append [PushRef, inner_bc] instruction
: op-quote {
    tk-val compile              // inner_bc_list (list of instructions)
    53 $1 2 PushList            // [53, inner_bc_list]
    append                      // inner_bc ++ [[53, inner_bc_list]]
} ;

// List literal: compile elements, append PushI64(count) + PushList
: op-list6 {
    tk-val compile              // element_bc_list
    dup len                     // [bc, count]
    [48] swap append            // [bc, [48, count]]
    append                      // bc ++ [[48, count]]
    52 append                   // bc ++ [[48, count]] ++ [52]
} ;

// Conditional: Cond(then_len+1) + then_bc + Jump(else_len) + else_bc
// Token: [7, then_list, else_list]
: op-cond {
    dup 1 @nth compile          // [token, then_bc]
    over 2 @nth compile         // [token, then_bc, else_bc]
    // $0=else, $1=then, $2=token
    $1 len 1 +                  // tl+1
    [80] swap append            // [80, tl+1]
    [] swap append              // [[80, tl+1]]
    // Stack: [token, then, else, [[80,tl+1]]]
    // Concat then_bc items after Cond instruction
    $2 swap concat-bc           // [[80,tl+1]] ++ then_bc items
    // Stack: [token, then, else, result1]
    // Append Jump instruction
    $1 len [81] swap append     // [81, el]
    append                      // result1 ++ [[81, el]]
    // Stack: [token, then, else, result1']
    // Concat else_bc items after Jump
    $1 swap concat-bc           // result1' ++ else_bc items
    // Stack: [token, then, else, final]
    // Cleanup: keep final, drop token/then/else
    swap drop swap drop swap drop
} ;

// Loop: body_bc + cond_bc + [Loop, -(bl+cl+1)]
// Token: [8, body_list, cond_list]
: op-loop {
    dup 1 @nth compile          // [token, body_bc]
    over 2 @nth compile         // [token, body_bc, cond_bc]
    swap                        // [token, cond_bc, body_bc]
    // Flatten body into []
    [] concat-bc                // [token, cond_bc, body_flat]
    // Append cond items
    $1 swap concat-bc           // [token, cond_bc, combined]
    // Append Loop instruction
    dup len 1 + -1 *            // [token, cond_bc, combined, -(total+1)]
    [82] swap append            // [82, -total]
    append                      // combined ++ [[82, -total]]
    // Stack: [token, cond_bc, result]
    swap drop swap drop
} ;

: compile-one {
    dup tk-type
    dup 0 =   ?? drop op-i64   return ]
    dup 1 =   ?? drop op-f64   return ]
    dup 2 =   ?? drop op-str   return ]
    dup 3 =   ?? drop op-op    return ]
    dup 4 =   ?? drop op-call  return ]
    dup 5 =   ?? drop op-quote return ]
    dup 6 =   ?? drop op-list6 return ]
    dup 7 =   ?? drop op-cond  return ]
    dup 8 =   ?? drop op-loop  return ]
    dup 13 =  ?? drop op-bool  return ]
    dup 15 =  ?? drop op-pick  return ]
    drop drop
    [] // unknown token → empty bytecode
} ;

// ── Concat: append each item from src list to dst ──
// [src, dst] → [dst']  — src=$1, dst=$0
: concat-bc {
    over len 0 = ?? swap drop return ]
    over 0 @nth              // [src, dst, item]
    append                    // [src, dst']
    swap tok-advance          // [dst', src']
    swap                      // [src', dst']
    concat-bc
} ;

// ── Compile: token list → flat bytecode list ──
: compile-loop {
    dup len 0 = ?? drop return ]
    dup 0 @nth compile-one    // [acc, tokens, bc_list]
    rot                       // [tokens, bc_list, acc]
    concat-bc                 // [tokens, acc']
    swap                      // [acc', tokens]
    tok-advance               // [acc', tokens']
    compile-loop
} ;

: compile { [] swap compile-loop } ;

export compile
export compile-one
export concat-bc
