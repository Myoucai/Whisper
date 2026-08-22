// ══════════════════════════════════════════════════════════════════════
// Whisper Self-Hosting Compiler — compiler.all.ws
// Pipeline: read-all → compile-full → cgen → output C code
// ══════════════════════════════════════════════════════════════════════

// ── Part 1: Lexer + Parser + Compiler ──────────────────────────────

// ── Whisper Lexer v8 (bootstrap) ────────────────────────────────────
// source string → flat token list (strings)
//   * // comments skipped to end of line
//   * "..." literals read with \" \\ \n \t \r escapes, stored with a
//     leading '"' marker so classify can tell them from words
//   * { } [ ] ; : | are single-char tokens
//   * ?? splits as two ? tokens (both → opcode 80, functionally identical)
//   * # splits unless #t/#f (not handled in this version — not needed
//     for self-hosting since #t/#f only appear in classify.ws/lexer.ws)

: ctos { [] ` append charsstr } ;

: is-blank { _ 32 = $1 9 = | $1 10 = | $1 13 = | ` drop } ;

: is-delim {
    _ 123 = $1 125 = | $1 91 = | $1 93 = |
    $1 58 = | $1 59 = | $1 124 = | ` drop
} ;

: is-ws { _ is-blank $1 is-delim | ` drop } ;

: esc-char {
    _ 110 = ?? drop 10
    | _ 116 = ?? drop 9
    | _ 114 = ?? drop 13
    | _ 48 = ?? drop 0
    | ]
    ]]]
} ;

// read-str-acc: acc s → acc' rest'
: read-str-acc {
    _ "" streq ?? drop "ERR" ""
    |   _ 0 strnth 34 = ??
            striter ` drop
        |   _ 0 strnth 92 = ??
                1 999999 strslice dup 0 strnth esc-char ctos
                @ ` strcat swap 1 999999 strslice read-str-acc
        |   _ 0 strnth ctos
            @ ` strcat
            ` striter ` drop
            read-str-acc
        ]
    ]
    ]
} ;

// read-chunk-acc: acc s → acc' rest'   (accumulate until stop char)
: read-chunk-acc {
    _ "" streq ??
    |   _ 0 strnth is-ws ??
        |   _ 0 strnth ctos
            @ ` strcat
            ` striter ` drop
            read-chunk-acc
        ]
    ]
} ;

// skip-line: s → rest   (skip to next newline or end)
: skip-line {
    _ "" streq ?? return ]
    _ 0 strnth 10 = ?? return ]
    striter ` drop skip-line
} ;

// is-comment?: s → bool  (true if starts with //)
: is-comment? {
    dup strlen 2 >= ??
        dup 0 strnth 47 = swap 1 strnth 47 = &
    | drop 0
    ]
} ;

// tokenize-loop: acc s → acc'
: tokenize-loop {
    _ "" streq ?? drop
    |   _ 0 strnth is-blank ??
            striter ` drop tokenize-loop
        |   _ is-comment? ??
                skip-line tokenize-loop
            |   _ 0 strnth 34 = ??
                    1 999999 strslice "" ` read-str-acc
                    ` "\"" ` strcat `
                    @ @ append ` tokenize-loop
                |   _ 0 strnth is-ws ??
                        striter ` ctos
                        @ ` append ` tokenize-loop
                    |   "" ` read-chunk-acc
                        @ @ append ` tokenize-loop
                    ]
                ]
            ]
        ]
    ]
} ;

