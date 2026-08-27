// Whisper Bootstrap Seed Compiler — one-time artifact, frozen after the
// self-hosting fixed point is reached (see whisperc/build.sh).
//
//   gcc -O2 -o whisper0 seed/compiler_seed.c -lm
//   ./whisper0 < program.ws          # interpret from stdin
//   ./whisper0 -c < program.ws       # emit C code to stdout
//
// The seed implements the full front-end (tokenize → structify → classify
// → split-defs → bytecode) plus two backends: a bytecode interpreter (dev
// harness) and a C emitter (bootstrap vehicle).  The Whisper-written
// compiler in whisperc/src/*.ws replaces everything here once it can
// compile itself to byte-identical C.
//
// Token/tag conventions shared with whisperc/src/*.ws:
//   tokens: flat strings; string literals carry a leading '"' marker
//   tags:   5=quote({}) 6=list([]) 7=cond(??|]) 8=loop({}{}#) 9=def(:{};)
//   classified: [type, value] with 0=i64 1=f64 2=str 3=op 4=word
//               5=quote 6=list 7=cond 8=loop 9=def 13=bool 15=pick(N)
//   bytecode: flat list; raw int = operand-less opcode,
//             [byte, operand...] = opcode with operand

#include "../runtime.c"

// ══════════════════════════════════════════════════════════════════════
// Tokenizer
// ══════════════════════════════════════════════════════════════════════

static int is_delim(char c) {
    return c == '{' || c == '}' || c == '[' || c == ']' ||
           c == ';' || c == ':' || c == '|';
}

static wval c_tokenize(const char *src) {
    wval *tokens = w_buf_new(64);
    int len = (int)w_str_len(src);
    int i = 0;
    char *buf = (char *)malloc((size_t)len + 1);
    if (!buf) { fprintf(stderr, "seed: oom\n"); exit(1); }
    while (i < len) {
        char c = src[i];
        char next = (i + 1 < len) ? src[i + 1] : 0;
        if (c == ' ' || c == '\t' || c == '\n' || c == '\r') { i++; continue; }
        if (c == '/' && next == '/') {
            while (i < len && src[i] != '\n') i++;
            continue;
        }
        if (c == '"' ) {
            i++;
            int n = 0;
            while (i < len && src[i] != '"') {
                if (src[i] == '\\' && i + 1 < len) {
                    i++;
                    switch (src[i]) {
                        case 'n': buf[n++] = '\n'; break;
                        case 't': buf[n++] = '\t'; break;
                        case 'r': buf[n++] = '\r'; break;
                        case '0': buf[n++] = '\0'; break;
                        default:  buf[n++] = src[i]; break;
                    }
                } else {
                    buf[n++] = src[i];
                }
                i++;
            }
            if (i < len) i++;  // closing quote
            // store with '"' marker prefix so classify sees a string literal
            char *m = (char *)w_alloc(4 + 1 + n + 1);
            *(uint32_t *)m = 1 + n;
            char *s = m + 4;
            s[0] = '"';
            memcpy(s + 1, buf, n);
            s[1 + n] = 0;
            tokens = w_buf_push(tokens, (wval)s);
            continue;
        }
        if (c == '?' && next == '?') {
            tokens = w_buf_push(tokens, (wval)w_str_new("??", 2));
            i += 2;
            continue;
        }
        if (is_delim(c)) {
            tokens = w_buf_push(tokens, (wval)w_str_new(src + i, 1));
            i++;
            continue;
        }
        if (c == '#' && next != 't' && next != 'f') {
            tokens = w_buf_push(tokens, (wval)w_str_new(src + i, 1));
            i++;
            continue;
        }
        int start = i;
        while (i < len && src[i] != ' ' && src[i] != '\t' &&
               src[i] != '\n' && src[i] != '\r' && !is_delim(src[i]) &&
               !(src[i] == '?' && i + 1 < len && src[i + 1] == '?') &&
               !(src[i] == '#' && (i + 1 >= len || (src[i + 1] != 't' && src[i + 1] != 'f'))))
            i++;
        tokens = w_buf_push(tokens, (wval)w_str_new(src + start, i - start));
    }
    free(buf);
    return (wval)tokens;
}

// ══════════════════════════════════════════════════════════════════════
// Structify: group {}/[] then recognize def/loop/cond patterns
// ══════════════════════════════════════════════════════════════════════

static wval c_structify(wval tokens_val);

static wval c_collect_balanced(wval *tokens, int start, const char *open,
                               const char *close, int *next_out) {
    wval *inner = w_buf_new(64);
    int depth = 1, i = start;
    int tlen = w_list_len(tokens);
    while (i < tlen && depth > 0) {
        const char *s = (const char *)tokens[1 + i];
        if (strcmp(s, open) == 0) depth++;
        else if (strcmp(s, close) == 0) {
            depth--;
            if (depth == 0) { *next_out = i + 1; break; }
        }
        if (depth > 0) inner = w_buf_push(inner, tokens[1 + i]);
        i++;
    }
    if (depth > 0) *next_out = tlen;
    return (wval)inner;
}

static wval c_recognize(wval tokens_val) {
    wval *tokens = (wval *)tokens_val;
    int len = w_list_len(tokens);
    wval *result = w_buf_new(64);
    int i = 0;
    while (i < len) {
        // export NAME / import NAME → module directives, compile to nothing
        if (i + 1 < len) {
            const char *s0 = (const char *)tokens[1 + i];
            if (wval_is_str(tokens[1+i]) && (strcmp(s0, "export") == 0 || strcmp(s0, "import") == 0)) {
                i += 2;
                continue;
            }
        }
        // : name {body} ;  →  [9, name, body]
        if (i + 3 < len) {
            const char *s0 = (const char *)tokens[1 + i];
            wval v1 = tokens[1 + i + 1];
            wval v2 = tokens[1 + i + 2];
            const char *s3 = (const char *)tokens[1 + i + 3];
            if (wval_is_str(tokens[1+i]) && strcmp(s0, ":") == 0 && wval_is_heap(v1) && !is_tag(v1, 5) &&
                !is_tag(v1, 6) && is_tag(v2, 5) && wval_is_str(tokens[1+i+3]) && strcmp(s3, ";") == 0) {
                wval *def = w_list_new(3);
                def[1] = (wval)9LL;
                def[2] = v1;
                def[3] = c_structify(((wval *)v2)[2]);
                result = w_buf_push(result, (wval)def);
                i += 4;
                continue;
            }
        }
        // {body} {cond} #  →  [8, body, cond]
        if (i + 2 < len) {
            wval v0 = tokens[1 + i];
            wval v1 = tokens[1 + i + 1];
            const char *s2 = (const char *)tokens[1 + i + 2];
            if (is_tag(v0, 5) && is_tag(v1, 5) && wval_is_str(tokens[1+i+2]) && strcmp(s2, "#") == 0) {
                wval *loop = w_list_new(3);
                loop[1] = (wval)8LL;
                loop[2] = c_structify(((wval *)v0)[2]);
                loop[3] = c_structify(((wval *)v1)[2]);
                result = w_buf_push(result, (wval)loop);
                i += 3;
                continue;
            }
        }
        // ?? then | else ]  →  [7, then, else]
        if (wval_is_str(tokens[1 + i])) {
            const char *s = (const char *)tokens[1 + i];
            if (strcmp(s, "??") == 0) {
                wval *th, *el;
                int next;
                split_cond(tokens, i + 1, &th, &el, &next);
                wval *cond = w_list_new(3);
                cond[1] = (wval)7LL;
                cond[2] = c_structify((wval)th);
                cond[3] = c_structify((wval)el);
                result = w_buf_push(result, (wval)cond);
                i = next;
                continue;
            }
        }
        result = w_buf_push(result, tokens[1 + i]);
        i++;
    }
    return (wval)result;
}

