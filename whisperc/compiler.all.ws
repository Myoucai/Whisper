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
// escape code -> replacement string; UNKNOWN escapes keep the
// backslash (parity with the Rust lexer's pass-through rule, so
// regex patterns like "\d+" survive the self-hosted chain)
: esc-str {
    dup 110 = ?? drop 10 ctos return ]
    dup 116 = ?? drop 9 ctos return ]
    dup 114 = ?? drop 13 ctos return ]
    dup 48 = ?? drop 0 ctos return ]
    dup 34 = ?? drop 34 ctos return ]
    dup 92 = ?? drop 92 ctos return ]
    92 ctos swap ctos strcat
} ;

// read-str-acc: acc s → acc' rest'
: read-str-acc {
    _ "" streq ?? drop "ERR" ""
    |   _ 0 strnth 34 = ??
            striter ` drop
        |   _ 0 strnth 92 = ??
                1 999999 strslice dup 0 strnth esc-str
                @ ` strcat swap 1 999999 strslice read-str-acc
        |   _ 0 strnth ctos
            @ ` strcat
            ` striter ` drop
            read-str-acc
        ]
    ]
    ]
} ;

// J-4 限定量：s 以点开头且下一字符为词字符 → 点并入词（mmod.mw）。
// 只用覆盖门禁已绿的 op（strnth/kv/??/=&/<=/>=/&/ctos）——K-4 纪律。
: is-wordch? {
    ctos "WC-S" ` kv-set
    "WC-S" kv-get 0 strnth drop
    "WC-S" kv-get 0 strnth 48 >= "WC-S" kv-get 0 strnth 57 <= &
    "WC-S" kv-get 0 strnth 65 >= "WC-S" kv-get 0 strnth 90 <= & |
    "WC-S" kv-get 0 strnth 97 >= "WC-S" kv-get 0 strnth 122 <= & |
    "WC-S" kv-get 0 strnth 95 = "WC-S" kv-get 0 strnth 63 = | |
    "WC-S" kv-get 0 strnth 45 = "WC-S" kv-get 0 strnth 47 = | |
} ;

: dot-joins? {
    "DJ-S" ` kv-set
    "DJ-S" kv-get 0 strnth 46 =
    "DJ-S" kv-get 1 strnth is-wordch? &
} ;

// s → 1/0：chunk 继续消费当前字符（非点，或"点+词字符"连接）
: chunk-cont? {
    "DJ-C" ` kv-set
    "DJ-C" kv-get 0 strnth 46 =
    ?? "DJ-C" kv-get dot-joins?
    | 1
    ]
} ;

// L-03 置信度字面量：累积到 "#t"/"#f" 时 ':' 并入（#t:0.75 一段）
// (acc s → 1/0) 消耗型；kv 先存 s 后存 acc（` = swap：值在键下）
: conf-colon? {
    "CC-S" ` kv-set
    "CC-A" ` kv-set
    "CC-A" kv-get "#t" streq "CC-A" kv-get "#f" streq |
    "CC-S" kv-get 0 strnth 58 = &
} ;