: tokenize { [] ` tokenize-loop } ;

export tokenize
export ctos
// Whisper Parser v5.0 — guarded checks, max 2-deep ?? per function
// Pipeline: flat_tokens → structify → structured_tokens
// Tags: 5=Quote({}), 6=List([]), 7=Cond, 8=Loop, 9=Def

: tok-advance { dup len 1 - 1 swap list-slice } ;

: advance-by {
    dup 0 = ?? drop return ]
    { swap tok-advance swap 1 - }
    { dup 0 > }
    #
    drop
} ;

// ══════════════════════════════════════════════════════════════════════
// Pass 1: Group paired delimiters {} and []
// ══════════════════════════════════════════════════════════════════════

: str-eq? { over str? ?? streq | drop drop #f ] } ;
: group-paired-rec { [] swap gp-loop } ;

: gp-loop {
    dup len 0 = ?? drop return ]
    dup 0 @nth
    dup "{" str-eq? ??
        drop tok-advance
        collect-brace
        swap [5] swap append
        rot swap append swap
        gp-loop
    |
    dup "[" str-eq? ??
        drop tok-advance
        collect-bracket
        swap [6] swap append
        rot swap append swap
        gp-loop
    |
        rot swap append swap
        tok-advance
        gp-loop
    ]
} ;

: collect-brace { [] swap cb-loop } ;

: cb-loop {
    dup len 0 = ?? return ]
    dup 0 @nth
    dup "{" str-eq? ??
        drop tok-advance
        collect-brace
        swap [5] swap append
        rot swap append swap
        cb-loop
    |
    dup "}" str-eq? ??
        drop tok-advance return
    |
        rot swap append swap
        tok-advance
        cb-loop
    ]
} ;

: collect-bracket { [] swap cbr-loop } ;

: cbr-loop {
    dup len 0 = ?? return ]
    dup 0 @nth
    dup "[" str-eq? ??
        drop tok-advance
        collect-bracket
        swap [6] swap append
        rot swap append swap
        cbr-loop
    |
    dup "]" str-eq? ??
        drop tok-advance return
    |
        rot swap append swap
        tok-advance
        cbr-loop
    ]
} ;

// ══════════════════════════════════════════════════════════════════════
// split-cond is a native opcode (0xF0) in the seed compiler.
// It takes (tokens start_idx) and returns [then_tokens, else_tokens, next_idx].
// ══════════════════════════════════════════════════════════════════════
// recognize: handle ??, loops, defs, export/import
// Each function has max 2-deep ?? nesting (the seed segfaults on 5+).
// We use guarded helper functions to avoid deep nesting.
// ══════════════════════════════════════════════════════════════════════

: recognize { [] swap rec-loop } ;

// is-tag-5?: check if value is tagged list with tag 5
: is-tag-5? { dup list? ?? 0 @nth 5 = | drop 0 ] } ;

// try-directive: handle export/import
: try-directive {
    dup len 2 < ?? 0 return ]
    dup 0 @nth _ str? ??
        dup "export" streq ?? drop tok-advance tok-advance 1 return ]
        "import" streq ?? tok-advance tok-advance 1 return ]
        0
    | drop 0
    ]
} ;

// try-def: handle ": name {body} ;"
// Uses guarded checks — each condition checked before accessing deeper fields.
: try-def-colon {
    // Stack: [acc, tokens]. tokens[0]=":", tokens[2] is tag-5 list
    // is-tag-5? consumed tokens[2], so stack is [acc, tokens]
    dup 3 @nth ";" streq ??
        // Full match! Stack: [acc, tokens]
        dup 1 @nth                  // acc, tokens, name
        over 2 @nth 1 @nth         // acc, tokens, name, body_inner
        structify                   // acc, tokens, name, body_s
        9 swap rot swap 3 PushList  // acc, tokens, [9, name, body_s]
        rot swap append swap        // acc', tokens
        tok-advance tok-advance tok-advance tok-advance
        1 return
    |
        0
    ]
} ;

: try-def {
    dup len 4 < ?? 0 return ]
    dup 0 @nth _ str? ??
        ":" streq ?? try-def-colon | 0 ]
    | drop 0
    ]
} ;

// try-loop: handle "{body} {cond} #"
: try-loop-inner {
    // Stack: [acc, tokens]. tokens[0] is tag-5, checked by caller
    dup 1 @nth is-tag-5? ??
        dup 2 @nth "#" streq ??
            // Full match! Stack: [acc, tokens]
            dup 0 @nth 1 @nth structify   // acc, tokens, body_s
            over 1 @nth 1 @nth structify  // acc, tokens, body_s, cond_s
            8 swap rot swap 3 PushList     // acc, tokens, [8, body_s, cond_s]
            rot swap append swap           // acc', tokens
            tok-advance tok-advance tok-advance
            1 return
        ]
        0 return
    |
        0
    ]
} ;

: try-loop {
    dup len 3 < ?? 0 return ]
    dup 0 @nth is-tag-5? ?? try-loop-inner | 0 ]
} ;

// rec-loop: main recognize loop
// We have 5 cases. To avoid 5-deep ?? in one function,
// we handle the first 3 cases (which involve function calls)
// then the last 2 inline.
: handle-regular {
    // Stack: [acc, tokens] — regular token case
    dup 0 @nth rot swap append swap
    tok-advance
    rec-loop
} ;

: handle-cond {
    // Stack: [acc, tokens] — tokens[0] is "??"
    tok-advance              // [acc, tokens']
    dup 0 split-cond         // [acc, tokens', result]
    // Build cond structure from result
    dup 0 @nth structify     // [acc, tokens', result, then_s]
    over 1 @nth structify    // [acc, tokens', result, then_s, else_s]
    7 swap rot swap 3 PushList  // [acc, tokens', result, cond]
    // Get next_idx, discard result
    over 2 @nth              // [acc, tokens', result, cond, next_idx]
    swap rot drop            // [acc, tokens', next_idx, cond]
    // Append cond to acc
    $3 swap append           // [acc, tokens', next_idx, acc']
    // Advance tokens' by next_idx
    swap $2 swap advance-by  // [acc, tokens', acc', tokens'']
    // Drop old acc and tokens', keep acc' and tokens''
    swap rot drop swap rot drop  // [acc', tokens'']
    rec-loop
} ;

: rec-loop {
    dup len 0 = ?? drop return ]

    // Try directive (export/import)
    try-directive ?? rec-loop return ]

    // Try def (: name {body} ;)
    try-def ?? rec-loop return ]

    // Try loop ({body} {cond} #)
    try-loop ?? rec-loop return ]

    // Try cond (?? ... ])
    dup 0 @nth _ str? ??
        "??" streq ?? handle-cond return ]
    | drop
    ]

    // Regular token
    handle-regular
} ;

// ══════════════════════════════════════════════════════════════════════
// structify: group braces/brackets, then recognize patterns
// ══════════════════════════════════════════════════════════════════════

: structify { group-paired-rec recognize } ;

export structify
export recognize
export group-paired-rec
// ── Whisper Classifier (bootstrap) ──────────────────────────────────
// word string → [type, value]; token tree → classified tree
//   0=i64 1=f64 2=str 3=op 4=word 13=bool 15=pick(N)

: is-digit { dup 48 >= swap 57 <= & } ;

: check-rest {
    dup "" streq ??
        drop #f
    | 0 strnth is-digit
    ]
} ;

: is-num-str {
    _ "" streq ?? drop #f
    | dup 0 strnth 45 = ??
        1 999999 strslice check-rest
    | 0 strnth is-digit
    ]
    ]
} ;

// Float literal: exactly one '.', both halves numeric ("-0.5", "3.14").
// Classified as [1, the-string]; the C emitter interpolates it as a C
// double literal (no f64->bits conversion exists in the .ws runtime).
: is-float-str {
    dup "." strsplit
    swap drop
    dup len 2 = ??
        dup 0 @nth is-num-str
        over 1 @nth is-num-str
        &
        swap drop
    | drop #f
    ]
} ;

: is-$n? { _ 0 strnth 36 = ?? 1 999999 strslice is-num-str | drop #f ] } ;

// ── Table-driven opcode lookup ──────────────────────────────────────
// Entry format: [opcode, name_str]  — linear scan, O(N) but N≤50 is fine.

: op-table { [
    [16 "+"] [17 "-"] [18 "*"] [19 "/"] [20 "%"]
    [24 "="] [25 "<"] [26 ">"] [27 "!="] [28 "<="] [29 ">="]
    [32 "&"] [33 "|"] [34 "!"]
    [0 "_"] [3 "@"] [1 "`"]
    [144 "."] [145 ".."] [146 ","]
    [160 ":"] [161 ";"]
    [80 "??"] [83 "#"] [4 "$"]
    [0 "dup"] [2 "drop"] [1 "swap"] [3 "rot"]
    [97 "return"] [66 "len"] [65 "append"] [222 "prepend"]
    [64 "@nth"] [67 "@map"] [68 "@each"] [69 "@fold"]
    [5 "@filter"] [83 "@times"]
    [70 "strlen"] [71 "strcat"] [72 "strslice"] [73 "streq"]
    [74 "strlt"] [75 "strfind"] [76 "strreplace"]
    [77 "strtoi64"] [78 "i64tostr"] [79 "strnth"]
    [6 "strsplit"] [7 "ctos"] [8 "list-slice"]
    [184 "strchars"] [185 "charsstr"] [186 "striter"]
    [187 "listfind"] [188 "strjoin"]
    [176 "i64tof64"] [177 "f64toi64"]
    [178 "fsqrt"] [179 "fsin"] [180 "fcos"] [181 "ftan"]
    [212 "floor"] [213 "ceil"] [214 "round"]
    [215 "log"] [216 "exp"] [217 "atan2"]
    [218 "bitand"] [219 "bitor"] [220 "bitxor"] [221 "bitnot"]
    [9 "shl"] [10 "shr"] [9 "<<"] [10 ">>"]
    [223 "sort"] [224 "rev"] [225 "dedup"]
    [226 "take"] [227 "drop-n"]
    [236 "listen"] [237 "accept"] [238 "recv"] [239 "send"]
    [241 "close"] [242 "read-file"] [243 "write-file"] [244 "getenv"]
    [245 "kv-set"] [246 "kv-get"] [247 "kv-del"] [248 "kv-keys"]
    [255 "serve-concurrent"]
    [194 "map-new"] [195 "map-get"] [196 "map-set"]
    [197 "map-del"] [198 "map-keys"] [199 "map-has"]
    [208 "now"] [209 "sleep"] [210 "random"] [211 "randint"]
    [11 "args"]
    [228 "list?"] [229 "i64?"] [230 "str?"] [231 "bool?"]
    [232 "read-all"]
    [240 "split-cond"]
    [52 "PushList"]
    [193 "try"]
    [234 "buf-new"] [235 "buf-push"]
] } ;