static wval c_structify(wval tokens_val) {
    wval *tokens = (wval *)tokens_val;
    int len = w_list_len(tokens);
    wval *result = w_buf_new(64);
    int i = 0;
    while (i < len) {
        const char *s = (const char *)tokens[1 + i];
        if (strcmp(s, "{") == 0) {
            int next;
            wval inner = c_collect_balanced(tokens, i + 1, "{", "}", &next);
            wval *group = w_list_new(2);
            group[1] = (wval)5LL;
            group[2] = c_structify(inner);
            result = w_buf_push(result, (wval)group);
            i = next;
        } else if (strcmp(s, "[") == 0) {
            int next;
            wval inner = c_collect_balanced(tokens, i + 1, "[", "]", &next);
            wval *group = w_list_new(2);
            group[1] = (wval)6LL;
            group[2] = c_structify(inner);
            result = w_buf_push(result, (wval)group);
            i = next;
        } else {
            result = w_buf_push(result, tokens[1 + i]);
            i++;
        }
    }
    return c_recognize((wval)result);
}

// ══════════════════════════════════════════════════════════════════════
// Classify: word → [type, value]
// ══════════════════════════════════════════════════════════════════════

#define OP1(w, op) else if (strcmp(s, #w) == 0) { tok[1] = 3LL; tok[2] = (wval)(int64_t)(op); }