// read-chunk-acc: acc s → acc' rest'   (accumulate until stop char)
: read-chunk-acc {
    _ "" streq ??
    |   _ 0 strnth is-ws $2 $2 conf-colon? ! &
        ??  return
        |   _ chunk-cont?
            ?? _ 0 strnth ctos
               @ ` strcat
               ` striter ` drop
               read-chunk-acc
            |   return ]
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

// 词 token vs 独立点 token 分派（J-4；拆出独立词把 tokenize-loop 的
// ?? 嵌套深度拉回 seed "max 2-deep" 限制内——深嵌套会产出错字节码）
: chunk-or-delim {
    _ chunk-cont?
    ?? "" ` read-chunk-acc
       @ @ append ` tokenize-loop
    |   striter ` ctos
        @ ` append ` tokenize-loop
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
                    |   chunk-or-delim
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
    // str-eq? (not bare streq): tokens[3] is whatever follows the body, which
    // for `{ a } { b } { c }` is another tag-5 LIST, not a string. The VM
    // raises "expected str, got [T]" on that; the C chain's w_str_eq happily
    // compares a list pointer and returns false — so this only ever blew up
    // on the Rust VM, and only from the third quotation onwards (with two the
    // length guard below returns first).
    dup 3 @nth ";" str-eq? ??
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
        // str-eq? for the same reason as try-def-colon: tokens[2] can be a
        // LIST (a third quotation), and bare streq on a list is a VM type
        // error while the C chain silently returns false.
        dup 2 @nth "#" str-eq? ??
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
    [147 "print"] [148 "exit"]
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
    [249 "read-line"] [250 "str-upper"] [251 "str-lower"] [252 "str-strip"] [253 "fmt"]
    [200 "listdir"] [201 "mkdir"] [202 "file-exists"] [203 "strftime"] [254 "leave"]
    [233 "strmatch"] [204 "strfind-re"]
    [12 "pack"] [13 "unpack"]
    [205 "range"]
    [150 "ffi-open"] [151 "ffi-sym"] [152 "ffi-call"]
    [86 "?->"]
    [153 "buf-alloc"] [154 "buf-free"] [155 "buf-peek"] [156 "buf-poke"] [157 "buf->str"]
    [194 "map-new"] [195 "map-get"] [196 "map-set"]
    [197 "map-del"] [198 "map-keys"] [199 "map-has"]
    [208 "now"] [209 "sleep"] [210 "random"] [211 "randint"]
    [11 "args"]
    [228 "list?"] [229 "i64?"] [230 "str?"] [231 "bool?"]
    [232 "read-all"]
    [240 "split-cond"]
    [52 "PushList"]
    [193 "try"]
    [47 "conf-of"] [54 "unwrap"]
    [182 "json-parse"] [183 "json-stringify"]
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

// L-03：#t:0.75 / #f:0.25 置信度字面量（Rust lexer 单 token 的对齐）
: is-conf-tok? {
    dup "#t:" strfind 0 =
    swap
    dup "#f:" strfind 0 =
    swap drop
    |
} ;
: conf-tok {
    "CT-S" ` kv-set
    "CT-S" kv-get "#t:" strfind 0 = ?? [14] | [12] ]
    "CT-S" kv-get 3 999999 strslice append
} ;

: classify-one {
    _ is-conf-tok? ?? conf-tok return
    | _ "#t"  streq ?? drop [13] 1 append
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
    ]
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
    // Mirror cl-quote: the collector leaves a bracket's inner tokens RAW
    // (cbr-loop only groups nested brackets, not braces), so an element like
    // `{ 1 + }` inside `[ ... ]` is still the four strings "{" "1" "+" "}".
    // Without this structify pass classify-nested sees a bare "{" and
    // classifies it as a WORD token — a list of quotations was therefore
    // unusable, which is exactly what dynamic dispatch needs.
    dup 1 @nth structify classify-nested
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
: op-conf1 { tk-val [55] swap append 1 PushList } ;   // [[55, "conf"]]  #t:c
: op-conf0 { tk-val [56] swap append 1 PushList } ;   // [[56, "conf"]]  #f:c
: op-qref  { tk-val [57] swap append 1 PushList } ;   // [[57, name]] 压引号函数指针
: op-call  { tk-val [96] swap append 1 PushList } ;    // [[96, name]]
: op-pick  { tk-val [4] swap append 1 PushList } ;     // [[4, N]]
: op-hof   { dup tk-type swap tk-val compile 2 PushList 1 PushList } ; // [[op', inner_bc]]
: op-op    { tk-val 1 PushList } ;                      // [[opcode]]

// Quote: compile inner body, emit single [PushRef, inner_bc] instruction
//
// ⚠️ 死代码（2026-09-21 收口时再次确认，勿据此判断引号可达性）：
// 这个 opcode `53` 在整个 compiler.all.ws 里**只出现在这里**——cgen 没有
// 53 分支，真走到它会落到 `#error whisper-cgen: unknown bare opcode`。
// 引号之所以能用，靠的是 `fuse-hof` 把"引号作值"抬成顶层静态函数
// （`[9, name, body]` 定义 + `[57, name]` 引用），与这条路径无关。
// 保留而不删的理由：`compile-one` 将来若要支持"引号完全内联"，入口在这里；
// 但那条路要先把嵌套函数 trampoline 的生命周期问题解决（见 spec §16 末尾）。
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
    patch-leaves
} ;

// ── leave backpatch: rewrite [3 254] markers to [81 offset] jumps ──
// Queue-style: [src dst] consumes one element per call. A leave seen
// when dst has k elements sits at flat position k; total length is
// len(src)+len(dst); jump target (past the trailing [82 off]) is
// total, so offset = total - k - 1 = len(src) - 1.
: pl-leave? {
    // compiled IR: bare opcodes are plain i64 scalars (dup=0, .=144, ...)
    // a leave marker is the scalar 254
    dup i64? ?? _ 254 = ` drop | drop #f ]
} ;

: pl-head {
    over len 0 = ?? return ]
    $1 0 @nth pl-leave? ??
        $1 len 1 - [81] ` append append
    |
        $1 0 @nth append
    ]
    swap 1 drop-n swap
} ;

: patch-leaves {
    []
    pl-until
    ` drop
} ;
: pl-until {
    over len 0 = ?? return ]
    pl-head
    pl-until
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
    dup 14 =  ?? drop op-conf1 return ]
    dup 12 =  ?? drop op-conf0 return ]
    dup 16 =  ?? drop op-qref  return ]
    dup 15 =  ?? drop op-pick  return ]
    dup 249 >= over 254 <= & ?? drop op-hof return ]
    drop drop
    // K-3：不再静默空码——发 #error 让 gcc 即刻失败（覆盖门禁的响亮化）
    [["#error whisper-cgen: unknown token\n"]]
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
// next-consumer-op: is the token AFTER the head a higher-order consumer?
// The length guard matters: when the head quote is also the LAST token there
// is no element 1, and an unguarded `1 @nth` aborts the compiler with
// "Index out of bounds" (a program ending in a bare quotation — e.g. `{ 1 + }`
// on its own — was enough to trigger it).
: next-consumer-op {
    dup len 2 < ?? 0 return ]
    dup 1 @nth consumer-op
} ;
// 非融合引号（引号作参数：`{ 1 + } map`）提升为顶层合成词 __qN。
// kv 计数器产生全局唯一名（kv 只存字符串铁律）。
: genq-name {
    // The counter string fetched below is already the branch's input, so the
    // FALSE path must consume it. It used to re-read "__QN" and leave the
    // original copy on the stack — invisible with a single quote (the TRUE
    // path drops it), but every extra quote left a stray counter string that
    // pushed the real operands out of reach and broke fuse-hof with
    // "Type mismatch: expected [T], got str".
    "__QN" kv-get dup strlen 0 = ??
        drop "__q0" "0" "__QN" ` kv-set
    |   strtoi64 1 + i64tostr dup "__QN" ` kv-set "__q" swap strcat
    ]
} ;

// ── 引号抬升：把容器节点内部的定义抬到顶层 ──────────────────────
// fuse-hof 的"引号作值"分支会把一个引号展开成一对 token：定义
// [9, "__qN", body] 与引用 [16, "__qN"]。**只有落在顶层的定义**才会被
// split-defs 收走、发射成真正的静态函数 W___qN；落在任何容器节点内部的
// 定义都会走到 compile-one 的兜底分支（它只认 tag 16 的引用，没有 tag 9
// 分支），变成 `#error whisper-cgen: unknown bare opcode`，而引用就此悬空
// （gcc 报 `'W___q0' undeclared`）。
//
// 容器有五种，通路只有一条：
//   列表 [6,…] · 条件 [7,…] · 循环 [8,…] · 词定义 [9,…] · 高阶融合体 [249..254,…]
//
//   part : 一条 token 流 → [defs, rest]
//          （先在本层 fuse-hof 展开裸引号，再把 [9,…] 定义拣出来）
//   hs   : 一个容器节点 → defs… ++ [node']
//          （拆开字段：是列表就当 token 流交给 part，是标量就原样保留
//            —— [9, name, body] 的 name 正走这条）
//
// 「是不是容器」用 tag 区间判（6..9 与 249..254）：新增容器类型只要落进这
// 两个区间就自动被覆盖，不必再复制一份状态机。
//
// 栈纪律（这几段代码的前提，违反即静默错位）：这些循环只能假设「调用者栈
// 之上恰好压着自己的一件本地值」，靠 `dup`（取 $0）与 `over`（取 $1）访问；
// 绝不能用 `$N`——它数的是整条栈，会把调用者的东西卷进来。`rot` 只在窗口
// 三件全属自己时才用。

// 是不是定义节点 [9, name, body]。与 is-quote-tok? 同构：先问 list? 再取头，
// 免得把标量当列表取下标（`0 @nth` 对标量是类型错）。
: is-def-tok? {
    dup list? ??
        _ 0 @nth 9 = swap drop return ]
    drop #f
} ;