: table-lookup-helper {
    // Stack: word table idx → opcode or -1
    $1 $1 @nth              // word table idx entry
    dup 1 @nth              // word table idx entry name
    $4 streq                // word table idx entry match
    ??
        // Found: extract opcode and return
        0 @nth              // word table idx opcode
        swap drop swap drop swap drop
        return
    |
        // Not found: advance index
        drop                // word table idx
        1 +                 // word table (idx+1)
        dup $2 len <        // word table (idx+1) in_bounds?
        ??
            table-lookup-helper
        |
            drop drop drop -1
        ]
    ]
} ;
: table-lookup {
    // table word → opcode or -1
    swap 0 table-lookup-helper
} ;

: classify-one {
    _ "#t"  streq ?? drop [13] 1 append
    | _ "#f"  streq ?? drop [13] 0 append
    | _ "over" streq ?? drop [15] 1 append
    | op-table $1 table-lookup
        dup -1 = ??
            // Not in table — try $N / number / word
            drop
            _ is-$n?  ?? [15] ` 1 999999 strslice strtoi64 append
            | _ is-float-str ?? [1] ` append
| _ is-num-str ?? [0] ` strtoi64 append
            | [4] ` append
            ]]]
        | // Found in table — emit [3, opcode]
            swap drop [3] swap append
        ]
    ]]
} ;