static wval c_classify_token(const char *s) {
    wval *tok = w_list_new(2);
    uint32_t slen = w_str_len(s);
    if (slen >= 1 && s[0] == '"') {
        tok[1] = 2LL;
        tok[2] = (wval)w_str_new(s + 1, slen - 1);  // strip marker
    }
    else if (strcmp(s, "#t") == 0) { tok[1] = 13LL; tok[2] = 1LL; }
    else if (strcmp(s, "#f") == 0) { tok[1] = 13LL; tok[2] = 0LL; }
    OP1(+, 16) OP1(-, 17) OP1(*, 18) OP1(/, 19) OP1(%, 20)
    OP1(=, 24) OP1(<, 25) OP1(>, 26) OP1(!=, 27) OP1(<=, 28) OP1(>=, 29)
    OP1(&, 32) OP1(|, 33) OP1(!, 34)
    OP1(_, 0) OP1(@, 3)
    OP1(., 144) OP1(.., 145)
    else if (strcmp(s, ",") == 0) { tok[1] = 3LL; tok[2] = 146LL; }
    OP1(:, 160) OP1(;, 161)
    OP1(??, 80) OP1(#, 83)
    OP1(dup, 0) OP1(drop, 2) OP1(swap, 1) OP1(rot, 3)
    OP1(len, 66) OP1(append, 65) OP1(prepend, 222)
    OP1(strlen, 70) OP1(strcat, 71) OP1(strslice, 72) OP1(streq, 73)
    OP1(strlt, 74) OP1(strfind, 75) OP1(strreplace, 76)
    OP1(strtoi64, 77) OP1(i64tostr, 78) OP1(strnth, 79)
    OP1(strsplit, 6) OP1(ctos, 7) OP1(list-slice, 8)
    OP1(strjoin, 188) OP1(strchars, 184) OP1(charsstr, 185)
    OP1(striter, 186) OP1(listfind, 187)
    OP1(return, 97) OP1(import, 162) OP1(export, 163)
    OP1(i64tof64, 176) OP1(f64toi64, 177)
    OP1(fsqrt, 178) OP1(fsin, 179) OP1(fcos, 180) OP1(ftan, 181)
    OP1(json-parse, 182) OP1(json-stringify, 183)
    OP1(map-new, 194) OP1(map-get, 195) OP1(map-set, 196)
    OP1(map-del, 197) OP1(map-keys, 198) OP1(map-has, 199)
    OP1(sort, 223) OP1(rev, 224) OP1(dedup, 225)
    OP1(take, 226) OP1(drop-n, 227) OP1(try, 193)
    OP1(@nth, 64) OP1(@map, 67) OP1(@each, 68) OP1(@fold, 69)
    OP1(@filter, 5) OP1(@times, 83) OP1(read-all, 232)
    OP1(PushList, 52)
    OP1(buf-new, 234) OP1(buf-push, 235)
    OP1(split-cond, 240)
    OP1(args, 11) OP1(now, 208) OP1(sleep, 209)
    OP1(random, 210) OP1(randint, 211)
    OP1(floor, 212) OP1(ceil, 213) OP1(round, 214)
    OP1(log, 215) OP1(exp, 216) OP1(atan2, 217)
    OP1(bitand, 218) OP1(bitor, 219) OP1(bitxor, 220) OP1(bitnot, 221)
    OP1(shl, 9) OP1(shr, 10)
    else if (strcmp(s, "`") == 0) { tok[1] = 3LL; tok[2] = 1LL; }
    else if (strcmp(s, "$") == 0) { tok[1] = 3LL; tok[2] = 4LL; }
    // over = pick 1 (second stack item)
    else if (strcmp(s, "over") == 0) { tok[1] = 15LL; tok[2] = 1LL; }
    // local ISA extension: type predicates (no upstream opcode assigned)
    else if (strcmp(s, "list?") == 0) { tok[1] = 3LL; tok[2] = 228LL; }
    else if (strcmp(s, "i64?") == 0) { tok[1] = 3LL; tok[2] = 229LL; }
    else if (strcmp(s, "str?") == 0) { tok[1] = 3LL; tok[2] = 230LL; }
    else if (strcmp(s, "bool?") == 0) { tok[1] = 3LL; tok[2] = 231LL; }
    else if (slen > 1 && s[0] == '$' && s[1] >= '0' && s[1] <= '9') {
        tok[1] = 15LL;
        tok[2] = (wval)(int64_t)strtoll(s + 1, NULL, 10);
    }
    else if (strcmp(s, "list?") == 0 || strcmp(s, "i64?") == 0 ||
             strcmp(s, "str?") == 0 || strcmp(s, "bool?") == 0) {
        tok[1] = 4LL;
        tok[2] = (wval)w_str_new(s, slen);
    }
    else {
        char *endp;
        long long n = strtoll(s, &endp, 10);
        if (*endp == '\0' && endp != s) {
            tok[1] = 0LL; tok[2] = (wval)n;
        } else {
            double d = strtod(s, &endp);
            if (*endp == '\0' && endp != s && memchr(s, '.', slen)) {
                tok[1] = 1LL; tok[2] = *(wval *)&d;
            } else {
                tok[1] = 4LL; tok[2] = (wval)w_str_new(s, slen);
            }
        }
    }
    return (wval)tok;
}

static wval c_classify_nested(wval tokens_val) {
    wval *tokens = (wval *)tokens_val;
    int len = w_list_len(tokens);
    wval *result = w_buf_new(64);
    for (int i = 0; i < len; i++) {
        wval item = tokens[1 + i];
        if (wval_is_heap(item)) {
            wval *p = (wval *)item;
            int plen = w_list_len(p);
            if (plen == 2 && (int64_t)p[1] == 5LL) {
                wval *tagged = w_list_new(2);
                tagged[1] = 5LL;
                tagged[2] = c_classify_nested(p[2]);
                result = w_buf_push(result, (wval)tagged);
            } else if (plen == 2 && (int64_t)p[1] == 6LL) {
                wval *tagged = w_list_new(2);
                tagged[1] = 6LL;
                tagged[2] = c_classify_nested(p[2]);
                result = w_buf_push(result, (wval)tagged);
            } else if (plen == 3 && (int64_t)p[1] == 7LL) {
                wval *r = w_list_new(3);
                r[1] = (wval)7LL;
                r[2] = c_classify_nested(p[2]);
                r[3] = c_classify_nested(p[3]);
                result = w_buf_push(result, (wval)r);
            } else if (plen == 3 && (int64_t)p[1] == 8LL) {
                wval *r = w_list_new(3);
                r[1] = (wval)8LL;
                r[2] = c_classify_nested(p[2]);
                r[3] = c_classify_nested(p[3]);
                result = w_buf_push(result, (wval)r);
            } else if (plen == 3 && (int64_t)p[1] == 9LL) {
                wval *r = w_list_new(3);
                r[1] = (wval)9LL;
                r[2] = p[2];                    // name stays a raw string
                r[3] = c_classify_nested(p[3]); // classify body
                result = w_buf_push(result, (wval)r);
            } else if (w_is_list_val(item)) {
                // list literal: classify each element recursively
                int64_t count = (int64_t)p[0];
                wval *cls = w_list_new(count);
                for (int j = 0; j < count; j++)
                    cls[1 + j] = c_classify_nested(p[1 + j]);
                result = w_buf_push(result, (wval)cls);
            } else {
                result = w_buf_push(result, c_classify_token((const char *)item));
            }
        } else {
            result = w_buf_push(result, item);
        }
    }
    return (wval)result;
}

// ══════════════════════════════════════════════════════════════════════
// Split defs: classified → [main_tokens, [[name, body], ...]]
// ══════════════════════════════════════════════════════════════════════

static int c_is_def_token(wval tok) {
    if (!wval_is_heap(tok)) return 0;
    wval *p = (wval *)tok;
    return w_list_len(p) == 3 && (int64_t)p[1] == 9LL;
}

static wval c_split_defs(wval classified) {
    wval *tokens = (wval *)classified;
    int len = w_list_len(tokens);
    wval *main_tokens = w_buf_new(64);
    wval *defs_list = w_buf_new(64);
    for (int i = 0; i < len; i++) {
        wval tok = tokens[1 + i];
        if (c_is_def_token(tok)) {
            wval *def = (wval *)tok;
            wval *pair = w_list_new(2);
            pair[1] = def[2]; // name
            pair[2] = def[3]; // classified body
            defs_list = w_buf_push(defs_list, (wval)pair);
        } else {
            main_tokens = w_buf_push(main_tokens, tok);
        }
    }
    wval *result = w_list_new(2);
    result[1] = (wval)main_tokens;
    result[2] = (wval)defs_list;
    return (wval)result;
}

// ══════════════════════════════════════════════════════════════════════
// Compile: classified tokens → flat bytecode list
// ══════════════════════════════════════════════════════════════════════

static wval c_compile_tokens(wval tokens_val);

static void append_bc_all(wval **ops, wval bc_val) {
    wval *src = (wval *)bc_val;
    int n = w_list_len(src);
    for (int j = 0; j < n; j++) *ops = w_buf_push(*ops, src[1 + j]);
}

static wval c_compile_token(wval tok) {
    wval *ops = w_buf_new(64);
    if (!wval_is_heap(tok)) return (wval)ops;
    wval *t = (wval *)tok;
    if (w_list_len(t) < 2) return (wval)ops;

    int64_t type = (int64_t)t[1];
    wval val = t[2];

    switch (type) {
        case 0: case 13: {  // I64 / Bool literal
            wval *op = w_list_new(2);
            op[1] = (type == 0) ? 0x30LL : 0x33LL;
            op[2] = val;
            ops = w_buf_push(ops, (wval)op);
            break;
        }
        case 1: {  // F64 literal (raw bits)
            wval *op = w_list_new(2);
            op[1] = 0x31LL;
            op[2] = val;
            ops = w_buf_push(ops, (wval)op);
            break;
        }
        case 2: {  // String literal
            wval *op = w_list_new(2);
            op[1] = 0x32LL;
            op[2] = val;
            ops = w_buf_push(ops, (wval)op);
            break;
        }
        case 3: {  // Operator — raw int
            ops = w_buf_push(ops, val);
            break;
        }
        case 4: {  // Word reference → Call
            wval *op = w_list_new(2);
            op[1] = 0x60LL;
            op[2] = val;
            ops = w_buf_push(ops, (wval)op);
            break;
        }
        case 5: {  // Quote { } → compile inner, PushRef
            wval inner_bc = c_compile_tokens(val);
            wval *op = w_list_new(2);
            op[1] = 0x35LL;
            op[2] = inner_bc;
            ops = w_buf_push(ops, (wval)op);
            break;
        }
        case 6: {  // List literal [ ] → elems + count + PushList
            wval inner_bc = c_compile_tokens(val);
            append_bc_all(&ops, inner_bc);
            wval *cnt = w_list_new(2);
            cnt[1] = 0x30LL;
            cnt[2] = (wval)(int64_t)w_list_len((wval *)val);  // element count
            ops = w_buf_push(ops, (wval)cnt);
            ops = w_buf_push(ops, 0x34LL);
            break;
        }
        case 15: {  // Pick(N)
            wval *op = w_list_new(2);
            op[1] = 0x04LL;
            op[2] = val;
            ops = w_buf_push(ops, (wval)op);
            break;
        }
        case 7: {  // Cond [7, then, else]
            wval then_bc = c_compile_tokens(t[2]);
            wval else_bc = c_compile_tokens(t[3]);
            int64_t then_len = w_list_len((wval *)then_bc);
            int64_t else_len = w_list_len((wval *)else_bc);
            wval *cond_op = w_list_new(2);
            cond_op[1] = 0x50LL;
            cond_op[2] = (wval)(then_len + 1);
            ops = w_buf_push(ops, (wval)cond_op);
            append_bc_all(&ops, then_bc);
            wval *jmp_op = w_list_new(2);
            jmp_op[1] = 0x51LL;
            jmp_op[2] = (wval)else_len;
            ops = w_buf_push(ops, (wval)jmp_op);
            append_bc_all(&ops, else_bc);
            break;
        }
        case 8: {  // Loop [8, body, cond] → body + cond + Loop(-(bl+cl+1))
            wval body_bc = c_compile_tokens(t[2]);
            wval cond_bc = c_compile_tokens(t[3]);
            int64_t body_len = w_list_len((wval *)body_bc);
            int64_t cond_len = w_list_len((wval *)cond_bc);
            append_bc_all(&ops, body_bc);
            append_bc_all(&ops, cond_bc);
            wval *loop_op = w_list_new(2);
            loop_op[1] = 0x52LL;
            loop_op[2] = (wval)(-(body_len + cond_len + 1));
            ops = w_buf_push(ops, (wval)loop_op);
            break;
        }
        default:
            break;
    }
    return (wval)ops;
}

static wval c_compile_tokens(wval tokens_val) {
    wval *tokens = (wval *)tokens_val;
    int len = w_list_len(tokens);
    wval *result = w_buf_new(64);
    for (int i = 0; i < len; i++)
        append_bc_all(&result, c_compile_token(tokens[1 + i]));
    return (wval)result;
}

// ══════════════════════════════════════════════════════════════════════
// Interpreter (dev harness — semantics reference for the emitter)
// ══════════════════════════════════════════════════════════════════════

#define MAX_WORDS 1024
static char *word_names[MAX_WORDS];
static wval word_bytecodes[MAX_WORDS];
static int word_count = 0;

static void register_word(const char *name, wval bytecodes) {
    if (word_count < MAX_WORDS) {
        word_names[word_count] = w_str_new(name, strlen(name));
        word_bytecodes[word_count] = bytecodes;
        word_count++;
    }
}

static wval lookup_word(const char *name) {
    for (int i = 0; i < word_count; i++)
        if (strcmp(word_names[i], name) == 0) return word_bytecodes[i];
    return (wval)0;
}

static void c_execute_bytecodes(wval bytecodes_val) {
    wval *bcs = (wval *)bytecodes_val;
    int len = w_list_len(bcs);
    int ip = 0;
    while (ip < len) {
        wval bc = bcs[1 + ip];
        if (!wval_is_heap(bc)) {
            int op = (int)bc;
            switch (op) {
                case 0x00: w_dup(); break;
                case 0x01: w_swap(); break;
                case 0x02: w_drop(); break;
                case 0x03: w_rot(); break;
                case 0x04: w_pick(); break;
                case 0x05: { // Filter — quote is a bytecode list here
                    wval q = w_pop(); wval *l = (wval *)w_pop();
                    int64_t count = w_list_len(l);
                    wval *result = w_buf_new(64);
                    for (int64_t i2 = 0; i2 < count; i2++) {
                        w_push(l[1 + i2]);
                        c_execute_bytecodes(q);
                        if ((int64_t)w_pop() != 0)
                            result = w_buf_push(result, l[1 + i2]);
                    }
                    w_push((wval)result);
                } break;
                case 0x06: { const char*d=(const char*)w_pop(); const char*s=(const char*)w_pop(); w_push((wval)w_str_split(s,d)); } break;
                case 0x07: { int64_t c=(int64_t)w_pop(); w_push((wval)rt_ctos(c)); } break;
                case 0x08: { int64_t n=(int64_t)w_pop(); int64_t st=(int64_t)w_pop(); wval*l=(wval*)w_pop(); w_push((wval)w_list_slice(l,st,n)); } break;
                case 0x09: { int64_t b=(int64_t)w_pop(); int64_t a=(int64_t)w_pop(); w_push((wval)(a<<b)); } break;
                case 0x0A: { int64_t b=(int64_t)w_pop(); int64_t a=(int64_t)w_pop(); w_push((wval)(a>>b)); } break;
                case 0x0B: w_push((wval)w_args()); break;
                case 0x10: { wval b=w_pop(),a=w_pop(); w_push((wval)((int64_t)a+(int64_t)b)); } break;
                case 0x11: { wval b=w_pop(),a=w_pop(); w_push((wval)((int64_t)a-(int64_t)b)); } break;
                case 0x12: { wval b=w_pop(),a=w_pop(); w_push((wval)((int64_t)a*(int64_t)b)); } break;
                case 0x13: { wval b=w_pop(),a=w_pop(); if(!(int64_t)b)exit(1); w_push((wval)((int64_t)a/(int64_t)b)); } break;
                case 0x14: { wval b=w_pop(),a=w_pop(); if(!(int64_t)b)exit(1); w_push((wval)((int64_t)a%(int64_t)b)); } break;
                case 0x18: { wval b=w_pop(),a=w_pop(); w_push((wval)((int64_t)a==(int64_t)b?1L:0L)); } break;
                case 0x19: { wval b=w_pop(),a=w_pop(); w_push((wval)((int64_t)a<(int64_t)b?1L:0L)); } break;
                case 0x1A: { wval b=w_pop(),a=w_pop(); w_push((wval)((int64_t)a>(int64_t)b?1L:0L)); } break;
                case 0x1B: { wval b=w_pop(),a=w_pop(); w_push((wval)((int64_t)a!=(int64_t)b?1L:0L)); } break;
                case 0x1C: { wval b=w_pop(),a=w_pop(); w_push((wval)((int64_t)a<=(int64_t)b?1L:0L)); } break;
                case 0x1D: { wval b=w_pop(),a=w_pop(); w_push((wval)((int64_t)a>=(int64_t)b?1L:0L)); } break;
                case 0x20: { wval b=w_pop(),a=w_pop(); w_push((wval)((int64_t)a&&(int64_t)b?1L:0L)); } break;
                case 0x21: { wval b=w_pop(),a=w_pop(); w_push((wval)((int64_t)a||(int64_t)b?1L:0L)); } break;
                case 0x22: { wval a=w_pop(); w_push((wval)((int64_t)a==0?1L:0L)); } break;
                case 0x34: { // PushList: pop n, collect n values
                    int64_t n=(int64_t)w_pop();
                    wval *l = w_list_new(n);
                    for (int64_t i2 = n - 1; i2 >= 0; i2--) l[1 + i2] = w_pop();
                    w_push((wval)l);
                } break;
                case 0x40: { int64_t i2=(int64_t)w_pop(); wval*l=(wval*)w_pop(); w_push(w_list_nth(l,i2)); } break;
                case 0x41: { wval e=w_pop(); wval*l=(wval*)w_pop(); w_push((wval)w_list_append(l,e)); } break;
                case 0x42: { wval*l=(wval*)w_pop(); w_push((wval)w_list_len(l)); } break;
                case 0x43: { // Map — quote is a bytecode list here
                    wval q = w_pop(); wval *l = (wval *)w_pop();
                    int64_t count = w_list_len(l);
                    wval *result = w_list_new(count);
                    for (int64_t i2 = 0; i2 < count; i2++) {
                        w_push(l[1 + i2]);
                        c_execute_bytecodes(q);
                        result[1 + i2] = w_pop();
                    }
                    w_push((wval)result);
                } break;
                case 0x44: { // Each — discard everything the quote leaves
                    wval q = w_pop(); wval *l = (wval *)w_pop();
                    int64_t count = w_list_len(l);
                    for (int64_t i2 = 0; i2 < count; i2++) {
                        int sp_before = w_sp;
                        w_push(l[1 + i2]);
                        c_execute_bytecodes(q);
                        if (w_sp > sp_before) w_sp = sp_before;
                    }
                } break;
                case 0x45: { // Fold
                    wval q = w_pop(); wval ini = w_pop(); wval *l = (wval *)w_pop();
                    int64_t count = w_list_len(l);
                    wval acc = ini;
                    for (int64_t i2 = 0; i2 < count; i2++) {
                        w_push(acc);
                        w_push(l[1 + i2]);
                        c_execute_bytecodes(q);
                        acc = w_pop();
                    }
                    w_push(acc);
                } break;
                case 0x46: { const char*s=(const char*)w_pop(); w_push((wval)(int64_t)w_str_len(s)); } break;
                case 0x47: { const char*b=(const char*)w_pop(); const char*a=(const char*)w_pop(); w_push((wval)w_str_concat(a,b)); } break;
                case 0x48: { int64_t n=(int64_t)w_pop(); int64_t st=(int64_t)w_pop(); const char*s=(const char*)w_pop(); w_push((wval)w_str_slice(s,st,n)); } break;
                case 0x49: { const char*b=(const char*)w_pop(); const char*a=(const char*)w_pop(); w_push((wval)w_str_eq(a,b)); } break;
                case 0x4A: { const char*b=(const char*)w_pop(); const char*a=(const char*)w_pop(); w_push((wval)w_str_lt(a,b)); } break;
                case 0x4B: { const char*n2=(const char*)w_pop(); const char*h=(const char*)w_pop(); w_push((wval)w_str_find(h,n2)); } break;
                case 0x4C: { const char*nw=(const char*)w_pop(); const char*ow=(const char*)w_pop(); const char*sr=(const char*)w_pop(); w_push((wval)w_str_replace(sr,ow,nw)); } break;
                case 0x4D: { const char*s=(const char*)w_pop(); w_push((wval)w_str_to_i64(s)); } break;
                case 0x4E: { int64_t n=(int64_t)w_pop(); w_push((wval)w_i64_to_str(n)); } break;
                case 0x4F: { int64_t i2=(int64_t)w_pop(); const char*s=(const char*)w_pop(); w_push((wval)(int64_t)w_str_nth(s,i2)); } break;
                case 0x61: return;
                case 0x53: { // Times: [n, quote] → run quote n times
                    wval q = w_pop(); int64_t n = (int64_t)w_pop();
                    for (int64_t k = 0; k < n; k++) c_execute_bytecodes(q);
                } break;
                case 0xE8: w_push((wval)w_read_all()); break;
                case 0xEA: w_push((wval)w_buf_new((int64_t)w_pop())); break;
                case 0xEB: { wval _e=w_pop(); wval*_b=(wval*)w_pop(); w_push((wval)w_buf_push(_b,_e)); } break;
                case 0x90: w_print_val(w_pop()); break;
                case 0x91: w_output_all(); break;
                case 0x92: w_push((wval)w_read_line()); break;
                case 0xB0: { int64_t n=(int64_t)w_pop(); double f=(double)n; w_push((wval)*(uint64_t*)&f); } break;
                case 0xB1: { uint64_t bits=(uint64_t)w_pop(); double f=*(double*)&bits; w_push((wval)(int64_t)f); } break;
                case 0xB2: { uint64_t bits=(uint64_t)w_pop(); double f=*(double*)&bits; f=w_fsqrt(f); w_push((wval)*(uint64_t*)&f); } break;
                case 0xB3: { uint64_t bits=(uint64_t)w_pop(); double f=*(double*)&bits; f=w_fsin(f); w_push((wval)*(uint64_t*)&f); } break;
                case 0xB4: { uint64_t bits=(uint64_t)w_pop(); double f=*(double*)&bits; f=w_fcos(f); w_push((wval)*(uint64_t*)&f); } break;
                case 0xB5: { uint64_t bits=(uint64_t)w_pop(); double f=*(double*)&bits; f=w_ftan(f); w_push((wval)*(uint64_t*)&f); } break;
                case 0xB8: { const char*s=(const char*)w_pop(); w_push((wval)w_strchars(s)); } break;
                case 0xB9: { wval*l=(wval*)w_pop(); w_push((wval)w_charsstr(l)); } break;
                case 0xBA: { const char*s=(const char*)w_pop(); w_striter(s); } break;
                case 0xBB: { wval e=w_pop(); wval*l=(wval*)w_pop(); w_push((wval)(int64_t)w_listfind(l,e)); } break;
                case 0xBC: { wval*l=(wval*)w_pop(); const char*s=(const char*)w_pop(); w_push((wval)w_strjoin(l,s)); } break;
                case 0xC2: w_push((wval)w_map_new()); break;
                case 0xC3: { const char*k=(const char*)w_pop(); wval*m=(wval*)w_pop(); w_push(w_map_get(m,k)); } break;
                case 0xC4: { wval v=w_pop(); const char*k=(const char*)w_pop(); wval*m=(wval*)w_pop(); w_push((wval)w_map_set(m,k,v)); } break;
                case 0xC5: { const char*k=(const char*)w_pop(); wval*m=(wval*)w_pop(); w_push((wval)w_map_del(m,k)); } break;
                case 0xC6: { wval*m=(wval*)w_pop(); w_push((wval)w_map_keys(m)); } break;
                case 0xC7: { const char*k=(const char*)w_pop(); wval*m=(wval*)w_pop(); w_push((wval)w_map_has(m,k)); } break;
                case 0xD0: w_push((wval)w_now()); break;
                case 0xD1: { int64_t ms=(int64_t)w_pop(); w_sleep(ms); } break;
                case 0xD2: { double f=w_random(); w_push((wval)*(uint64_t*)&f); } break;
                case 0xD3: { int64_t mx=(int64_t)w_pop(); int64_t mn=(int64_t)w_pop(); w_push((wval)w_randint(mn,mx)); } break;
                case 0xD4: { uint64_t bits=(uint64_t)w_pop(); double f=*(double*)&bits; f=floor(f); w_push((wval)*(uint64_t*)&f); } break;
                case 0xD5: { uint64_t bits=(uint64_t)w_pop(); double f=*(double*)&bits; f=ceil(f); w_push((wval)*(uint64_t*)&f); } break;
                case 0xD6: { uint64_t bits=(uint64_t)w_pop(); double f=*(double*)&bits; f=round(f); w_push((wval)*(uint64_t*)&f); } break;
                case 0xD7: { uint64_t bits=(uint64_t)w_pop(); double f=*(double*)&bits; f=w_flog(f); w_push((wval)*(uint64_t*)&f); } break;
                case 0xD8: { uint64_t bits=(uint64_t)w_pop(); double f=*(double*)&bits; f=w_fexp(f); w_push((wval)*(uint64_t*)&f); } break;
                case 0xD9: { uint64_t bx=(uint64_t)w_pop(); uint64_t by=(uint64_t)w_pop(); double x=*(double*)&bx; double y=*(double*)&by; double r=w_fatan2(y,x); w_push((wval)*(uint64_t*)&r); } break;
                case 0xDA: { int64_t b=(int64_t)w_pop(); int64_t a=(int64_t)w_pop(); w_push((wval)(a&b)); } break;
                case 0xDB: { int64_t b=(int64_t)w_pop(); int64_t a=(int64_t)w_pop(); w_push((wval)(a|b)); } break;
                case 0xDC: { int64_t b=(int64_t)w_pop(); int64_t a=(int64_t)w_pop(); w_push((wval)(a^b)); } break;
                case 0xDD: { int64_t a=(int64_t)w_pop(); w_push((wval)(~a)); } break;
                case 0xDE: { wval*l=(wval*)w_pop(); wval e=w_pop(); w_push((wval)w_list_prepend(e,l)); } break;
                case 0xDF: { wval*l=(wval*)w_pop(); w_push((wval)w_list_sort(l)); } break;
                case 0xE0: { wval*l=(wval*)w_pop(); w_push((wval)w_list_rev(l)); } break;
                case 0xE1: { wval*l=(wval*)w_pop(); w_push((wval)w_list_dedup(l)); } break;
                case 0xE2: { int64_t n=(int64_t)w_pop(); wval*l=(wval*)w_pop(); w_push((wval)w_list_take(l,n)); } break;
                case 0xE3: { int64_t n=(int64_t)w_pop(); wval*l=(wval*)w_pop(); w_push((wval)w_list_drop(l,n)); } break;
                case 0xE4: w_list_(); break;
                case 0xE5: w_i64_(); break;
                case 0xE6: w_str_(); break;
                case 0xE7: w_bool_(); break;
                case 0xF0: { // split-cond: (tokens start_idx) → [then, else, next_idx]
                    int64_t start = (int64_t)w_pop();
                    wval *tokens = (wval *)w_pop();
                    if (!tokens) { fprintf(stderr, "split-cond: NULL tokens\n"); exit(1); }
                    wval *th, *el; int next;
                    split_cond(tokens, (int)start, &th, &el, &next);
                    wval *result = w_list_new(3);
                    result[1] = (wval)th;
                    result[2] = (wval)el;
                    result[3] = (wval)(int64_t)next;
                    w_push((wval)result);
                } break;
                default: break;
            }
            ip++;
        } else {
            // [opcode, operand...]
            wval *op = (wval *)bc;
            int64_t byte = (int64_t)op[1];
            switch (byte) {
                case 0x04: { int64_t n=(int64_t)op[2]; w_pick_n((int)n); } break;
                case 0x30: case 0x31: case 0x32: case 0x33: case 0x35:
                    w_push(op[2]);
                    break;
                case 0x50: { wval c=w_pop(); if ((int64_t)c == 0) ip += (int)op[2]; } break;
                case 0x51: ip += (int)op[2] + 1; continue;
                case 0x52: { wval c=w_pop(); if ((int64_t)c != 0) ip += (int)op[2]; } break;
                case 0x60: {
                    const char *name = (const char *)op[2];
                    wval word_bc = lookup_word(name);
                    if (word_bc != 0) c_execute_bytecodes(word_bc);
                    else fprintf(stderr, "seed: undefined word: %s\n", name);
                } break;
                default: break;
            }
            ip++;
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// C emitter
// ══════════════════════════════════════════════════════════════════════

static FILE *emit_out;

static void emit_safe_name(const char *name) {
    uint32_t len = w_str_len(name);
    for (uint32_t i = 0; i < len; i++) {
        unsigned char c = (unsigned char)name[i];
        if ((c >= '0' && c <= '9') || (c >= 'A' && c <= 'Z') ||
            (c >= 'a' && c <= 'z') || c == '_')
            fputc((int)c, emit_out);
        else
            fputc('_', emit_out);
    }
}

static void emit_c_string(const char *s) {
    uint32_t len = w_str_len(s);
    fputc('"', emit_out);
    for (uint32_t i = 0; i < len; i++) {
        unsigned char c = (unsigned char)s[i];
        if (c == '"' || c == '\\') { fputc('\\', emit_out); fputc((int)c, emit_out); }
        else if (c == '\n') fputs("\\n", emit_out);
        else if (c == '\t') fputs("\\t", emit_out);
        else if (c == '\r') fputs("\\r", emit_out);
        else if (c < 32 || c > 126) fprintf(emit_out, "\\%03o", c);
        else fputc((int)c, emit_out);
    }
    fputc('"', emit_out);
}

// Pass A: count PushRef sites (quote functions) in traversal order
static int quote_total = 0;
static void count_quotes(wval bc_val) {
    wval *bcs = (wval *)bc_val;
    int len = w_list_len(bcs);
    for (int i = 0; i < len; i++) {
        wval bc = bcs[1 + i];
        if (wval_is_heap(bc)) {
            wval *op = (wval *)bc;
            if ((int64_t)op[1] == 0x35LL) {
                quote_total++;
                count_quotes(op[2]);
            }
        }
    }
}

typedef struct { int id; wval bc; } QItem;
static QItem qitems[8192];
static int qn = 0, qhead = 0;
static int quote_counter = 0;

static void emit_body(wval bc_val);

static void emit_instruction(wval bc, int p) {
    if (!wval_is_heap(bc)) {
        int op = (int)bc;
        switch (op) {
            case 0x00: fputs("w_dup();\n", emit_out); break;
            case 0x01: fputs("w_swap();\n", emit_out); break;
            case 0x02: fputs("w_drop();\n", emit_out); break;
            case 0x03: fputs("w_rot();\n", emit_out); break;
            case 0x04: fputs("w_pick();\n", emit_out); break;
            case 0x05: fputs("{wval _q=w_pop();wval*_l=(wval*)w_pop();w_push((wval)w_filter_over(_l,(wfn)_q));}\n", emit_out); break;
            case 0x06: fputs("{const char*_d=(const char*)w_pop();const char*_s=(const char*)w_pop();w_push((wval)w_str_split(_s,_d));}\n", emit_out); break;
            case 0x07: fputs("{int64_t _c=(int64_t)w_pop();w_push((wval)rt_ctos(_c));}\n", emit_out); break;
            case 0x08: fputs("{int64_t _n=(int64_t)w_pop();int64_t _s=(int64_t)w_pop();wval*_l=(wval*)w_pop();w_push((wval)w_list_slice(_l,_s,_n));}\n", emit_out); break;
            case 0x09: fputs("{int64_t _b=(int64_t)w_pop();int64_t _a=(int64_t)w_pop();w_push((wval)(_a<<_b));}\n", emit_out); break;
            case 0x0A: fputs("{int64_t _b=(int64_t)w_pop();int64_t _a=(int64_t)w_pop();w_push((wval)(_a>>_b));}\n", emit_out); break;
            case 0x0B: fputs("w_push((wval)w_args());\n", emit_out); break;
            case 0x10: fputs("{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a+(int64_t)_b));}\n", emit_out); break;
            case 0x11: fputs("{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a-(int64_t)_b));}\n", emit_out); break;
            case 0x12: fputs("{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a*(int64_t)_b));}\n", emit_out); break;
            case 0x13: fputs("{wval _b=w_pop(),_a=w_pop();if(!(int64_t)_b)exit(1);w_push((wval)((int64_t)_a/(int64_t)_b));}\n", emit_out); break;
            case 0x14: fputs("{wval _b=w_pop(),_a=w_pop();if(!(int64_t)_b)exit(1);w_push((wval)((int64_t)_a%(int64_t)_b));}\n", emit_out); break;
            case 0x18: fputs("{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a==(int64_t)_b?1L:0L));}\n", emit_out); break;
            case 0x19: fputs("{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a<(int64_t)_b?1L:0L));}\n", emit_out); break;
            case 0x1A: fputs("{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a>(int64_t)_b?1L:0L));}\n", emit_out); break;
            case 0x1B: fputs("{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a!=(int64_t)_b?1L:0L));}\n", emit_out); break;
            case 0x1C: fputs("{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a<=(int64_t)_b?1L:0L));}\n", emit_out); break;
            case 0x1D: fputs("{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a>=(int64_t)_b?1L:0L));}\n", emit_out); break;
            case 0x20: fputs("{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a&&(int64_t)_b?1L:0L));}\n", emit_out); break;
            case 0x21: fputs("{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a||(int64_t)_b?1L:0L));}\n", emit_out); break;
            case 0x22: fputs("{wval _a=w_pop();w_push((wval)((int64_t)_a==0?1L:0L));}\n", emit_out); break;
            case 0x34: fputs("{int64_t _n=(int64_t)w_pop();wval*_l=w_list_new(_n);for(int64_t _i=_n-1;_i>=0;_i--)_l[1+_i]=w_pop();w_push((wval)_l);}\n", emit_out); break;
            case 0x40: fputs("{int64_t _i=(int64_t)w_pop();wval*_l=(wval*)w_pop();w_push(w_list_nth(_l,_i));}\n", emit_out); break;
            case 0x41: fputs("{wval _e=w_pop();wval*_l=(wval*)w_pop();w_push((wval)w_list_append(_l,_e));}\n", emit_out); break;
            case 0x42: fputs("{wval*_l=(wval*)w_pop();w_push((wval)w_list_len(_l));}\n", emit_out); break;
            case 0x43: fputs("{wval _q=w_pop();wval*_l=(wval*)w_pop();w_push((wval)w_map_over(_l,(wfn)_q));}\n", emit_out); break;
            case 0x44: fputs("{wval _q=w_pop();wval*_l=(wval*)w_pop();w_each_over(_l,(wfn)_q);}\n", emit_out); break;
            case 0x45: fputs("{wval _q=w_pop();wval _i=w_pop();wval*_l=(wval*)w_pop();w_push(w_fold_over(_l,_i,(wfn)_q));}\n", emit_out); break;
            case 0x46: fputs("{const char*_s=(const char*)w_pop();w_push((wval)(int64_t)w_str_len(_s));}\n", emit_out); break;
            case 0x47: fputs("{const char*_b=(const char*)w_pop();const char*_a=(const char*)w_pop();w_push((wval)w_str_concat(_a,_b));}\n", emit_out); break;
            case 0x48: fputs("{int64_t _n=(int64_t)w_pop();int64_t _s=(int64_t)w_pop();const char*_c=(const char*)w_pop();w_push((wval)w_str_slice(_c,_s,_n));}\n", emit_out); break;
            case 0x49: fputs("{const char*_b=(const char*)w_pop();const char*_a=(const char*)w_pop();w_push((wval)w_str_eq(_a,_b));}\n", emit_out); break;
            case 0x4A: fputs("{const char*_b=(const char*)w_pop();const char*_a=(const char*)w_pop();w_push((wval)w_str_lt(_a,_b));}\n", emit_out); break;
            case 0x4B: fputs("{const char*_n=(const char*)w_pop();const char*_h=(const char*)w_pop();w_push((wval)w_str_find(_h,_n));}\n", emit_out); break;
            case 0x4C: fputs("{const char*_n=(const char*)w_pop();const char*_o=(const char*)w_pop();const char*_s=(const char*)w_pop();w_push((wval)w_str_replace(_s,_o,_n));}\n", emit_out); break;
            case 0x4D: fputs("{const char*_s=(const char*)w_pop();w_push((wval)w_str_to_i64(_s));}\n", emit_out); break;
            case 0x4E: fputs("{int64_t _n=(int64_t)w_pop();w_push((wval)w_i64_to_str(_n));}\n", emit_out); break;
            case 0x4F: fputs("{int64_t _i=(int64_t)w_pop();const char*_s=(const char*)w_pop();w_push((wval)(int64_t)w_str_nth(_s,_i));}\n", emit_out); break;
            case 0x61: fputs("return;\n", emit_out); break;
            case 0x53: fputs("{wval _q=w_pop();int64_t _n=(int64_t)w_pop();for(int64_t _k=0;_k<_n;_k++)((wfn)_q)();}\n", emit_out); break;
            case 0xE8: fputs("w_push((wval)w_read_all());\n", emit_out); break;
            case 0xEA: fputs("{int64_t _n=(int64_t)w_pop();w_push((wval)w_buf_new(_n));}\n", emit_out); break;
            case 0xEB: fputs("{wval _e=w_pop();wval*_b=(wval*)w_pop();w_push((wval)w_buf_push(_b,_e));}\n", emit_out); break;
            case 0xF0: {
                // split-cond: emit as inline C that calls split_cond
                static int sc_id = 0;
                int id = sc_id++;
                fprintf(emit_out, "{int64_t _start=(int64_t)w_pop();wval*_toks=(wval*)w_pop();"
                        "wval*_th=0,*_el=0;int _next=0;"
                        "split_cond(_toks,(int)_start,&_th,&_el,&_next);"
                        "wval*_r=w_list_new(3);_r[1]=(wval)_th;_r[2]=(wval)_el;_r[3]=(wval)(int64_t)_next;"
                        "w_push((wval)_r);}\n", emit_out);
                (void)id;
            } break;
            case 0x90: fputs("w_print_val(w_pop());\n", emit_out); break;
            case 0x91: fputs("w_output_all();\n", emit_out); break;
            case 0x92: fputs("w_push((wval)w_read_line());\n", emit_out); break;
            case 0xB0: fputs("{int64_t _n=(int64_t)w_pop();double _f=(double)_n;w_push((wval)*(uint64_t*)&_f);}\n", emit_out); break;
            case 0xB1: fputs("{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;w_push((wval)(int64_t)_f);}\n", emit_out); break;
            case 0xB2: fputs("{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;_f=w_fsqrt(_f);w_push((wval)*(uint64_t*)&_f);}\n", emit_out); break;
            case 0xB3: fputs("{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;_f=w_fsin(_f);w_push((wval)*(uint64_t*)&_f);}\n", emit_out); break;
            case 0xB4: fputs("{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;_f=w_fcos(_f);w_push((wval)*(uint64_t*)&_f);}\n", emit_out); break;
            case 0xB5: fputs("{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;_f=w_ftan(_f);w_push((wval)*(uint64_t*)&_f);}\n", emit_out); break;
            case 0xB8: fputs("{const char*_s=(const char*)w_pop();w_push((wval)w_strchars(_s));}\n", emit_out); break;
            case 0xB9: fputs("{wval*_l=(wval*)w_pop();w_push((wval)w_charsstr(_l));}\n", emit_out); break;
            case 0xBA: fputs("{const char*_s=(const char*)w_pop();w_striter(_s);}\n", emit_out); break;
            case 0xBB: fputs("{wval _e=w_pop();wval*_l=(wval*)w_pop();w_push((wval)(int64_t)w_listfind(_l,_e));}\n", emit_out); break;
            case 0xBC: fputs("{const char*_s=(const char*)w_pop(); wval*_l=(wval*)w_pop(); w_push((wval)w_strjoin(_l,_s));}\n", emit_out); break;
            case 0xC2: fputs("w_push((wval)w_map_new());\n", emit_out); break;
            case 0xC3: fputs("{const char*_k=(const char*)w_pop();wval*_m=(wval*)w_pop();w_push(w_map_get(_m,_k));}\n", emit_out); break;
            case 0xC4: fputs("{wval _v=w_pop();const char*_k=(const char*)w_pop();wval*_m=(wval*)w_pop();w_push((wval)w_map_set(_m,_k,_v));}\n", emit_out); break;
            case 0xC5: fputs("{const char*_k=(const char*)w_pop();wval*_m=(wval*)w_pop();w_push((wval)w_map_del(_m,_k));}\n", emit_out); break;
            case 0xC6: fputs("{wval*_m=(wval*)w_pop();w_push((wval)w_map_keys(_m));}\n", emit_out); break;
            case 0xC7: fputs("{const char*_k=(const char*)w_pop();wval*_m=(wval*)w_pop();w_push((wval)w_map_has(_m,_k));}\n", emit_out); break;
            case 0xD0: fputs("w_push((wval)w_now());\n", emit_out); break;
            case 0xD1: fputs("{int64_t _m=(int64_t)w_pop();w_sleep(_m);}\n", emit_out); break;
            case 0xD2: fputs("{double _f=w_random();w_push((wval)*(uint64_t*)&_f);}\n", emit_out); break;
            case 0xD3: fputs("{int64_t _x=(int64_t)w_pop();int64_t _n=(int64_t)w_pop();w_push((wval)w_randint(_n,_x));}\n", emit_out); break;
            case 0xD4: fputs("{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;_f=floor(_f);w_push((wval)*(uint64_t*)&_f);}\n", emit_out); break;
            case 0xD5: fputs("{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;_f=ceil(_f);w_push((wval)*(uint64_t*)&_f);}\n", emit_out); break;
            case 0xD6: fputs("{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;_f=round(_f);w_push((wval)*(uint64_t*)&_f);}\n", emit_out); break;
            case 0xD7: fputs("{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;_f=w_flog(_f);w_push((wval)*(uint64_t*)&_f);}\n", emit_out); break;
            case 0xD8: fputs("{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;_f=w_fexp(_f);w_push((wval)*(uint64_t*)&_f);}\n", emit_out); break;
            case 0xD9: fputs("{uint64_t _bx=(uint64_t)w_pop();uint64_t _by=(uint64_t)w_pop();double _x=*(double*)&_bx;double _y=*(double*)&_by;double _r=w_fatan2(_y,_x);w_push((wval)*(uint64_t*)&_r);}\n", emit_out); break;
            case 0xDA: fputs("{int64_t _b=(int64_t)w_pop();int64_t _a=(int64_t)w_pop();w_push((wval)(_a&_b));}\n", emit_out); break;
            case 0xDB: fputs("{int64_t _b=(int64_t)w_pop();int64_t _a=(int64_t)w_pop();w_push((wval)(_a|_b));}\n", emit_out); break;
            case 0xDC: fputs("{int64_t _b=(int64_t)w_pop();int64_t _a=(int64_t)w_pop();w_push((wval)(_a^_b));}\n", emit_out); break;
            case 0xDD: fputs("{int64_t _a=(int64_t)w_pop();w_push((wval)(~_a));}\n", emit_out); break;
            case 0xDE: fputs("{wval*_l=(wval*)w_pop();wval _e=w_pop();w_push((wval)w_list_prepend(_e,_l));}\n", emit_out); break;
            case 0xDF: fputs("{wval*_l=(wval*)w_pop();w_push((wval)w_list_sort(_l));}\n", emit_out); break;
            case 0xE0: fputs("{wval*_l=(wval*)w_pop();w_push((wval)w_list_rev(_l));}\n", emit_out); break;
            case 0xE1: fputs("{wval*_l=(wval*)w_pop();w_push((wval)w_list_dedup(_l));}\n", emit_out); break;
            case 0xE2: fputs("{int64_t _n=(int64_t)w_pop();wval*_l=(wval*)w_pop();w_push((wval)w_list_take(_l,_n));}\n", emit_out); break;
            case 0xE3: fputs("{int64_t _n=(int64_t)w_pop();wval*_l=(wval*)w_pop();w_push((wval)w_list_drop(_l,_n));}\n", emit_out); break;
            case 0xE4: fputs("w_list_();\n", emit_out); break;
            case 0xE5: fputs("w_i64_();\n", emit_out); break;
            case 0xE6: fputs("w_str_();\n", emit_out); break;
            case 0xE7: fputs("w_bool_();\n", emit_out); break;
            case 0xA0: case 0xA1: case 0xA2: case 0xA3:
                fputs("/* def marker */\n", emit_out); break;
            default:
                fprintf(emit_out, "/* unknown opcode 0x%02x */\n", op);
                break;
        }
    } else {
        wval *op = (wval *)bc;
        int64_t byte = (int64_t)op[1];
        switch (byte) {
            case 0x04:
                fprintf(emit_out, "w_pick_n(%lld);\n", (long long)(int64_t)op[2]);
                break;
            case 0x30:
                fprintf(emit_out, "w_push((wval)(int64_t)%lldL);\n", (long long)(int64_t)op[2]);
                break;
            case 0x31:
                fprintf(emit_out, "w_push((wval)0x%016llxULL);\n", (unsigned long long)op[2]);
                break;
            case 0x32:
                fputs("w_push((wval)w_str_from_cstr(", emit_out);
                emit_c_string((const char *)op[2]);
                fputs("));\n", emit_out);
                break;
            case 0x33:
                fprintf(emit_out, "w_push((wval)%lldL);\n", (long long)(int64_t)op[2]);
                break;
            case 0x35: {
                int id = quote_counter++;
                fprintf(emit_out, "w_push((wval)(wfn)W_q%d);\n", id);
                if (qn < 8192) { qitems[qn].id = id; qitems[qn].bc = op[2]; qn++; }
                else { fprintf(stderr, "seed: too many quotes\n"); exit(1); }
                break;
            }
            case 0x50:
                fprintf(emit_out, "{wval _c=w_pop();if(!(int64_t)_c)goto L_%d;}\n",
                        p + (int)(int64_t)op[2] + 1);
                break;
            case 0x51:
                fprintf(emit_out, "goto L_%d;\n", p + (int)(int64_t)op[2] + 1);
                break;
            case 0x52:
                fprintf(emit_out, "{wval _c=w_pop();if((int64_t)_c)goto L_%d;}\n",
                        p + (int)(int64_t)op[2] + 1);
                break;
            case 0x60:
                fputs("W_", emit_out);
                emit_safe_name((const char *)op[2]);
                fputs("();\n", emit_out);
                break;
            default:
                fputs("/* unknown */\n", emit_out);
                break;
        }
    }
}

static void emit_body(wval bc_val) {
    wval *bcs = (wval *)bc_val;
    int len = w_list_len(bcs);
    char *labels = (char *)calloc((size_t)len + 1, 1);
    if (!labels) { fprintf(stderr, "seed: oom\n"); exit(1); }
    for (int p = 0; p < len; p++) {
        wval bc = bcs[1 + p];
        if (wval_is_heap(bc)) {
            wval *op = (wval *)bc;
            int64_t byte = (int64_t)op[1];
            if (byte == 0x50LL || byte == 0x51LL || byte == 0x52LL) {
                int t = p + (int)(int64_t)op[2] + 1;
                if (t >= 0 && t <= len) labels[t] = 1;
            }
        }
    }
    for (int p = 0; p < len; p++) {
        if (labels[p]) fprintf(emit_out, "L_%d:\n", p);
        emit_instruction(bcs[1 + p], p);
    }
    if (labels[len]) fprintf(emit_out, "L_%d:\n;\n", len);
    free(labels);
}

static void emit_program(wval main_bc, wval defs_list_val, FILE *out) {
    emit_out = out;
    quote_total = 0;
    qn = 0; qhead = 0;

    // Pass A: number quotes across defs + main (traversal order fixpoint)
    wval *defs = (wval *)defs_list_val;
    int dn = w_list_len(defs);
    for (int i = 0; i < dn; i++) {
        wval *pair = (wval *)defs[1 + i];
        count_quotes(pair[2]);
    }
    count_quotes(main_bc);

    fputs("// Generated by the Whisper seed compiler\n", out);
    fputs("#include \"runtime.c\"\n\n", out);

    // Forward declarations
    for (int i = 0; i < dn; i++) {
        wval *pair = (wval *)defs[1 + i];
        fputs("static void W_", out);
        emit_safe_name((const char *)pair[1]);
        fputs("(void);\n", out);
    }
    for (int i = 0; i < quote_total; i++)
        fprintf(out, "static void W_q%d(void);\n", i);
    fputc('\n', out);

    // Word definition functions
    for (int i = 0; i < dn; i++) {
        wval *pair = (wval *)defs[1 + i];
        fputs("static void W_", out);
        emit_safe_name((const char *)pair[1]);
        fputs("(void) {\n", out);
        emit_body(pair[2]);
        fputs("}\n\n", out);
    }

    // Main
    fputs("int main(int argc, char **argv) {\n", out);
    fputs("    w_init(argc, argv);\n", out);
    quote_counter = 0;
    emit_body(main_bc);
    fputs("    return 0;\n}\n\n", out);

    // Quote functions (draining the queue; bodies may enqueue more)
    while (qhead < qn) {
        QItem it = qitems[qhead++];
        fprintf(out, "static void W_q%d(void) {\n", it.id);
        emit_body(it.bc);
        fputs("}\n\n", out);
    }
}

// ══════════════════════════════════════════════════════════════════════
// Driver
// ══════════════════════════════════════════════════════════════════════

int main(int argc, char **argv) {
    int emit_mode = (argc > 1 && strcmp(argv[1], "-c") == 0);
    w_init(argc, argv);

    char *source = w_read_all();
    wval tokens = c_tokenize(source);
    wval structured = c_structify(tokens);
    wval classified = c_classify_nested(structured);
    wval split = c_split_defs(classified);
    wval *sp = (wval *)split;
    wval main_tokens = sp[1];
    wval defs_list = sp[2];

    // Compile each def body (in place: pair[2] tokens → bytecode)
    wval *defs = (wval *)defs_list;
    int dn = w_list_len(defs);
    for (int i = 0; i < dn; i++) {
        wval *pair = (wval *)defs[1 + i];
        const char *name = (const char *)pair[1];
        wval body_bc = c_compile_tokens(pair[2]);
        pair[2] = body_bc;
        register_word(name, body_bc);
    }

    wval main_bc = c_compile_tokens(main_tokens);

    if (emit_mode) {
        emit_program(main_bc, defs_list, stdout);
    } else {
        c_execute_bytecodes(main_bc);
    }
    return 0;
}