// 划分。状态 = [tokens, rest_acc, defs_acc]（与 split-defs-iter 同构）。
// `[] [] rot mk3` 的 rot 窗口是 (fused, [], [])，三件全属自己，安全。
: part {
    fuse-hof
    [] [] rot mk3
    part-iter
} ;
: part-iter {
    dup 0 @nth len 0 = ??
        // 收尾：返回 [defs, rest]
        dup 2 @nth over 1 @nth 2 PushList
        swap drop
        return
    ]
    dup 0 @nth 0 @nth is-def-tok? ??
        // 定义：抬进 defs_acc，rest_acc 原样
        dup 0 @nth 0 @nth               // state, elem
        over 2 @nth swap append         // state, defs'
        swap dup 0 @nth tok-advance     // defs', state, tokens'
        swap 1 @nth swap mk3            // [tokens', rest, defs']
        part-iter
        return
    ]
    // 普通 token：留在 rest_acc，defs_acc 原样
    dup 0 @nth 0 @nth                   // state, elem
    over 1 @nth swap append             // state, rest'
    swap dup 0 @nth tok-advance         // rest', state, tokens'
    swap 2 @nth rot rot mk3             // [tokens', rest', defs_acc]
    part-iter
} ;

// 容器节点 → defs… ++ [node']；非容器原样包成单元素 token 流。
: hs {
    dup 0 @nth
    dup 6 = ?? drop hs1 return ]                    // 列表：1 个流字段 @1
    dup 7 = ?? drop hs2 return ]                    // 条件：2 个流字段 @1,@2
    dup 8 = ?? drop hs2 return ]                    // 循环：2 个流字段 @1,@2
    dup 9 = ?? drop hs9 return ]                     // 定义：流字段 @2（@1 是名字）
    dup 249 >= over 254 <= & ?? drop hs1 return ]   // 高阶融合体：流字段 @1
    drop 1 PushList
} ;
// 单流字段 @1（列表、高阶融合体）
: hs1 {
    dup 1 @nth part                     // [node, P]
    dup 0 @nth rot 0 @nth rot 1 @nth    // [d, kind, r]
    2 PushList append
} ;
// 双流字段 @1,@2（条件、循环）
//
// 先把原料归一成一份"料单"，再从料单里逐项取值：每次取值都是「dup … @nth
// 后 swap」，把料单换回栈顶——料单因此始终可再取，取出的值依次沉到它下面，
// 最后 drop 掉料单即得到按读取顺序排好的取值序列。反过来指望 `rot` 把料单
// 捞上来是不行的：rot 只转栈顶三件，四件一组时最底下那件永远上不来。
// 取值顺序按最终装配顺序挑，装配只用 `2 PushList` 打包 + `concat-bc` 拼接，
// 同样不依赖四件以上的重排。
: hs2 {
    dup 0 @nth 1 PushList               // [node, K]
    over 1 @nth part                    // [node, K, P1]
    rot 2 @nth part                     // [K, P1, P2]
    2 PushList                          // [K, PP]  PP = [P1, P2]
    dup 0 @nth 1 @nth swap              // [K, r1, PP]   PP[0][1]
    dup 1 @nth 1 @nth swap              // [K, r1, r2, PP]
    dup 1 @nth 0 @nth swap              // [K, r1, r2, d2, PP]
    dup 0 @nth 0 @nth swap              // [K, r1, r2, d2, d1, PP]
    drop concat-bc                      // [K, r1, r2, A]   A = d1 ++ d2
    rot rot 2 PushList rot concat-bc append
} ;
// 流字段 @2（词定义；@1 是名字，字符串，原样保留）
: hs9 {
    dup 0 @nth 1 PushList               // [node, K]
    over 1 @nth                         // [node, K, name]
    rot 2 @nth part                     // [K, name, P]
    dup 0 @nth swap                     // [K, name, d, P]
    dup 1 @nth swap                     // [K, name, d, r, P]
    drop                                // [K, name, d, r]
    4 PushList                          // [SL]  SL = [K, name, d, r]
    dup 2 @nth swap                     // [d, SL]
    dup 0 @nth swap                     // [d, K, SL]
    dup 1 @nth swap                     // [d, K, name, SL]
    dup 3 @nth swap                     // [d, K, name, r, SL]
    drop                                // [d, K, name, r]
    2 PushList swap concat-bc append     // d ++ [[kind, name, r]]
} ;