// ── classify: map classify-one over tokens (no @map, for self-hosting) ──
: classify-loop {
    dup len 0 = ?? drop return ]
    dup 0 @nth classify-one   // [acc, tokens, classified]
    rot swap append swap       // [acc', tokens]
    tok-advance                // [acc', tokens']
    classify-loop
} ;
: classify { [] swap classify-loop } ;

// ── Recursive classification for structured token trees ─────────────

: cl-quote {
    // Inner tokens are raw (recognize skips structured tokens): run the
    // full structify (brace grouping + cond/loop recognition) so ??|]
    // inside quotes become Cond nodes instead of stray word calls.
    dup 1 @nth structify classify-nested
    swap drop [5] swap append
} ;
: cl-list {
    dup 1 @nth classify-nested
    swap drop [6] swap append
} ;
: cl-cond {
    dup 1 @nth classify-nested
    over 2 @nth classify-nested
    rot drop [7] rot append swap append
} ;
: cl-loop {
    dup 1 @nth classify-nested
    over 2 @nth classify-nested
    rot drop [8] rot append swap append
} ;
: cl-def {
    dup 2 @nth classify-nested
    over 1 @nth
    rot drop [9] swap append swap append
} ;

// cn-map-loop: map classify-nested over list elements (no @map)
: cn-map-loop {
    dup len 0 = ?? drop return ]
    dup 0 @nth classify-nested  // [acc, tokens, classified]
    rot swap append swap         // [acc', tokens]
    tok-advance                  // [acc', tokens']
    cn-map-loop
} ;

: classify-nested {
    _ list? ??
        dup len 0 > ??
            dup 0 @nth
            _ 5 = ?? drop cl-quote
            | _ 6 = ?? drop cl-list
            | _ 7 = ?? drop cl-cond
            | _ 8 = ?? drop cl-loop
            | _ 9 = ?? drop cl-def
            | drop [] swap cn-map-loop
            ]]]]]
        |
        ]
    |
        _ str? ??
            _ 0 strnth 34 = ??
                1 999999 strslice [2] swap append
            | classify-one
            ]
        |
            ]
    ]
} ;

export classify
export classify-one
export classify-nested
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
: op-hof   { dup tk-type swap tk-val compile 2 PushList 1 PushList } ; // [[op', inner_bc]]
: op-op    { tk-val 1 PushList } ;                      // [[opcode]]

// Quote: compile inner body, emit single [PushRef, inner_bc] instruction
: op-quote {
    tk-val compile              // inner_bc_list
    53 swap 2 PushList          // [53, inner_bc_list]
    1 PushList                  // [[53, inner_bc_list]]
} ;

// List literal: compile elements, append PushI64(count) + PushList
// Must count elements BEFORE compiling — compiled bytecode count differs
// for nested lists (each inner list produces 4 bytecode ops but 1 stack item).
: op-list6 {
    tk-val                      // inner_list
    dup len                     // inner_list, elem_count
    swap compile                // elem_count, element_bc_list
    swap                        // element_bc_list, elem_count
    [48] swap append            // element_bc_list, [48, elem_count]
    append                      // bc ++ [[48, elem_count]]
    52 append                   // bc ++ [[48, elem_count]] ++ [52]
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
    dup 249 >= over 254 <= & ?? drop op-hof return ]
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

// ── Higher-order fusion: {quote} @map/@each/@fold/@times/@filter/try ──
// A [5, inner] quote token followed by a consumer op fuses into ONE
// instruction [op', inner-tokens] (op' 249-254); the C emitter wraps
// the compiled body in a gcc nested function so quote-local labels
// cannot collide with the enclosing function's labels.
: is-quote-tok? {
    dup list? ??
        _ 0 @nth 5 = swap drop return ]
    drop #f
} ;
: tok-head-quote? { dup 0 @nth is-quote-tok? } ;
: consumer-op {
    dup list? ??
        _ 0 @nth 3 = ??
            1 @nth
            dup 67 = ?? drop 249 return ]
            dup 68 = ?? drop 250 return ]
            dup 69 = ?? drop 251 return ]
            dup 83 = ?? drop 252 return ]
            dup 5  = ?? drop 253 return ]
            dup 193 = ?? drop 254 return ]
            drop 0
        | drop 0
        ]
    | drop 0
    ]
} ;
: next-consumer-op { dup 1 @nth consumer-op } ;
: fuse-hof {
    dup len 1 < ?? return ]
    tok-head-quote? ??
        next-consumer-op dup 0 > ??
            over 0 @nth 1 @nth
            2 PushList
            swap tok-advance tok-advance
            fuse-hof prepend
            return
        | drop
        ]
    | ]
    dup 0 @nth swap tok-advance fuse-hof prepend
} ;
: compile { fuse-hof [] swap compile-loop } ;

export compile
export compile-one
export concat-bc

// ── Stack helpers ───────────────────────────────────────────────────
: mk3 { [] swap append swap append swap append } ;

// ── Split definitions from main body ────────────────────────────────
// Takes classified token list. Returns [main_bc, [[name, bc]...]].
// State = [tokens, main_acc, defs_acc]

: split-defs-iter {
    dup 0 @nth len 0 = ??
        // Done: tokens empty. Return [main_acc, defs_acc]
        dup 1 @nth swap 2 @nth  // main_acc, defs_acc
        2 PushList               // [main_acc, defs_acc]
        return
    ]
    // Check if first token is a def (type 9)
    // After tk-type, stack: state, type. After 9 = ??: state (consumed type+bool)
    dup 0 @nth 0 @nth tk-type 9 = ??
        // DEF: [9, name, body] — compile body, append [name, bc] to defs_acc
        // Stack: state (type and bool already consumed by = and ??)
        dup 0 @nth 0 @nth      // state, def_token
        dup 2 @nth compile     // state, def_token, compiled_body
        over 1 @nth            // state, def_token, compiled_body, name
        swap                   // state, def_token, name, compiled_body
        2 PushList             // state, def_token, [name, compiled_body]
        swap drop              // state, [name, compiled_body]
        over 2 @nth            // state, [name, bc], defs_acc
        swap append            // state, defs_acc'
        // Rebuild state: [tokens', main_acc, defs_acc']
        swap                   // defs_acc', state
        dup 0 @nth tok-advance // defs_acc', state, tokens'
        swap                   // defs_acc', tokens', state
        dup 1 @nth             // defs_acc', tokens', state, main_acc
        swap                   // defs_acc', tokens', main_acc, state
        drop                   // defs_acc', tokens', main_acc
        swap                   // defs_acc', main_acc, tokens'
        mk3                    // [tokens', main_acc, defs_acc']
        split-defs-iter
        return
    ]
    // REGULAR: compile-one, concat into main_acc
    // Stack: state (= and ?? consumed type+bool, no extra items)
    dup 0 @nth 0 @nth compile-one  // state, bc_list
    over 1 @nth concat-bc          // state, main_acc'
    // Rebuild state: [tokens', main_acc', defs_acc]
    swap                             // main_acc', state
    dup 0 @nth tok-advance          // main_acc', state, tokens'
    swap                             // main_acc', tokens', state
    2 @nth                          // main_acc', tokens', defs_acc
    rot rot                          // defs_acc, main_acc', tokens'
    mk3                              // [tokens', main_acc', defs_acc]
    split-defs-iter
    return
} ;

: split-defs {
    [] [] rot mk3                   // [tokens, [], []]
    split-defs-iter
} ;

export split-defs

// ── Full compilation pipeline ───────────────────────────────────────
// source_string → [main_bytecodes, [[name, def_bytecodes], ...]]

: classify-full { tokenize structify classify-nested } ;

: compile-full {
    classify-full               // tokenize → structify → classify
    fuse-hof                    // fuse {quote}+consumer pairs (main AND def bodies)
    split-defs                  // → [main_bc, [[name, bc]...]]
} ;

export compile-full
export classify-full