: fuse-hof {
    dup len 1 < ?? return ]
    tok-head-quote? ??
        next-consumer-op dup 0 > ??
            over 0 @nth 1 @nth
            2 PushList
            // 新造出来的融合体 [249..254, inner] 也要过一遍 hs：引号体里
            // 若还有引号，它的定义同样得抬到顶层，否则 op-hof 编译 inner
            // 时会撞上 compile-one 的 tag 9 兜底。
            hs
            swap tok-advance tok-advance
            fuse-hof
            swap concat-bc
            return
        | drop
          // Build BOTH tokens before recursing, and keep the generated name
          // on the stack instead of in a shared KV scratch key.
          //
          // The old form parked the name under "__FH-N", recursed, then read
          // the key back. A nested fuse-hof (i.e. a second non-fused quote)
          // overwrote that key, so the outer reference was emitted with the
          // INNER quote's name and pointed at the wrong def. One quote never
          // recursed and so never showed it; two quotes broke with
          // "Type mismatch: expected [T], got str".
          genq-name
          over 0 @nth 1 @nth
          over 9 mk3
          [16] $2 append
          rot drop
          ` rot
          tok-advance fuse-hof
          // 同理：刚合成的定义 [9, "__qN", inner] 可能装着更深的引号
          // （`{ { 5 } }` 的外层引号体就是内层引号），过一遍 hs 把里层的
          // 定义一并抬出来。
          // 拼装用 concat-bc（拼的是"列表的项"），所以引用必须先 1 PushList
          // 包成单元素列表，否则 [16, "__qN"] 会被摊成两个 token。
          rot 1 PushList rot hs swap concat-bc concat-bc
          return
        ]
    | ]
    // 容器节点（列表/条件/循环/词定义/高阶融合体）：先在它的字段流里做同一
    // 趟融合（引号在容器内部同样要处理），再把合成出来的定义抬到容器之外
    // ——顶层 split-defs 才收得到。非容器时 hs 等价于 [head] 原样返回，
    // 所以这里不必先判种类。
    dup 0 @nth                       // [tokens, head]
    dup list? ??
        hs                          // [tokens, defs… ++ [node']]
        swap tok-advance            // [spliced, tokens']
        fuse-hof                    // [spliced, fused_rest]
        swap concat-bc              // spliced ++ fused_rest
        return
    | drop ]
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

// 行尾归一化（CRLF → LF）。与 Rust 前端 `Lexer::new` 里的
// `normalize_newlines` 对称——**两边都要做**，否则同一个程序在三条路径上
// 会给出不同结果：`whisper run` 走 Rust lexer（修前保留 \r）、`-t c2` 的
// 源码在 Rust 侧经 `str::lines()` 已被吃掉 \r、自举链从 stdin 读时被
// Windows 文本模式吃掉 \r，而 `wd` 用 `read-file`（二进制）读，\r 又回来了。
// 差别不是外观：跨行字符串字面量里的那个 \r 会直接变成程序数据
// （`"a\r\nb" strlen` 给 4 还是 3）。
// 只折叠 CRLF 这一对；单独的 \r 是合法字符，保留。
: normalize-eol { "\r\n" "\n" strreplace } ;

: compile-full {
    normalize-eol               // 行尾归一化（见上）
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
    dup 7  = ?? drop "{int64_t _n=w_as_i64(w_pop());w_push(W_PTR(rt_ctos(_n)));}\n" return ]
    dup 8  = ?? drop "{int64_t _n=w_as_i64(w_pop());int64_t _s=w_as_i64(w_pop());wval*_l=(wval*)w_as_ptr(w_pop());w_push(W_PTR(w_list_slice(_l,_s,_n)));}\n" return ]
    dup 16 = ?? drop "w_op_add();\n" return ]
    dup 17 = ?? drop "w_op_sub();\n" return ]
    dup 18 = ?? drop "w_op_mul();\n" return ]
    dup 24 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push(W_BOOL(w_val_eq(_a,_b)?1L:0L));}\n" return ]
    dup 25 = ?? drop "w_op_lt();\n" return ]
    dup 26 = ?? drop "w_op_gt();\n" return ]
    dup 28 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push(W_BOOL(w_num_f64(_a)<=w_num_f64(_b)?1L:0L));}\n" return ]
    dup 29 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push(W_BOOL(w_num_f64(_a)>=w_num_f64(_b)?1L:0L));}\n" return ]
    dup 32 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push(W_BOOL(w_as_i64(_a)&&w_as_i64(_b)?1L:0L));}\n" return ]
    dup 33 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push(W_BOOL(w_as_i64(_a)||w_as_i64(_b)?1L:0L));}\n" return ]
    dup 34 = ?? drop "{wval _a=w_pop();w_push(W_BOOL(w_as_i64(_a)==0?1L:0L));}\n" return ]
    dup 52 = ?? drop "{int64_t _n=w_as_i64(w_pop());wval*_l=w_list_new(_n);for(int64_t _i=_n-1;_i>=0;_i--)_l[1+_i]=w_pop();w_push(W_PTR(_l));}\n" return ]
    dup 64 = ?? drop "{int64_t _i=w_as_i64(w_pop());wval*_l=(wval*)w_as_ptr(w_pop());w_push(w_list_nth(_l,_i));}\n" return ]
    dup 65 = ?? drop "{wval _e=w_pop();wval*_l=(wval*)w_as_ptr(w_pop());w_push(W_PTR(w_list_append(_l,_e)));}\n" return ]
    dup 66 = ?? drop "{wval*_l=(wval*)w_as_ptr(w_pop());w_push(W_I64(w_list_len(_l)));}\n" return ]
    dup 70 = ?? drop "{const char*_s=(const char*)w_as_ptr(w_pop());w_push(W_I64((int64_t)w_str_len(_s)));}\n" return ]
    dup 71 = ?? drop "{const char*_b=(const char*)w_as_ptr(w_pop());const char*_a=(const char*)w_as_ptr(w_pop());w_push(W_PTR(w_str_concat(_a,_b)));}\n" return ]
    dup 72 = ?? drop "{int64_t _n=w_as_i64(w_pop());int64_t _s=w_as_i64(w_pop());const char*_str=(const char*)w_as_ptr(w_pop());w_push(W_PTR(w_str_slice(_str,_s,_n)));}\n" return ]
    dup 73 = ?? drop "{const char*_b=(const char*)w_as_ptr(w_pop());const char*_a=(const char*)w_as_ptr(w_pop());w_push(W_BOOL(w_str_eq(_a,_b)));}\n" return ]
    dup 77 = ?? drop "{const char*_s=(const char*)w_as_ptr(w_pop());w_push(W_I64(w_str_to_i64(_s)));}\n" return ]
    dup 78 = ?? drop "{int64_t _n=w_as_i64(w_pop());w_push(W_PTR(w_i64_to_str(_n)));}\n" return ]
    dup 79 = ?? drop "{int64_t _i=w_as_i64(w_pop());const char*_s=(const char*)w_as_ptr(w_pop());w_push(W_I64((int64_t)w_str_nth(_s,_i)));}\n" return ]
    dup 6  = ?? drop "{const char*_d=(const char*)w_as_ptr(w_pop());const char*_s=(const char*)w_as_ptr(w_pop());w_push(W_PTR(w_str_split(_s,_d)));}\n" return ]
    dup 9  = ?? drop "{int64_t _b=w_as_i64(w_pop());int64_t _a=w_as_i64(w_pop());w_push(W_I64(_a<<_b));}\n" return ]
    dup 10 = ?? drop "{int64_t _b=w_as_i64(w_pop());int64_t _a=w_as_i64(w_pop());w_push(W_I64(_a>>_b));}\n" return ]
    dup 11 = ?? drop "w_push(W_PTR(w_args()));\n" return ]
    dup 19 = ?? drop "w_op_div();\n" return ]
    dup 20 = ?? drop "w_op_mod();\n" return ]
    dup 27 = ?? drop "{wval _b=w_pop(),_a=w_pop();w_push(W_BOOL(w_val_eq(_a,_b)?0L:1L));}\n" return ]
    dup 74 = ?? drop "{const char*_b=(const char*)w_as_ptr(w_pop());const char*_a=(const char*)w_as_ptr(w_pop());w_push(W_I64(w_str_lt(_a,_b)));}\n" return ]
    dup 75 = ?? drop "{const char*_n=(const char*)w_as_ptr(w_pop());const char*_h=(const char*)w_as_ptr(w_pop());w_push(W_I64(w_str_find(_h,_n)));}\n" return ]
    dup 76 = ?? drop "{const char*_nw=(const char*)w_as_ptr(w_pop());const char*_ow=(const char*)w_as_ptr(w_pop());const char*_s=(const char*)w_as_ptr(w_pop());w_push(W_PTR(w_str_replace(_s,_ow,_nw)));}\n" return ]
    dup 97 = ?? drop "return;\n" return ]
    dup 144 = ?? drop "w_print_val(w_pop());\n" return ]
    dup 145 = ?? drop "w_output_all();\n" return ]
    // L-1: 裸打印（不补换行、不改写引号）与真退出码。
    // 0x93/0x94 —— 不能取 0xC8/0xC9：那是 listdir/mkdir（十进制 200/201）。
    dup 147 = ?? drop "{const char*_s=(const char*)w_as_ptr(w_pop());w_print_raw(_s);}\n" return ]
    dup 148 = ?? drop "{int64_t _c=w_as_i64(w_pop());exit((int)_c);}\n" return ]
    dup 176 = ?? drop "{int64_t _n=w_as_i64(w_pop());w_push(W_F64((double)_n));}\n" return ]
    dup 177 = ?? drop "{double _f=w_as_f64(w_pop());w_push(W_I64((int64_t)_f));}\n" return ]
    dup 178 = ?? drop "{double _f=w_as_f64(w_pop());w_push(W_F64(w_fsqrt(_f)));}\n" return ]
    dup 179 = ?? drop "{double _f=w_as_f64(w_pop());_f=w_fsin(_f);w_push(W_F64(_f));}\n" return ]
    dup 180 = ?? drop "{double _f=w_as_f64(w_pop());_f=w_fcos(_f);w_push(W_F64(_f));}\n" return ]
    dup 181 = ?? drop "{double _f=w_as_f64(w_pop());_f=w_ftan(_f);w_push(W_F64(_f));}\n" return ]
    dup 184 = ?? drop "{const char*_s=(const char*)w_as_ptr(w_pop());w_push(W_PTR(w_strchars(_s)));}\n" return ]
    dup 185 = ?? drop "{wval*_l=(wval*)w_as_ptr(w_pop());w_push(W_PTR(w_charsstr(_l)));}\n" return ]
    dup 186 = ?? drop "{const char*_s=(const char*)w_as_ptr(w_pop());w_striter(_s);}\n" return ]
    dup 187 = ?? drop "{wval _e=w_pop();wval*_l=(wval*)w_as_ptr(w_pop());w_push(W_I64((int64_t)w_listfind(_l,_e)));}\n" return ]
    dup 188 = ?? drop "{wval*_l=(wval*)w_as_ptr(w_pop()); const char*_s=(const char*)w_as_ptr(w_pop()); w_push(W_PTR(w_strjoin(_l,_s)));}\n" return ]
    dup 194 = ?? drop "w_push(W_PTR(w_map_new()));\n" return ]
    dup 195 = ?? drop "{const char*_k=(const char*)w_as_ptr(w_pop());wval*_m=(wval*)w_as_ptr(w_pop());w_push(w_map_get(_m,_k));}\n" return ]
    dup 196 = ?? drop "{wval _v=w_pop();const char*_k=(const char*)w_as_ptr(w_pop());wval*_m=(wval*)w_as_ptr(w_pop());w_push(W_PTR(w_map_set(_m,_k,_v)));}\n" return ]
    dup 197 = ?? drop "{const char*_k=(const char*)w_as_ptr(w_pop());wval*_m=(wval*)w_as_ptr(w_pop());w_push(W_PTR(w_map_del(_m,_k)));}\n" return ]
    dup 198 = ?? drop "{wval*_m=(wval*)w_as_ptr(w_pop());w_push(W_PTR(w_map_keys(_m)));}\n" return ]
    dup 199 = ?? drop "{const char*_k=(const char*)w_as_ptr(w_pop());wval*_m=(wval*)w_as_ptr(w_pop());w_push(W_BOOL((int64_t)w_map_has(_m,_k)));}\n" return ]
    dup 208 = ?? drop "w_push(W_I64(w_now()));\n" return ]
    dup 209 = ?? drop "{int64_t _ms=w_as_i64(w_pop());w_sleep(_ms);}\n" return ]
    dup 210 = ?? drop "{double _f=w_random();w_push(W_F64(_f));}\n" return ]
    dup 211 = ?? drop "{int64_t _mx=w_as_i64(w_pop());int64_t _mn=w_as_i64(w_pop());w_push(W_I64(w_randint(_mn,_mx)));}\n" return ]
    dup 212 = ?? drop "{double _f=w_as_f64(w_pop());w_push(W_F64(floor(_f)));}\n" return ]
    dup 213 = ?? drop "{double _f=w_as_f64(w_pop());w_push(W_F64(ceil(_f)));}\n" return ]
    dup 214 = ?? drop "{double _f=w_as_f64(w_pop());w_push(W_F64(round(_f)));}\n" return ]
    dup 215 = ?? drop "{double _f=w_as_f64(w_pop());w_push(W_F64(w_flog(_f)));}\n" return ]
    dup 216 = ?? drop "{double _f=w_as_f64(w_pop());w_push(W_F64(w_fexp(_f)));}\n" return ]
    dup 217 = ?? drop "{double _x=w_num_f64(w_pop());double _y=w_num_f64(w_pop());w_push(W_F64(w_fatan2(_y,_x)));}\n" return ]
    dup 218 = ?? drop "{int64_t _b=w_as_i64(w_pop());int64_t _a=w_as_i64(w_pop());w_push(W_I64(_a&_b));}\n" return ]
    dup 219 = ?? drop "{int64_t _b=w_as_i64(w_pop());int64_t _a=w_as_i64(w_pop());w_push(W_I64(_a|_b));}\n" return ]
    dup 220 = ?? drop "{int64_t _b=w_as_i64(w_pop());int64_t _a=w_as_i64(w_pop());w_push(W_I64(_a^_b));}\n" return ]
    dup 221 = ?? drop "{int64_t _a=w_as_i64(w_pop());w_push(W_I64(~_a));}\n" return ]
    dup 222 = ?? drop "{wval*_l=(wval*)w_as_ptr(w_pop());wval _e=w_pop();w_push(W_PTR(w_list_prepend(_e,_l)));}\n" return ]
    dup 223 = ?? drop "{wval*_l=(wval*)w_as_ptr(w_pop());w_push(W_PTR(w_list_sort(_l)));}\n" return ]
    dup 224 = ?? drop "{wval*_l=(wval*)w_as_ptr(w_pop());w_push(W_PTR(w_list_rev(_l)));}\n" return ]
    dup 225 = ?? drop "{wval*_l=(wval*)w_as_ptr(w_pop());w_push(W_PTR(w_list_dedup(_l)));}\n" return ]
    dup 226 = ?? drop "{int64_t _n=w_as_i64(w_pop());wval*_l=(wval*)w_as_ptr(w_pop());w_push(W_PTR(w_list_take(_l,_n)));}\n" return ]
    dup 227 = ?? drop "{int64_t _n=w_as_i64(w_pop());wval*_l=(wval*)w_as_ptr(w_pop());w_push(W_PTR(w_list_drop(_l,_n)));}\n" return ]
    dup 228 = ?? drop "w_list_();\n" return ]
    dup 229 = ?? drop "w_i64_();\n" return ]
    dup 230 = ?? drop "w_str_();\n" return ]
    dup 231 = ?? drop "w_bool_();\n" return ]
    dup 232 = ?? drop "w_push(W_PTR(w_read_all()));\n" return ]
    dup 234 = ?? drop "{int64_t _n=w_as_i64(w_pop());w_push(W_PTR(w_buf_new(_n)));}\n" return ]
    dup 235 = ?? drop "{wval _e=w_pop();wval*_b=(wval*)w_as_ptr(w_pop());w_push(W_PTR(w_buf_push(_b,_e)));}\n" return ]
    dup 240 = ?? drop "{int64_t _start=w_as_i64(w_pop());wval*_toks=(wval*)w_as_ptr(w_pop());wval*_th=0,*_el=0;int _next=0;split_cond(_toks,(int)_start,&_th,&_el,&_next);wval*_r=w_list_new(3);_r[1]=W_PTR((void*)_th);_r[2]=W_PTR((void*)_el);_r[3]=W_I64((int64_t)_next);w_push(W_PTR(_r));}\n" return ]
    dup 236 = ?? drop "{ int64_t _p=w_as_i64(w_pop()); w_push(w_sock_listen(_p)); }
" return ]
    dup 237 = ?? drop "{ wval _h=w_pop(); w_push(w_sock_accept(_h)); }
" return ]
    dup 238 = ?? drop "{ wval _c=w_pop(); w_push(w_sock_recv(_c)); }
" return ]
    dup 239 = ?? drop "{ const char *_s=(const char*)w_as_ptr(w_pop()); wval _c=w_pop(); w_push(w_sock_send(_c,_s)); }
" return ]
    dup 241 = ?? drop "{ wval _c=w_pop(); w_sock_close(_c); }
" return ]
    dup 242 = ?? drop "{ const char *_p=(const char*)w_as_ptr(w_pop()); w_push(w_read_file(_p)); }
" return ]
    dup 243 = ?? drop "{ const char *_d=(const char*)w_as_ptr(w_pop()); const char *_p=(const char*)w_as_ptr(w_pop()); w_push(w_write_file(_p,_d)); }
" return ]
    dup 244 = ?? drop "{ const char *_n=(const char*)w_as_ptr(w_pop()); w_push(w_env_get(_n)); }
" return ]
    dup 245 = ?? drop "{ const char *_v=(const char*)w_as_ptr(w_pop()); const char *_k=(const char*)w_as_ptr(w_pop()); w_kv_set(_k,_v); }
" return ]
    dup 246 = ?? drop "{ const char *_k=(const char*)w_as_ptr(w_pop()); w_push(w_kv_get(_k)); }
" return ]
    dup 247 = ?? drop "{ const char *_k=(const char*)w_as_ptr(w_pop()); w_kv_del(_k); }
" return ]
    dup 248 = ?? drop "w_push(W_PTR(w_kv_keys_list()));
" return ]
    dup 255 = ?? drop "{ const char *_n=(const char*)w_as_ptr(w_pop()); int64_t _p=w_as_i64(w_pop()); w_serve_concurrent(_p, w_find_word(_n)); }
" return ]
    dup 249 = ?? drop "{ w_push(w_read_line()); }
" return ]
    dup 250 = ?? drop "{ w_push(w_str_case(w_pop(),1)); }
" return ]
    dup 251 = ?? drop "{ w_push(w_str_case(w_pop(),0)); }
" return ]
    dup 252 = ?? drop "{ w_push(w_str_strip(w_pop())); }
" return ]
    dup 253 = ?? drop "{ wval _v=w_pop(); wval _f=w_pop(); w_push(w_fmt(_f,_v)); }
" return ]
    dup 200 = ?? drop "{ w_push(w_listdir(w_pop())); }
" return ]
    dup 201 = ?? drop "{ w_push(w_mkdir(w_pop())); }
" return ]
    dup 202 = ?? drop "{ struct stat _st; w_push(W_I64((int64_t)(stat((const char*)w_as_ptr(w_pop()),&_st)==0))); }
" return ]
    dup 203 = ?? drop "{ const char*_f=(const char*)w_as_ptr(w_pop()); int64_t _t=w_as_i64(w_pop()); w_push(w_strftime_utc(_t,_f)); }
" return ]
    dup 254 = ?? drop "
" return ]    dup 233 = ?? drop "{ const char*_p=(const char*)w_as_ptr(w_pop()); const char*_s=(const char*)w_as_ptr(w_pop()); w_push(w_strmatch(_s,_p)); }
" return ]    dup 12 = ?? drop "{ const char*_f=(const char*)w_as_ptr(w_pop()); wval *_v=(wval*)w_as_ptr(w_pop()); w_push(w_pack(W_PTR((void*)_f),W_PTR((void*)_v))); }
" return ]
    dup 13 = ?? drop "{ const char*_f=(const char*)w_as_ptr(w_pop()); const char*_b=(const char*)w_as_ptr(w_pop()); w_push(w_unpack(W_PTR((void*)_f),W_PTR((void*)_b))); }
" return ]
    dup 205 = ?? drop "{ int64_t _c=w_as_i64(w_pop()); int64_t _s=w_as_i64(w_pop()); wval*_l=w_list_new(_c); for(int64_t _i=0;_i<_c;_i++)_l[1+_i]=W_I64(_s+_i); w_push(W_PTR(_l)); }\n" return ]
    dup 204 = ?? drop "{ const char*_p=(const char*)w_as_ptr(w_pop()); const char*_s=(const char*)w_as_ptr(w_pop()); w_push(w_strfind_re(_s,_p)); }
" return ]
    dup 150 = ?? drop "{ w_push(w_ffi_open(w_pop())); }
" return ]
    dup 151 = ?? drop "{ const char*_n=(const char*)w_as_ptr(w_pop()); int64_t _h=w_as_i64(w_pop()); w_push(W_I64((int64_t)(uintptr_t)w_ffi_sym(_h,_n))); }
" return ]
    dup 152 = ?? drop "{ int64_t _f=w_as_i64(w_pop()); const char*_fmt=(const char*)w_as_ptr(w_pop()); wval*_a=(wval*)w_as_ptr(w_pop()); w_push(W_I64(w_ffi_call(_f,_fmt,_a))); }
" return ]
    dup 153 = ?? drop "{ int64_t _n=w_as_i64(w_pop()); w_push(W_PTR((void*)calloc((size_t)(_n>0?_n:1),1)); }
" return ]
    dup 154 = ?? drop "{ free((void*)w_as_i64(w_pop())); }
" return ]
    dup 155 = ?? drop "{ uint64_t _v=0; memcpy(&_v,(const void*)(uintptr_t)w_as_i64(w_pop()),8); w_push(W_I64((int64_t)_v)); }
" return ]
    dup 156 = ?? drop "{ uint64_t _v=(uint64_t)w_as_i64(w_pop()); memcpy((void*)(uintptr_t)w_as_i64(w_pop()),&_v,8); }
" return ]
    dup 157 = ?? drop "{ int64_t _l=w_as_i64(w_pop()); const char*_p=(const char*)w_as_i64(w_pop()); w_push(W_PTR(w_str_new(_p,(uint32_t)(_l>0?_l:0)))); }
" return ]
    dup 47 = ?? drop "{ wval _v=(w_sp>0)?w_stack[w_sp-1]:W_I64(0); w_push(W_F64(w_conf(_v))); }\n" return ]
    dup 54 = ?? drop "{ wval _v=w_pop(); if (_v.t==WT_SIG) _v=w_sig_inner(_v); w_push(_v); }\n" return ]
    dup 182 = ?? drop "{ w_push(w_json_parse_cstr((const char*)w_as_ptr(w_pop()))); }\n" return ]
    dup 183 = ?? drop "{ wval _v=w_pop(); if (_v.t==WT_SIG) _v=w_sig_inner(_v); w_push(W_PTR(w_json_stringify(_v))); }\n" return ]
    // 裸 HOF（stdlib 包装词：: map { @map } ; ——引号运行时才到栈上）
    dup 67 = ?? drop "{ wfn _f=(wfn)w_as_ptr(w_pop()); wval *_l=(wval*)w_as_ptr(w_pop()); w_push(W_PTR(w_map_over(_l,_f))); }\n" return ]
    dup 68 = ?? drop "{ wfn _f=(wfn)w_as_ptr(w_pop()); wval *_l=(wval*)w_as_ptr(w_pop()); w_each_over(_l,_f); }\n" return ]
    dup 69 = ?? drop "{ wfn _f=(wfn)w_as_ptr(w_pop()); wval _a=w_pop(); wval *_l=(wval*)w_as_ptr(w_pop()); w_push(w_fold_over(_l,_a,_f)); }\n" return ]
    dup 83 = ?? drop "{ wfn _f=(wfn)w_as_ptr(w_pop()); int64_t _n=w_as_i64(w_pop()); for(int64_t _i=0;_i<_n;_i++) _f(); }\n" return ]
    dup 5  = ?? drop "{ wfn _f=(wfn)w_as_ptr(w_pop()); wval *_l=(wval*)w_as_ptr(w_pop()); w_push(W_PTR(w_filter_over(_l,_f))); }\n" return ]
    dup 193 = ?? drop "{ w_try_call((wfn)w_as_ptr(w_pop())); }\n" return ]
    drop "#error whisper-cgen: unknown bare opcode\n"
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
        "{wval _c=w_pop();if(!w_as_i64(_c))goto L_" swap ";}\n" strcat strcat
        return
    ]
    over 81 = ??
        swap drop
        "goto L_" swap ";\n" strcat strcat
        return
    ]
    swap drop
    "{wval _c=w_pop();if(w_as_i64(_c))goto L_" swap ";}\n" strcat strcat
} ;

// ── Emit single instruction → C string ───────────────────────────
: emit-one {
    dup list? ??
        dup 0 @nth
        dup 48 = ?? drop 1 @nth i64tostr "w_push(W_I64((int64_t)" swap "L));\n" strcat strcat return ]
        dup 49 = ?? drop 1 @nth "{double _d=" swap "; w_push(W_F64(_d));}\n" strcat strcat return ]
        dup 249 = ?? drop 1 @nth emit-func-body "{ auto void _qb(void) { " swap strcat " } wval *_l=(wval*)w_as_ptr(w_pop()); int64_t _n=w_list_len(_l); wval *_r=w_list_new(_n); for (int64_t _i=0;_i<_n;_i++){ w_push(_l[1+_i]); _qb(); _r[1+_i]=w_pop(); } w_push(W_PTR(_r)); }\n" strcat return ]
        dup 250 = ?? drop 1 @nth emit-func-body "{ auto void _qb(void) { " swap strcat " } wval *_l=(wval*)w_as_ptr(w_pop()); int64_t _n=w_list_len(_l); for (int64_t _i=0;_i<_n;_i++){ int _s0=w_sp; w_push(_l[1+_i]); _qb(); if (w_sp>_s0) w_sp=_s0; } }\n" strcat return ]
        dup 251 = ?? drop 1 @nth emit-func-body "{ auto void _qb(void) { " swap strcat " } wval _a=w_pop(); wval *_l=(wval*)w_as_ptr(w_pop()); int64_t _n=w_list_len(_l); for (int64_t _i=0;_i<_n;_i++){ w_push(_a); w_push(_l[1+_i]); _qb(); _a=w_pop(); } w_push(_a); }\n" strcat return ]
        dup 252 = ?? drop 1 @nth emit-func-body "{ auto void _qb(void) { " swap strcat " } int64_t _n=w_as_i64(w_pop()); for (int64_t _i=0;_i<_n;_i++){ _qb(); } }\n" strcat return ]
        dup 253 = ?? drop 1 @nth emit-func-body "{ auto void _qb(void) { " swap strcat " } wval *_l=(wval*)w_as_ptr(w_pop()); int64_t _n=w_list_len(_l); wval *_tmp=(wval*)malloc((_n>0?_n:1)*sizeof(wval)); int64_t _c=0; for (int64_t _i=0;_i<_n;_i++){ w_push(_l[1+_i]); _qb(); if (w_as_i64(w_pop())!=0) _tmp[_c++]=_l[1+_i]; } wval *_r=w_list_new(_c); for (int64_t _j=0;_j<_c;_j++) _r[1+_j]=_tmp[_j]; free(_tmp); w_push(W_PTR(_r)); }\n" strcat return ]
        dup 254 = ?? drop 1 @nth emit-func-body "{ auto void _qb(void) { " swap strcat " } w_try_call(_qb); }\n" strcat return ]
        dup 50 = ?? drop 1 @nth escape-c-str "w_push(W_PTR(w_str_from_cstr(\"" swap "\")));\n" strcat strcat return ]
        dup 51 = ?? drop 1 @nth i64tostr "w_push(W_BOOL((int64_t)" swap "L));\n" strcat strcat return ]
        dup 55 = ?? drop 1 @nth "{ wval _s=w_sig(W_BOOL(1)," swap "); w_push(_s); }\n" strcat strcat return ]
        dup 56 = ?? drop 1 @nth "{ wval _s=w_sig(W_BOOL(0)," swap "); w_push(_s); }\n" strcat strcat return ]
        dup 57 = ?? drop 1 @nth safe-c-name "w_push(W_PTR((void*)W_" swap "));\n" strcat strcat return ]
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