// ── Part 2: C Code Generator ─────────────────────────────────────

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
    dup 6  = ?? drop "{const char*_d=(const char*)w_pop();const char*_s=(const char*)w_pop();w_push((wval)w_str_split(_s,_d));}\n" return ]
    dup 9  = ?? drop "{int64_t _b=(int64_t)w_pop();int64_t _a=(int64_t)w_pop();w_push((wval)(_a<<_b));}\n" return ]
    dup 10 = ?? drop "{int64_t _b=(int64_t)w_pop();int64_t _a=(int64_t)w_pop();w_push((wval)(_a>>_b));}\n" return ]
    dup 11 = ?? drop "w_push((wval)w_args());\n" return ]
    dup 19 = ?? drop "{wval _b=w_pop(),_a=w_pop();if(!(int64_t)_b)exit(1);w_push((wval)((int64_t)_a/(int64_t)_b));}\n" return ]
    dup 20 = ?? drop "{wval _b=w_pop(),_a=w_pop();if(!(int64_t)_b)exit(1);w_push((wval)((int64_t)_a%(int64_t)_b));}\n" return ]
    dup 27 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a!=(int64_t)_b?1L:0L));}\n" return ]
    dup 74 = ?? drop "{const char*_b=(const char*)w_pop();const char*_a=(const char*)w_pop();w_push((wval)w_str_lt(_a,_b));}\n" return ]
    dup 75 = ?? drop "{const char*_n=(const char*)w_pop();const char*_h=(const char*)w_pop();w_push((wval)w_str_find(_h,_n));}\n" return ]
    dup 76 = ?? drop "{const char*_nw=(const char*)w_pop();const char*_ow=(const char*)w_pop();const char*_s=(const char*)w_pop();w_push((wval)w_str_replace(_s,_ow,_nw));}\n" return ]
    dup 97 = ?? drop "return;\n" return ]
    dup 144 = ?? drop "w_print_val(w_pop());\n" return ]
    dup 145 = ?? drop "w_output_all();\n" return ]
    dup 176 = ?? drop "{int64_t _n=(int64_t)w_pop();double _f=(double)_n;w_push((wval)*(uint64_t*)&_f);}\n" return ]
    dup 177 = ?? drop "{uint64_t _bits=(uint64_t)w_pop();double _f=*(double*)&_bits;w_push((wval)(int64_t)_f);}\n" return ]
    dup 178 = ?? drop "{uint64_t _bits=(uint64_t)w_pop();double _f=*(double*)&_bits;_f=w_fsqrt(_f);w_push((wval)*(uint64_t*)&_f);}\n" return ]
    dup 179 = ?? drop "{uint64_t _bits=(uint64_t)w_pop();double _f=*(double*)&_bits;_f=w_fsin(_f);w_push((wval)*(uint64_t*)&_f);}\n" return ]
    dup 180 = ?? drop "{uint64_t _bits=(uint64_t)w_pop();double _f=*(double*)&_bits;_f=w_fcos(_f);w_push((wval)*(uint64_t*)&_f);}\n" return ]
    dup 181 = ?? drop "{uint64_t _bits=(uint64_t)w_pop();double _f=*(double*)&_bits;_f=w_ftan(_f);w_push((wval)*(uint64_t*)&_f);}\n" return ]
    dup 184 = ?? drop "{const char*_s=(const char*)w_pop();w_push((wval)w_strchars(_s));}\n" return ]
    dup 185 = ?? drop "{wval*_l=(wval*)w_pop();w_push((wval)w_charsstr(_l));}\n" return ]
    dup 186 = ?? drop "{const char*_s=(const char*)w_pop();w_striter(_s);}\n" return ]
    dup 187 = ?? drop "{wval _e=w_pop();wval*_l=(wval*)w_pop();w_push((wval)(int64_t)w_listfind(_l,_e));}\n" return ]
    dup 188 = ?? drop "{wval*_l=(wval*)w_pop();w_push((wval)w_strjoin(_l));}\n" return ]
    dup 194 = ?? drop "w_push((wval)w_map_new());\n" return ]
    dup 195 = ?? drop "{const char*_k=(const char*)w_pop();wval*_m=(wval*)w_pop();w_push(w_map_get(_m,_k));}\n" return ]
    dup 196 = ?? drop "{wval _v=w_pop();const char*_k=(const char*)w_pop();wval*_m=(wval*)w_pop();w_push((wval)w_map_set(_m,_k,_v));}\n" return ]
    dup 197 = ?? drop "{const char*_k=(const char*)w_pop();wval*_m=(wval*)w_pop();w_push((wval)w_map_del(_m,_k));}\n" return ]
    dup 198 = ?? drop "{wval*_m=(wval*)w_pop();w_push((wval)w_map_keys(_m));}\n" return ]
    dup 199 = ?? drop "{const char*_k=(const char*)w_pop();wval*_m=(wval*)w_pop();w_push((wval)(int64_t)w_map_has(_m,_k));}\n" return ]
    dup 208 = ?? drop "w_push((wval)w_now());\n" return ]
    dup 209 = ?? drop "{int64_t _ms=(int64_t)w_pop();w_sleep(_ms);}\n" return ]
    dup 210 = ?? drop "{double _f=w_random();w_push((wval)*(uint64_t*)&_f);}\n" return ]
    dup 211 = ?? drop "{int64_t _mx=(int64_t)w_pop();int64_t _mn=(int64_t)w_pop();w_push((wval)w_randint(_mn,_mx));}\n" return ]
    dup 212 = ?? drop "{uint64_t _bits=(uint64_t)w_pop();double _f=*(double*)&_bits;_f=floor(_f);w_push((wval)*(uint64_t*)&_f);}\n" return ]
    dup 213 = ?? drop "{uint64_t _bits=(uint64_t)w_pop();double _f=*(double*)&_bits;_f=ceil(_f);w_push((wval)*(uint64_t*)&_f);}\n" return ]
    dup 214 = ?? drop "{uint64_t _bits=(uint64_t)w_pop();double _f=*(double*)&_bits;_f=round(_f);w_push((wval)*(uint64_t*)&_f);}\n" return ]
    dup 215 = ?? drop "{uint64_t _bits=(uint64_t)w_pop();double _f=*(double*)&_bits;_f=w_flog(_f);w_push((wval)*(uint64_t*)&_f);}\n" return ]
    dup 216 = ?? drop "{uint64_t _bits=(uint64_t)w_pop();double _f=*(double*)&_bits;_f=w_fexp(_f);w_push((wval)*(uint64_t*)&_f);}\n" return ]
    dup 217 = ?? drop "{uint64_t _bx=(uint64_t)w_pop();uint64_t _by=(uint64_t)w_pop();double _x=*(double*)&_bx;double _y=*(double*)&_by;double _r=w_fatan2(_y,_x);w_push((wval)*(uint64_t*)&_r);}\n" return ]
    dup 218 = ?? drop "{int64_t _b=(int64_t)w_pop();int64_t _a=(int64_t)w_pop();w_push((wval)(_a&_b));}\n" return ]
    dup 219 = ?? drop "{int64_t _b=(int64_t)w_pop();int64_t _a=(int64_t)w_pop();w_push((wval)(_a|_b));}\n" return ]
    dup 220 = ?? drop "{int64_t _b=(int64_t)w_pop();int64_t _a=(int64_t)w_pop();w_push((wval)(_a^_b));}\n" return ]
    dup 221 = ?? drop "{int64_t _a=(int64_t)w_pop();w_push((wval)(~_a));}\n" return ]
    dup 222 = ?? drop "{wval*_l=(wval*)w_pop();wval _e=w_pop();w_push((wval)w_list_prepend(_e,_l));}\n" return ]
    dup 223 = ?? drop "{wval*_l=(wval*)w_pop();w_push((wval)w_list_sort(_l));}\n" return ]
    dup 224 = ?? drop "{wval*_l=(wval*)w_pop();w_push((wval)w_list_rev(_l));}\n" return ]
    dup 225 = ?? drop "{wval*_l=(wval*)w_pop();w_push((wval)w_list_dedup(_l));}\n" return ]
    dup 226 = ?? drop "{int64_t _n=(int64_t)w_pop();wval*_l=(wval*)w_pop();w_push((wval)w_list_take(_l,_n));}\n" return ]
    dup 227 = ?? drop "{int64_t _n=(int64_t)w_pop();wval*_l=(wval*)w_pop();w_push((wval)w_list_drop(_l,_n));}\n" return ]
    dup 228 = ?? drop "w_list_();\n" return ]
    dup 229 = ?? drop "w_i64_();\n" return ]
    dup 230 = ?? drop "w_str_();\n" return ]
    dup 231 = ?? drop "w_bool_();\n" return ]
    dup 232 = ?? drop "w_push((wval)w_read_all());\n" return ]
    dup 234 = ?? drop "{int64_t _n=(int64_t)w_pop();w_push((wval)w_buf_new(_n));}\n" return ]
    dup 235 = ?? drop "{wval _e=w_pop();wval*_b=(wval*)w_pop();w_push((wval)w_buf_push(_b,_e));}\n" return ]
    dup 240 = ?? drop "{int64_t _start=(int64_t)w_pop();wval*_toks=(wval*)w_pop();wval*_th=0,*_el=0;int _next=0;split_cond(_toks,(int)_start,&_th,&_el,&_next);wval*_r=w_list_new(3);_r[1]=(wval)_th;_r[2]=(wval)_el;_r[3]=(wval)(int64_t)_next;w_push((wval)_r);}\n" return ]
    dup 236 = ?? drop "{ int64_t _p=(int64_t)w_pop(); w_push(w_sock_listen(_p)); }
" return ]
    dup 237 = ?? drop "{ wval _h=w_pop(); w_push(w_sock_accept(_h)); }
" return ]
    dup 238 = ?? drop "{ wval _c=w_pop(); w_push(w_sock_recv(_c)); }
" return ]
    dup 239 = ?? drop "{ const char *_s=(const char*)w_pop(); wval _c=w_pop(); w_push(w_sock_send(_c,_s)); }
" return ]
    dup 241 = ?? drop "{ wval _c=w_pop(); w_sock_close(_c); }
" return ]
    dup 242 = ?? drop "{ const char *_p=(const char*)w_pop(); w_push(w_read_file(_p)); }
" return ]
    dup 243 = ?? drop "{ const char *_d=(const char*)w_pop(); const char *_p=(const char*)w_pop(); w_push(w_write_file(_p,_d)); }
" return ]
    dup 244 = ?? drop "{ const char *_n=(const char*)w_pop(); w_push(w_env_get(_n)); }
" return ]
    dup 245 = ?? drop "{ const char *_v=(const char*)w_pop(); const char *_k=(const char*)w_pop(); w_kv_set(_k,_v); }
" return ]
    dup 246 = ?? drop "{ const char *_k=(const char*)w_pop(); w_push(w_kv_get(_k)); }
" return ]
    dup 247 = ?? drop "{ const char *_k=(const char*)w_pop(); w_kv_del(_k); }
" return ]
    dup 248 = ?? drop "w_push((wval)w_kv_keys_list());
" return ]
    dup 255 = ?? drop "{ const char *_n=(const char*)w_pop(); int64_t _p=(int64_t)w_pop(); w_serve_concurrent(_p, w_find_word(_n)); }
" return ]
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
        dup 49 = ?? drop 1 @nth "{double _d=" swap "; w_push(*(wval*)&_d);}\n" strcat strcat return ]
        dup 249 = ?? drop 1 @nth emit-func-body "{ auto void _qb(void) { " swap strcat " } wval *_l=(wval*)w_pop(); int64_t _n=w_list_len(_l); wval *_r=w_list_new(_n); for (int64_t _i=0;_i<_n;_i++){ w_push(_l[1+_i]); _qb(); _r[1+_i]=w_pop(); } w_push((wval)_r); }\n" strcat return ]
        dup 250 = ?? drop 1 @nth emit-func-body "{ auto void _qb(void) { " swap strcat " } wval *_l=(wval*)w_pop(); int64_t _n=w_list_len(_l); for (int64_t _i=0;_i<_n;_i++){ int _s0=w_sp; w_push(_l[1+_i]); _qb(); if (w_sp>_s0) w_sp=_s0; } }\n" strcat return ]
        dup 251 = ?? drop 1 @nth emit-func-body "{ auto void _qb(void) { " swap strcat " } wval _a=w_pop(); wval *_l=(wval*)w_pop(); int64_t _n=w_list_len(_l); for (int64_t _i=0;_i<_n;_i++){ w_push(_a); w_push(_l[1+_i]); _qb(); _a=w_pop(); } w_push(_a); }\n" strcat return ]
        dup 252 = ?? drop 1 @nth emit-func-body "{ auto void _qb(void) { " swap strcat " } int64_t _n=(int64_t)w_pop(); for (int64_t _i=0;_i<_n;_i++){ _qb(); } }\n" strcat return ]
        dup 253 = ?? drop 1 @nth emit-func-body "{ auto void _qb(void) { " swap strcat " } wval *_l=(wval*)w_pop(); int64_t _n=w_list_len(_l); wval *_r=w_list_new(0); for (int64_t _i=0;_i<_n;_i++){ w_push(_l[1+_i]); _qb(); if ((int64_t)w_pop()!=0) _r=w_list_append(_r,_l[1+_i]); } w_push((wval)_r); }\n" strcat return ]
        dup 254 = ?? drop 1 @nth emit-func-body "{ auto void _qb(void) { " swap strcat " } _qb(); wval _r=w_pop(); w_push((wval)1); w_push(_r); }\n" strcat return ]
        dup 50 = ?? drop 1 @nth escape-c-str "w_push((wval)w_str_from_cstr(\"" swap "\"));\n" strcat strcat return ]
        dup 51 = ?? drop 1 @nth i64tostr "w_push((wval)" swap "L);\n" strcat strcat return ]
        dup 96 = ?? drop 1 @nth safe-c-name "W_" swap "();\n" strcat strcat return ]
        dup 4  = ?? drop 1 @nth i64tostr "w_pick_n(" swap ");\n" strcat strcat return ]
        swap drop emit-bare return
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
    "static void W_" swap "(void) {\n" strcat strcat
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
    dup 0 @nth over 1 @nth @nth       // state, def
    dup 0 @nth over 1 @nth            // state, def, name, bytecodes
    emit-func                          // state, def, func_str
    swap drop                          // state, func_str
    over 2 @nth                        // state, func_str, acc
    swap                               // state, acc, func_str
    strcat                             // state, acc'
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

// ── Emit forward declarations ───────────────────────────────────
// State = [defs, pos, acc]
: emit-fwd-iter {
    dup 0 @nth len over 1 @nth <= ??
        2 @nth return
    ]
    dup 0 @nth over 1 @nth @nth       // state, def
    dup 0 @nth                        // state, def, name
    swap drop                          // state, name
    safe-c-name                        // state, sn
    "static void W_" swap "(void);\n" strcat strcat
                                        // state, fwd_str
    over 2 @nth                        // state, fwd_str, acc
    swap                               // state, acc, fwd_str
    strcat                             // state, acc'
    swap                               // acc', state
    dup 0 @nth                        // acc', state, defs
    swap                               // acc', defs, state
    dup 1 @nth 1 +                    // acc', defs, state, pos+1
    swap                               // acc', defs, pos+1, state
    drop                               // acc', defs, pos+1
    swap                               // acc', pos+1, defs
    mk3                                // [defs, pos+1, acc']
    emit-fwd-iter
} ;

: emit-fwd-decls {
    0 "" swap rot mk3
    emit-fwd-iter
} ;

// ── Main entry: compile-full result → C code ─────────────────────
: emit-registry-iter {
    // state=[defs,pos,acc] -> acc'  (mirrors emit-fwd-iter)
    dup 0 @nth len over 1 @nth <= ?? 2 @nth return ]
    dup 0 @nth over 1 @nth @nth
    dup 0 @nth
    swap drop
    safe-c-name
    _ "if(!strcmp(n,\"" ` strcat
    "\")) return W_" strcat
    ` strcat
    ";\n" strcat
    over 2 @nth
    swap
    strcat
    swap
    dup 0 @nth
    swap
    dup 1 @nth 1 +
    swap
    drop
    swap
    mk3
    emit-registry-iter
} ;
: emit-registry { 0 "" swap rot mk3 emit-registry-iter } ;



: cgen {
    // Stack: [main_bc, defs]
    dup 1 @nth                        // list, defs
    dup emit-fwd-decls                // list, defs, fwd_str
    "static wfn w_find_word(const char *n){
" strcat
    over emit-registry strcat "return 0;
}
" strcat
    swap                              // list, fwd_str, defs
    emit-defs                         // list, fwd_str, body_str
    strcat                            // list, fwd_body
    over 0 @nth                       // list, fwd_body, main_bc
    dup len 0 = ??
        drop                          // list, fwd_body
        "int main(int argc, char **argv) {\n    w_init(argc, argv);\n    return 0;\n}\n\n"
        strcat                        // list, combined
    |   emit-func-body                // list, fwd_body, main_body
        "int main(int argc, char **argv) {\n    w_init(argc, argv);\n"
        swap strcat
        "    return 0;\n}\n\n" strcat
        strcat                        // list, combined
    ]
    swap drop                         // combined
} ;

export cgen
export emit-func
export emit-func-body
export emit-defs
export escape-c-str
export safe-c-name
export mk3

// ── Part 3: Driver ───────────────────────────────────────────────

: header { "// Generated by the Whisper compiler
#include \"runtime.c\"

" } ;
: driver {
    read-all compile-full cgen header swap strcat .
} ;
driver
