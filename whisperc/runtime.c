// Whisper C Runtime — minimal runtime for transpiled Whisper programs.
// Compile: gcc -O2 -o program runtime.c program.c
//
// Memory model:
//   Stack: fixed-size array, grows upward (sp index)
//   Heap:  bump allocator from a large mmap'd region
//   Strings: [len:4B][data...][null] — len is stored just before data pointer
//   Lists:   [count:8B][elem0:8B][elem1:8B]...

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

#ifdef _WIN32
// winsock2.h must precede windows.h
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <process.h>
#else
#include <sys/mman.h>
#endif

// ── Types ───────────────────────────────────────────────────────────

typedef int64_t  wi64;
typedef double   wf64;
typedef uint64_t wval;  // all values are 64-bit (i64, f64, pointer, bool)

// ── Stack ───────────────────────────────────────────────────────────

#define STACK_SIZE 65536
// Thread-local interpreter state: worker threads (serve-concurrent)
// each get a fresh stack; the heap stays per-thread too so arena
// pointers never cross threads.
#define W_TLS __thread
static W_TLS wval *w_stack = NULL;   // allocated per thread
static W_TLS int  w_sp = 0;
static volatile int _dbg_max_sp = 0;
static volatile int _dbg_call_depth = 0;

// Function trace ring buffer
#define TRACE_BUF 64
static const char *_fn_trace[TRACE_BUF];
static W_TLS int _fn_trace_i = 0;
static void _trace_fn(const char *fn) { _fn_trace[_fn_trace_i & (TRACE_BUF-1)] = fn; _fn_trace_i++; }

static inline void w_push(wval v) {
    if (w_sp >= STACK_SIZE) { fprintf(stderr, "w_push: stack overflow sp=%d\n", w_sp); exit(1); }
    w_stack[w_sp++] = v;
}
static wval w_pop(void) {
    if (w_sp <= 0) {
        fprintf(stderr, "w_pop: stack underflow! sp=%d\n", w_sp);
        fprintf(stderr, "Recent Whisper calls:\n");
        for (int i = 0; i < TRACE_BUF; i++) {
            int idx = (_fn_trace_i - TRACE_BUF + i) & (TRACE_BUF-1);
            if (_fn_trace[idx]) fprintf(stderr, "  %s\n", _fn_trace[idx]);
        }
        exit(1);
    }
    return w_stack[--w_sp];
}
static inline wval w_peek(void) {
    if (w_sp <= 0) { fprintf(stderr, "w_peek: stack empty!\n"); exit(1); }
    return w_stack[w_sp - 1];
}
static inline void w_dup(void)          { w_push(w_peek()); }
static inline void w_swap(void)         { wval a=w_pop(), b=w_pop(); w_push(a); w_push(b); }
static inline void w_drop(void)         { w_sp--; }
static inline void w_rot(void)          { wval c=w_pop(),b=w_pop(),a=w_pop(); w_push(b); w_push(c); w_push(a); }
// Pick: w_pick_n(N) for $N operator, w_pick() reads index from stack
static inline void w_pick_n(int n) { w_push(w_stack[w_sp - 1 - n]); }
static void w_pick(void) { int64_t n = (int64_t)w_pop(); w_push(w_stack[w_sp - 1 - (int)n]); }

// ── Heap allocator ──────────────────────────────────────────────────
// Split-arena bump allocator: strings grow up from the heap base,
// lists grow down from the heap end. A value's type is then decided
// by address range alone — no layout heuristics, no misclassification
// of short strings as lists.

#define HEAP_SIZE (4096LL * 1024 * 1024)  // 4 GB total, 2 GB per arena
static W_TLS char *w_heap = NULL;
static W_TLS size_t w_heap_pos = 0;       // string arena bump (up)
static W_TLS size_t w_heap_list_pos = 0;  // list arena bump (down)
// Worker threads use a smaller arena than the 4GB main heap; the
// split layout (strings up / lists down) works at any size.
static W_TLS long long w_heap_size = 0;

static void w_heap_init(void) {
#ifdef _WIN32
    if (w_heap_size == 0) w_heap_size = HEAP_SIZE;
    w_heap = (char *)VirtualAlloc(NULL, (SIZE_T)w_heap_size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if (w_heap == NULL) { fprintf(stderr, "VirtualAlloc failed for %lld bytes\n", w_heap_size); exit(1); }
#else
    if (w_heap_size == 0) w_heap_size = HEAP_SIZE;
    w_heap = mmap(NULL, (size_t)w_heap_size, PROT_READ|PROT_WRITE,
                  MAP_PRIVATE|MAP_ANONYMOUS, -1, 0);
    if (w_heap == MAP_FAILED) { perror("mmap"); exit(1); }
#endif
}

static void *w_alloc(size_t bytes) {
    // Align to 8 bytes
    bytes = (bytes + 7) & ~7;
    if (w_heap_pos + bytes > (size_t)(w_heap_size / 2)) {
        fprintf(stderr, "whisper: string heap exhausted (pos=%zu, need=%zu, limit=%zu)\n",
                w_heap_pos, bytes, (size_t)(w_heap_size / 2));
        exit(1);
    }
    void *p = w_heap + w_heap_pos;
    w_heap_pos += bytes;
    return p;
}

static void *w_alloc_list(size_t bytes) {
    bytes = (bytes + 7) & ~7;
    if (w_heap_list_pos + bytes > (size_t)(w_heap_size / 2)) {
        fprintf(stderr, "whisper: list heap exhausted (pos=%zu, need=%zu, limit=%zu)\n",
                w_heap_list_pos, bytes, (size_t)(w_heap_size / 2));
        exit(1);
    }
    w_heap_list_pos += bytes;
    return w_heap + w_heap_size - w_heap_list_pos;
}

// ── String operations ───────────────────────────────────────────────
// String layout: [len:4B][char data...][null terminator]
// The pointer points to the char data (past the 4-byte length prefix).

static inline uint32_t w_str_len(const char *s) {
    if (!s) {
        fprintf(stderr, "w_str_len: NULL, sp=%d\n", w_sp);
        for (int i = (w_sp > 8 ? w_sp - 8 : 0); i < w_sp; i++) {
            wval v = w_stack[i];
            fprintf(stderr, "  [%d] = %lld (%p)\n", i, (long long)v, (void*)v);
        }
        exit(1);
    }
    return *(uint32_t *)(s - 4);
}

static char *w_str_new(const char *data, uint32_t len) {
    char *p = w_alloc(4 + len + 1);
    *(uint32_t *)p = len;
    char *s = p + 4;
    if (data) memcpy(s, data, len);
    s[len] = 0;
    return s;
}

static char *w_str_from_cstr(const char *cstr) {
    return w_str_new(cstr, strlen(cstr));
}

static char *w_str_concat(const char *a, const char *b) {
    uint32_t la = w_str_len(a), lb = w_str_len(b);
    char *p = w_str_new(NULL, la + lb);
    memcpy(p, a, la);
    memcpy(p + la, b, lb);
    return p;
}

static char *w_str_slice(const char *s, int64_t start, int64_t len) {
    uint32_t slen = w_str_len(s);
    if (start < 0) start = 0;
    if (len < 0) len = 0;
    if ((uint32_t)start > slen) start = slen;
    if ((uint32_t)(start + len) > slen) len = slen - start;
    return w_str_new(s + start, (uint32_t)len);
}

static int64_t w_str_eq(const char *a, const char *b) {
    uint32_t la = w_str_len(a), lb = w_str_len(b);
    if (la != lb) return 0;
    return memcmp(a, b, la) == 0;
}

static int64_t w_str_find(const char *haystack, const char *needle) {
    uint32_t hlen = w_str_len(haystack), nlen = w_str_len(needle);
    if (nlen == 0) return 0;
    if (nlen > hlen) return -1;
    for (uint32_t i = 0; i <= hlen - nlen; i++) {
        if (memcmp(haystack + i, needle, nlen) == 0) return (int64_t)i;
    }
    return -1;
}

static char *w_str_replace(const char *src, const char *old, const char *new) {
    uint32_t slen = w_str_len(src), olen = w_str_len(old), nlen = w_str_len(new);
    // Count occurrences
    int count = 0;
    for (uint32_t i = 0; i <= slen - olen; ) {
        if (memcmp(src + i, old, olen) == 0) { count++; i += olen; }
        else i++;
    }
    if (count == 0) return w_str_new(src, slen);
    uint32_t rlen = slen + count * (nlen - olen);
    char *result = w_alloc(4 + rlen + 1);
    *(uint32_t *)result = rlen;
    char *r = result + 4;
    uint32_t ri = 0;
    for (uint32_t i = 0; i < slen; ) {
        if (i <= slen - olen && memcmp(src + i, old, olen) == 0) {
            memcpy(r + ri, new, nlen);
            ri += nlen;
            i += olen;
        } else {
            r[ri++] = src[i++];
        }
    }
    r[rlen] = 0;
    return result + 4;
}

static char *w_i64_to_str(int64_t n) {
    char buf[32];
    int len = snprintf(buf, sizeof(buf), "%lld", (long long)n);
    return w_str_new(buf, len);
}

static int64_t w_str_to_i64(const char *s) {
    return strtoll(s, NULL, 10);
}

static char *rt_ctos(int64_t c) {
    char ch = (char)c;
    return w_str_new(&ch, 1);
}

static int64_t w_str_lt(const char *a, const char *b) {
    uint32_t la = w_str_len(a), lb = w_str_len(b);
    uint32_t minlen = la < lb ? la : lb;
    int cmp = memcmp(a, b, minlen);
    if (cmp < 0) return 1;
    if (cmp > 0) return 0;
    return la < lb;
}

static int64_t w_str_nth(const char *s, int64_t idx) {
    uint32_t len = w_str_len(s);
    if (idx < 0 || (uint32_t)idx >= len) return -1;
    return (int64_t)(unsigned char)s[idx];
}

// ── List operations ─────────────────────────────────────────────────
// List layout: [count:8B][elem0:8B][elem1:8B]...
// Pointer points to the count field.

// Address-range type test: top half of the heap is the list arena.
static int w_is_list_val(wval v) {
    if (v < (wval)w_heap || v >= (wval)(w_heap + w_heap_size)) return 0;
    return v >= (wval)(w_heap + w_heap_size / 2);
}

static inline wval *w_list_new(int64_t count) {
    wval *p = (wval *)w_alloc_list(8 * ((size_t)count + 1));
    p[0] = (wval)count;
    return p;
}

static inline int64_t w_list_len(wval *list) {
    if (!list) {
        fprintf(stderr, "w_list_len: NULL, sp=%d\n", w_sp);
        for (int i = (w_sp > 6 ? w_sp - 6 : 0); i < w_sp; i++)
            fprintf(stderr, "  [%d] = %p (%lld)\n", i, (void*)w_stack[i], (long long)w_stack[i]);
        exit(1);
    }
    return (int64_t)list[0];
}

static inline wval w_list_nth(wval *list, int64_t n) {
    return list[1 + n];
}

static wval *w_list_append(wval *list, wval elem) {
    int64_t count = w_list_len(list);
    wval *new_list = w_list_new(count + 1);
    memcpy(new_list + 1, list + 1, count * 8);
    new_list[1 + count] = elem;
    return new_list;
}

static wval *w_list_prepend(wval elem, wval *list) {
    int64_t count = w_list_len(list);
    wval *new_list = w_list_new(count + 1);
    new_list[1] = elem;
    memcpy(new_list + 2, list + 1, count * 8);
    return new_list;
}

static wval *w_list_slice(wval *list, int64_t start, int64_t len) {
    int64_t count = w_list_len(list);
    if (start < 0) start = 0;
    if (len < 0) len = 0;
    if (start > count) start = count;
    if (start + len > count) len = count - start;
    wval *new_list = w_list_new(len);
    memcpy(new_list + 1, list + 1 + start, len * 8);
    return new_list;
}

static wval *w_list_sort(wval *list) {
    int64_t count = w_list_len(list);
    wval *new_list = w_list_new(count);
    memcpy(new_list + 1, list + 1, count * 8);
    // Simple insertion sort for i64 values
    for (int64_t i = 1; i < count; i++) {
        wval key = new_list[1 + i];
        int64_t j = i - 1;
        while (j >= 0 && (int64_t)new_list[1 + j] > (int64_t)key) {
            new_list[1 + j + 1] = new_list[1 + j];
            j--;
        }
        new_list[1 + j + 1] = key;
    }
    return new_list;
}

static wval *w_list_rev(wval *list) {
    int64_t count = w_list_len(list);
    wval *new_list = w_list_new(count);
    for (int64_t i = 0; i < count; i++) {
        new_list[1 + i] = list[1 + count - 1 - i];
    }
    return new_list;
}

static wval *w_list_take(wval *list, int64_t n) {
    int64_t count = w_list_len(list);
    if (n > count) n = count;
    if (n < 0) n = 0;
    wval *new_list = w_list_new(n);
    memcpy(new_list + 1, list + 1, n * 8);
    return new_list;
}

static wval *w_list_drop(wval *list, int64_t n) {
    int64_t count = w_list_len(list);
    if (n > count) n = count;
    if (n < 0) n = 0;
    int64_t new_count = count - n;
    wval *new_list = w_list_new(new_count);
    memcpy(new_list + 1, list + 1 + n, new_count * 8);
    return new_list;
}

// ── Mutable list buffers — O(1) append for compiler passes ──────────
// Layout: [cap:8B][count:8B][elems...] — the value points at count, so
// every read-side list operation (len/nth/map/print) works unchanged.
// buf-push returns a possibly-new buffer (grown by 2x); callers must
// use the returned value. Regular append() on a buffer yields a plain
// immutable list (capacity slot dropped).

static wval *w_buf_new(int64_t cap0) {
    if (cap0 < 8) cap0 = 8;
    wval *p = (wval *)w_alloc_list(8 * ((size_t)cap0 + 2));
    p[0] = (wval)cap0;
    p[1] = (wval)0;
    return p + 1;
}

static wval *w_buf_push(wval *buf, wval elem) {
    wval *meta = buf - 1;
    int64_t cap = (int64_t)meta[0];
    int64_t count = (int64_t)buf[0];
    if (count >= cap) {
        int64_t ncap = cap * 2;
        wval *np = (wval *)w_alloc_list(8 * ((size_t)ncap + 2));
        np[0] = (wval)ncap;
        np[1] = (wval)count;
        memcpy(np + 2, buf + 1, (size_t)count * 8);
        buf = np + 1;
    }
    buf[1 + count] = elem;
    buf[0] = (wval)(count + 1);
    return buf;
}

// ── Type predicates (heap pointer heuristic) ───────────────────────
static int wval_is_heap(wval v) {
    return (v >= (wval)w_heap && v < (wval)(w_heap + w_heap_size));
}
static int wval_is_str(wval v) {
    if (!wval_is_heap(v)) return 0;
    return (v < (wval)(w_heap + w_heap_size / 2));
}
static int wval_is_list(wval v) {
    if (!wval_is_heap(v)) return 0;
    return (v >= (wval)(w_heap + w_heap_size / 2));
}

// Tag check for structured tokens: tag 5 = braces {}, tag 6 = brackets []
static int is_tag(wval v, int64_t tag) {
    if (!wval_is_heap(v)) return 0;
    wval *p = (wval *)v;
    return w_list_len(p) == 2 && (int64_t)p[1] == tag;
}

// ── split_cond: parse ?? | ] conditional structure ────────────────
// tokens[start..] → split into then/else branches at depth 1
// out_next = index past the closing ]
static void split_cond(wval *tokens, int start, wval **out_then,
                       wval **out_else, int *out_next) {
    wval *th = w_buf_new(8), *el = w_buf_new(64);
    int depth = 1, in_else = 0, i = start;
    int tlen = w_list_len(tokens);
    while (i < tlen && depth > 0) {
        wval tok = tokens[1 + i];
        if (wval_is_str(tok)) {
            const char *s = (const char *)tok;
            if (strcmp(s, "??") == 0) {
                depth++;
                if (!in_else) th = w_buf_push(th, tok);
                else el = w_buf_push(el, tok);
            } else if (strcmp(s, "]") == 0) {
                depth--;
                if (depth == 0) break;
                if (!in_else) th = w_buf_push(th, tok);
                else el = w_buf_push(el, tok);
            } else if (strcmp(s, "|") == 0 && depth == 1) {
                in_else = 1;
            } else {
                if (!in_else) th = w_buf_push(th, tok);
                else el = w_buf_push(el, tok);
            }
        } else {
            if (!in_else) th = w_buf_push(th, tok);
            else el = w_buf_push(el, tok);
        }
        i++;
    }
    *out_then = th;
    *out_else = el;
    *out_next = i + 1;
}

static wval *w_list_dedup(wval *list) {
    int64_t count = w_list_len(list);
    wval *result = w_list_new(0);
    for (int64_t i = 0; i < count; i++) {
        int found = 0;
        for (int64_t j = 0; j < w_list_len(result); j++) {
            if (result[1 + j] == list[1 + i]) { found = 1; break; }
        }
        if (!found) result = w_list_append(result, list[1 + i]);
    }
    return result;
}

static wval *w_str_split(const char *s, const char *delim) {
    wval *result = w_list_new(0);
    uint32_t slen = w_str_len(s), dlen = w_str_len(delim);
    if (dlen == 0) {
        for (uint32_t i = 0; i < slen; i++)
            result = w_list_append(result, (wval)w_str_new(s + i, 1));
    } else {
        uint32_t start = 0;
        for (uint32_t i = 0; i + dlen <= slen; i++) {
            if (memcmp(s + i, delim, dlen) == 0) {
                result = w_list_append(result, (wval)w_str_new(s + start, i - start));
                start = i + dlen;
                i += dlen - 1;
            }
        }
        result = w_list_append(result, (wval)w_str_new(s + start, slen - start));
    }
    return result;
}

static wval *w_strchars(const char *s) {
    uint32_t len = w_str_len(s);
    wval *list = w_list_new(len);
    for (uint32_t i = 0; i < len; i++) list[1 + i] = (wval)(int64_t)(unsigned char)s[i];
    return list;
}

static char *w_charsstr(wval *list) {
    int64_t count = w_list_len(list);
    char *buf = (char *)w_alloc(count + 1);
    for (int64_t i = 0; i < count; i++) buf[i] = (char)(int64_t)list[1 + i];
    return w_str_new(buf, count);
}

// StrIter: pushes (first-char, rest-string); on empty pushes (-1, "")
static void w_striter(const char *s) {
    if (w_str_len(s) == 0) {
        w_push((wval)-1LL);
        w_push((wval)w_str_new("", 0));
    } else {
        w_push((wval)(int64_t)(unsigned char)s[0]);
        w_push((wval)w_str_new(s + 1, w_str_len(s) - 1));
    }
}

static char *w_strjoin(wval *list) {
    int64_t count = w_list_len(list);
    size_t total = 0;
    for (int64_t i = 0; i < count; i++) total += w_str_len((const char *)list[1 + i]);
    char *buf = (char *)w_alloc(total + 1);
    size_t pos = 0;
    for (int64_t i = 0; i < count; i++) {
        const char *s = (const char *)list[1 + i];
        uint32_t l = w_str_len(s);
        memcpy(buf + pos, s, l);
        pos += l;
    }
    return w_str_new(buf, total);
}

static int64_t w_listfind(wval *list, wval elem) {
    int64_t count = w_list_len(list);
    int elem_is_str = w_is_list_val(elem) == 0 && elem >= (wval)w_heap && elem < (wval)(w_heap + w_heap_size);
    for (int64_t i = 0; i < count; i++) {
        wval v = list[1 + i];
        if (v == elem) return i;
        if (elem_is_str && v >= (wval)w_heap && v < (wval)(w_heap + w_heap_size) &&
            w_str_eq((const char *)v, (const char *)elem)) return i;
    }
    return -1;
}

// ── IO operations ───────────────────────────────────────────────────

static void w_print_i64(int64_t n) { printf("%lld\n", (long long)n); }
static void w_print_f64(double f) { printf("%g\n", f); }
static void w_print_bool(int64_t b) { printf("%s\n", b ? "#t" : "#f"); }
static void w_print_str(const char *s) {
    uint32_t len = w_str_len(s);
    fwrite(s, 1, len, stdout);
    putchar('\n');
}

static void w_dump_val(wval v);

static void w_print_val(wval v) {
    w_dump_val(v);
    putchar('\n');
}

static void w_dump_val(wval v) {
    if (v >= (wval)w_heap && v < (wval)(w_heap + w_heap_size)) {
        if (w_is_list_val(v)) {
            wval *l = (wval *)v;
            int64_t count = (int64_t)l[0];
            if (count < 10000000) {
                printf("[");
                for (int64_t i = 0; i < count; i++) {
                    if (i > 0) printf(" ");
                    w_dump_val(l[1 + i]);
                }
                printf("]");
                return;
            }
        } else {
            const char *s = (const char *)v;
            uint32_t len = w_str_len(s);
            if (len < 10000000) {
                fwrite(s, 1, len, stdout);
                return;
            }
        }
    }
    // Default: print as i64
    printf("%lld", (long long)(int64_t)v);
}

static char *w_read_line(void) {
    char buf[4096];
    if (!fgets(buf, sizeof(buf), stdin)) return w_str_new("", 0);
    size_t len = strlen(buf);
    if (len > 0 && buf[len-1] == '\n') len--;
    if (len > 0 && buf[len-1] == '\r') len--;
    return w_str_new(buf, len);
}

// OutputAll: print the whole stack as [v0 v1 ...]
static void w_dump_val(wval v);

static void w_output_all(void) {
    printf("[");
    for (int i = 0; i < w_sp; i++) {
        if (i > 0) printf(" ");
        w_dump_val(w_stack[i]);
    }
    printf("]\n");
}

// ── JSON (simplified) ───────────────────────────────────────────────

static char *w_json_stringify(wval v) {
    // For i64 values
    return w_i64_to_str((int64_t)v);
}

// ── Map operations (association list) ───────────────────────────────

static wval *w_map_new(void) {
    return w_list_new(0);
}

static wval w_map_get(wval *map, const char *key) {
    int64_t count = w_list_len(map);
    for (int64_t i = 0; i < count; i++) {
        wval *pair = (wval *)map[1 + i];
        if (pair && w_list_len(pair) == 2) {
            const char *k = (const char *)pair[1];
            if (w_str_eq(k, key)) return pair[2];
        }
    }
    return 0;
}

static int64_t w_map_has(wval *map, const char *key) {
    int64_t count = w_list_len(map);
    for (int64_t i = 0; i < count; i++) {
        wval *pair = (wval *)map[1 + i];
        if (pair && w_list_len(pair) == 2) {
            const char *k = (const char *)pair[1];
            if (w_str_eq(k, key)) return 1;
        }
    }
    return 0;
}

static wval *w_map_set(wval *map, const char *key, wval value) {
    int64_t count = w_list_len(map);
    // Check if key exists
    for (int64_t i = 0; i < count; i++) {
        wval *pair = (wval *)map[1 + i];
        if (pair && w_list_len(pair) == 2) {
            const char *k = (const char *)pair[1];
            if (w_str_eq(k, key)) {
                // Update in place (mutation for simplicity)
                pair[2] = value;
                return map;
            }
        }
    }
    // Add new entry
    wval *pair = w_list_new(2);
    pair[1] = (wval)key;
    pair[2] = value;
    return w_list_append(map, (wval)pair);
}

static wval *w_map_del(wval *map, const char *key) {
    int64_t count = w_list_len(map);
    wval *new_map = w_list_new(0);
    for (int64_t i = 0; i < count; i++) {
        wval *pair = (wval *)map[1 + i];
        if (pair && w_list_len(pair) == 2) {
            const char *k = (const char *)pair[1];
            if (!w_str_eq(k, key)) {
                new_map = w_list_append(new_map, (wval)pair);
            }
        }
    }
    return new_map;
}

static wval *w_map_keys(wval *map) {
    int64_t count = w_list_len(map);
    wval *keys = w_list_new(count);
    for (int64_t i = 0; i < count; i++) {
        wval *pair = (wval *)map[1 + i];
        keys[1 + i] = pair ? pair[1] : 0;
    }
    return keys;
}

// ── Math ────────────────────────────────────────────────────────────

#include <math.h>

static double w_fsqrt(double x) { return sqrt(x); }
static double w_fsin(double x)  { return sin(x); }
static double w_fcos(double x)  { return cos(x); }
static double w_ftan(double x)  { return tan(x); }
static double w_flog(double x)  { return log(x); }
static double w_fexp(double x)  { return exp(x); }
static double w_fatan2(double y, double x) { return atan2(y, x); }

// ── Random ──────────────────────────────────────────────────────────

static W_TLS uint64_t w_rng_state = 0;

static void w_random_seed(uint64_t seed) {
    w_rng_state = seed ? seed : 0xDEADBEEF;
}

static double w_random(void) {
    w_rng_state ^= w_rng_state >> 12;
    w_rng_state ^= w_rng_state << 25;
    w_rng_state ^= w_rng_state >> 27;
    uint64_t bits = w_rng_state * 0x2545F4914F6CDD1DULL >> 11;
    return (double)bits / (double)(1ULL << 53);
}

static int64_t w_randint(int64_t min, int64_t max) {
    if (min >= max) return min;
    return min + (int64_t)(w_random() * (double)(max - min));
}

// ── Time ────────────────────────────────────────────────────────────

#include <time.h>
#ifndef _WIN32
#include <unistd.h>
#endif

static int64_t w_now(void) {
    return (int64_t)time(NULL);
}

static void w_sleep(int64_t ms) {
#ifdef _WIN32
    Sleep((DWORD)ms);
#else
    usleep((useconds_t)(ms * 1000));
#endif
}

// ── Bitwise ─────────────────────────────────────────────────────────

static inline int64_t w_shl(int64_t a, int64_t b) { return a << b; }
static inline int64_t w_shr(int64_t a, int64_t b) { return a >> b; }
static inline int64_t w_bitand(int64_t a, int64_t b) { return a & b; }
static inline int64_t w_bitor(int64_t a, int64_t b) { return a | b; }
static inline int64_t w_bitxor(int64_t a, int64_t b) { return a ^ b; }
static inline int64_t w_bitnot(int64_t a) { return ~a; }

// ── Program args ────────────────────────────────────────────────────

static int w_argc = 0;
static char **w_argv = NULL;

static wval *w_args(void) {
    wval *list = w_list_new(w_argc);
    for (int i = 0; i < w_argc; i++) {
        list[1 + i] = (wval)w_str_from_cstr(w_argv[i]);
    }
    return list;
}

// ── Initialization ──────────────────────────────────────────────────

// ── Type check functions (for Whisper list?, i64?, str?, bool?) ─────
static void w_list_(void) {
    wval v = w_pop();
    w_push((wval)(w_is_list_val(v) ? 1L : 0L));
}
static void w_i64_(void) {
    wval v = w_pop();
    w_push((wval)(v < (wval)w_heap || v >= (wval)(w_heap + w_heap_size) ? 1L : 0L));
}
static void w_str_(void) {
    wval v = w_pop();
    w_push((wval)((v >= (wval)w_heap && v < (wval)(w_heap + w_heap_size) && !w_is_list_val(v)) ? 1L : 0L));
}
static void w_bool_(void) {
    wval v = w_pop();
    w_push((wval)((v == 0 || v == 1) ? 1L : 0L));
}

// ── Quote (closure) support ─────────────────────────────────────────
// A quote is encoded as the raw C function pointer of the generated
// w_qN() body. Higher-order words call back through it on the global
// stack, exactly like the bytecode interpreter re-entering a PushRef.

typedef void (*wfn)(void);

static wval *w_map_over(wval *list, wfn f) {
    int64_t count = w_list_len(list);
    wval *result = w_list_new(count);
    for (int64_t i = 0; i < count; i++) {
        w_push(list[1 + i]);
        f();
        result[1 + i] = w_pop();
    }
    return result;
}

static void w_each_over(wval *list, wfn f) {
    int64_t count = w_list_len(list);
    for (int64_t i = 0; i < count; i++) {
        int sp_before = w_sp;
        w_push(list[1 + i]);
        f();
        // the quote may leave any number of results; @each discards all
        if (w_sp > sp_before) w_sp = sp_before;
    }
}

static wval w_fold_over(wval *list, wval init, wfn f) {
    int64_t count = w_list_len(list);
    wval acc = init;
    for (int64_t i = 0; i < count; i++) {
        w_push(acc);
        w_push(list[1 + i]);
        f();
        acc = w_pop();
    }
    return acc;
}

static wval *w_filter_over(wval *list, wfn f) {
    int64_t count = w_list_len(list);
    wval *result = w_list_new(0);
    for (int64_t i = 0; i < count; i++) {
        w_push(list[1 + i]);
        f();
        if ((int64_t)w_pop() != 0) result = w_list_append(result, list[1 + i]);
    }
    return result;
}

// ── Full-input read (multi-line source) ─────────────────────────────

static char *w_read_all(void) {
    size_t cap = 1 << 16, len = 0;
    char *buf = (char *)malloc(cap);
    if (!buf) { fprintf(stderr, "whisper: out of memory\n"); exit(1); }
    size_t n;
    while ((n = fread(buf + len, 1, cap - len - 1, stdin)) > 0) {
        len += n;
        if (cap - len < 2) {
            cap *= 2;
            char *nb = (char *)realloc(buf, cap);
            if (!nb) { fprintf(stderr, "whisper: out of memory\n"); exit(1); }
            buf = nb;
        }
    }
    buf[len] = 0;
    // Strip CR so CRLF sources behave identically to LF sources
    size_t w = 0;
    for (size_t i = 0; i < len; i++) {
        if (buf[i] != '\r') buf[w++] = buf[i];
    }
    char *s = w_str_new(buf, (uint32_t)w);
    free(buf);
    return s;
}

#ifdef _WIN32
static LONG WINAPI _seh_handler(EXCEPTION_POINTERS *ep) {
    if (ep->ExceptionRecord->ExceptionCode == EXCEPTION_ACCESS_VIOLATION) {
        fprintf(stderr, "\nSEGV at address %p, sp=%d, heap_str=%zu/%zu, heap_list=%zu/%zu\n",
                (void*)ep->ExceptionRecord->ExceptionInformation[1],
                w_sp, w_heap_pos, (size_t)(HEAP_SIZE/2),
                w_heap_list_pos, (size_t)(HEAP_SIZE/2));
        fprintf(stderr, "Instruction at %p\n", ep->ContextRecord->Rip);
        _exit(139);
    }
    return EXCEPTION_CONTINUE_SEARCH;
}
#endif


// ── Native IO: sockets, files, environment (v1.0.1 vocabulary) ──────
#ifdef _WIN32
typedef SOCKET w_socket_t;
#define W_BAD_SOCKET INVALID_SOCKET
#define w_closesock closesocket
#else
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <unistd.h>
typedef int w_socket_t;
#define W_BAD_SOCKET (-1)
#define w_closesock close
#endif

static void w_net_init(void) {
#ifdef _WIN32
    WSADATA d;
    WSAStartup(MAKEWORD(2, 2), &d);
#endif
}

// listen(port) -> server handle | -1
static wval w_sock_listen(int64_t port) {
    w_socket_t s = socket(AF_INET, SOCK_STREAM, 0);
    if (s == W_BAD_SOCKET) return -1;
    int opt = 1;
    setsockopt(s, SOL_SOCKET, SO_REUSEADDR, (const char *)&opt, sizeof(opt));
    struct sockaddr_in a;
    memset(&a, 0, sizeof(a));
    a.sin_family = AF_INET;
    a.sin_addr.s_addr = htonl(INADDR_ANY);
    a.sin_port = htons((unsigned short)port);
    if (bind(s, (struct sockaddr *)&a, sizeof(a)) != 0) { w_closesock(s); return -1; }
    if (listen(s, 16) != 0) { w_closesock(s); return -1; }
    return (wval)(int64_t)s;
}

// accept(server) -> conn handle | -1 (blocking)
static wval w_sock_accept(wval srv) {
    w_socket_t c = accept((w_socket_t)(int64_t)srv, NULL, NULL);
    if (c == W_BAD_SOCKET) return -1;
    return (wval)(int64_t)c;
}

// recv(conn) -> request string. HTTP-style: stop at header terminator, peer
// close, or 2s receive timeout. If the headers carry Content-Length, keep
// reading until the body is complete (1MB cap). Empty string on close/error.
static int w_hdr_content_length(const char *buf, size_t len) {
    const char *needle = "content-length:";
    size_t nlen = 15;
    for (size_t i = 0; i + nlen < len; i++) {
        size_t j = 0;
        while (j < nlen) {
            char a = buf[i + j];
            if (a >= 'A' && a <= 'Z') a = (char)(a + 32);
            if (a != needle[j]) break;
            j++;
        }
        if (j == nlen) {
            long v = strtol(buf + i + nlen, NULL, 10);
            if (v < 0) v = 0;
            if (v > 1048576) v = 1048576;
            return (int)v;
        }
    }
    return -1;
}

static wval w_sock_recv(wval h) {
    w_socket_t c = (w_socket_t)(int64_t)h;
#ifdef _WIN32
    DWORD ms = 2000;
    setsockopt(c, SOL_SOCKET, SO_RCVTIMEO, (const char *)&ms, sizeof(ms));
#else
    struct timeval tv = {2, 0};
    setsockopt(c, SOL_SOCKET, SO_RCVTIMEO, (const char *)&tv, sizeof(tv));
#endif
    size_t cap = 65536, len = 0;
    char *buf = (char *)w_alloc(cap);
    while (len < cap - 1) {
        if (len >= 4 && memcmp(buf + len - 4, "\r\n\r\n", 4) == 0) {
            int clen = w_hdr_content_length(buf, len);
            if (clen <= 0) break;
            size_t need = len + (size_t)clen + 1;
            if (need > cap) {
                cap = need;
                char *nb = (char *)w_alloc(cap);
                memcpy(nb, buf, len);
                buf = nb;
            }
            while (len < need - 1) {
                int n = (int)recv(c, buf + len, (int)(need - 1 - len), 0);
                if (n <= 0) break;
                len += (size_t)n;
            }
            break;
        }
        int n = (int)recv(c, buf + len, (int)(cap - 1 - len), 0);
        if (n <= 0) break;
        len += (size_t)n;
    }
    return (wval)w_str_new(buf, (uint32_t)len);
}

// send(conn, data) -> bytes sent | -1
static wval w_sock_send(wval h, const char *s) {
    w_socket_t c = (w_socket_t)(int64_t)h;
    int n = (int)send(c, s, w_str_len(s), 0);
    return (wval)(int64_t)n;
}

static void w_sock_close(wval h) {
    w_closesock((w_socket_t)(int64_t)h);
}

// read-file(path) -> contents | "" on error
static wval w_read_file(const char *path) {
    FILE *f = fopen(path, "rb");
    if (!f) return (wval)w_str_new("", 0);
    fseek(f, 0, SEEK_END);
    long sz = ftell(f);
    fseek(f, 0, SEEK_SET);
    if (sz < 0) { fclose(f); return (wval)w_str_new("", 0); }
    char *buf = (char *)w_alloc((size_t)sz + 1);
    size_t rd = fread(buf, 1, (size_t)sz, f);
    fclose(f);
    return (wval)w_str_new(buf, (uint32_t)rd);
}

// write-file(path, data) -> bytes written | -1
static wval w_write_file(const char *path, const char *data) {
    FILE *f = fopen(path, "wb");
    if (!f) return -1;
    size_t wr = fwrite(data, 1, w_str_len(data), f);
    fclose(f);
    return (wval)(int64_t)wr;
}

// getenv(name) -> value | "" if unset
static wval w_env_get(const char *name) {
    const char *v = getenv(name);
    if (!v) return (wval)w_str_new("", 0);
    return (wval)w_str_new(v, (uint32_t)strlen(v));
}

// ── Shared KV store (v1.0.2 vocabulary) ──────────────────────────────
// Process-global string→string map with a mutex; survives across the
// worker threads spawned by serve-concurrent (arena strings are
// per-thread, so values are copied into malloc'd storage).
#define W_KV_MAX 1024
static char *kv_keys[W_KV_MAX];
static char *kv_vals[W_KV_MAX];
static int kv_used = 0;
#ifdef _WIN32
static CRITICAL_SECTION w_kv_lock;
static int w_kv_lock_init = 0;
static void w_kv_lock_enter(void) {
    if (!w_kv_lock_init) { InitializeCriticalSection(&w_kv_lock); w_kv_lock_init = 1; }
    EnterCriticalSection(&w_kv_lock);
}
static void w_kv_lock_leave(void) { LeaveCriticalSection(&w_kv_lock); }
#else
#include <pthread.h>
static pthread_mutex_t w_kv_lock = PTHREAD_MUTEX_INITIALIZER;
static void w_kv_lock_enter(void) { pthread_mutex_lock(&w_kv_lock); }
static void w_kv_lock_leave(void) { pthread_mutex_unlock(&w_kv_lock); }
#endif

static char *kv_dup(const char *s) {
    // copy the runtime string layout (4-byte len prefix + data) so the
    // stored copy stays a valid runtime string; kv_free releases the
    // real malloc base (which sits 4 bytes before the data pointer).
    uint32_t n = w_str_len(s);
    uint32_t *p = (uint32_t *)malloc(4 + n);
    *p = n;
    memcpy(p + 1, s, n);
    return (char *)(p + 1);
}

static void kv_free(char *s) {
    free((uint32_t *)s - 1);
}

static void w_kv_set(const char *key, const char *val) {
    w_kv_lock_enter();
    for (int i = 0; i < kv_used; i++) {
        if (w_str_eq(kv_keys[i], key)) {
            kv_free(kv_vals[i]);
            kv_vals[i] = kv_dup(val);
            w_kv_lock_leave();
            return;
        }
    }
    if (kv_used < W_KV_MAX) {
        kv_keys[kv_used] = kv_dup(key);
        kv_vals[kv_used] = kv_dup(val);
        kv_used++;
    }
    w_kv_lock_leave();
}

static wval w_kv_get(const char *key) {
    w_kv_lock_enter();
    for (int i = 0; i < kv_used; i++) {
        if (w_str_eq(kv_keys[i], key)) {
            wval r = (wval)w_str_new(kv_vals[i], w_str_len(kv_vals[i]));
            w_kv_lock_leave();
            return r;
        }
    }
    w_kv_lock_leave();
    return (wval)w_str_new("", 0); // missing -> empty string (VM parity)
}

static void w_kv_del(const char *key) {
    w_kv_lock_enter();
    for (int i = 0; i < kv_used; i++) {
        if (w_str_eq(kv_keys[i], key)) {
            kv_free(kv_keys[i]);
            kv_free(kv_vals[i]);
            for (int j = i; j < kv_used - 1; j++) {
                kv_keys[j] = kv_keys[j + 1];
                kv_vals[j] = kv_vals[j + 1];
            }
            kv_used--;
            break;
        }
    }
    w_kv_lock_leave();
}

static wval *w_kv_keys_list(void) {
    w_kv_lock_enter();
    wval *l = w_list_new(kv_used);
    for (int i = 0; i < kv_used; i++)
        l[1 + i] = (wval)w_str_new(kv_keys[i], w_str_len(kv_keys[i]));
    w_kv_lock_leave();
    return l;
}

// ── Concurrent serve (v1.0.2) ────────────────────────────────────────
// serve-concurrent(port, handler): native accept loop; one thread per
// connection. Each worker initializes its own thread-local stack and a
// compact heap, pushes the conn handle, and calls the handler (quote
// bodies are plain C functions). Never returns.
#define W_THREAD_HEAP (64LL * 1024 * 1024)  // 64MB per worker

typedef struct { w_socket_t conn; wfn handler; } w_job;

static void w_worker_cleanup(void) {
    if (w_stack) { free(w_stack); w_stack = NULL; }
    w_sp = 0;
    if (w_heap) {
#ifdef _WIN32
        VirtualFree(w_heap, 0, MEM_RELEASE);
#else
        munmap(w_heap, (size_t)w_heap_size);
#endif
        w_heap = NULL;
    }
    w_heap_pos = 0;
    w_heap_list_pos = 0;
    w_heap_size = 0;
}

#ifdef _WIN32
static unsigned __stdcall w_worker_main(void *arg) {
#else
static void *w_worker_main(void *arg) {
#endif
    w_job *job = (w_job *)arg;
    w_stack = (wval *)malloc(sizeof(wval) * STACK_SIZE);
    if (w_stack == NULL) { free(job); return 0; }
    w_sp = 0;
    w_heap_size = W_THREAD_HEAP;
    w_heap_init();
    w_random_seed((uint64_t)(uintptr_t)job ^ (uint64_t)time(NULL));
    w_push((wval)(int64_t)job->conn);
    job->handler();
    w_sock_close(job->conn);
    w_worker_cleanup();
    free(job);
    return 0;
}

static void w_serve_concurrent(int64_t port, wfn handler) {
    wval srv = w_sock_listen(port);
    if ((int64_t)srv < 0) { fprintf(stderr, "serve-concurrent: listen failed\n"); exit(1); }
    for (;;) {
        w_socket_t c = accept((w_socket_t)(int64_t)srv, NULL, NULL);
        if (c == W_BAD_SOCKET) continue;
        w_job *job = (w_job *)malloc(sizeof(w_job));
        job->conn = c;
        job->handler = handler;
#ifdef _WIN32
        HANDLE t = (HANDLE)_beginthreadex(NULL, 0, w_worker_main, job, 0, NULL);
        if (t) CloseHandle(t);
#else
        pthread_t t;
        if (pthread_create(&t, NULL, w_worker_main, job) == 0) pthread_detach(t);
#endif
    }
}

static void w_init(int argc, char **argv) {
#ifdef _WIN32
    SetUnhandledExceptionFilter(_seh_handler);
#endif
    if (w_stack == NULL) {
        w_stack = (wval *)malloc(sizeof(wval) * STACK_SIZE);
        if (w_stack == NULL) { fprintf(stderr, "stack alloc failed\n"); exit(1); }
    }
    w_heap_init();
    w_net_init();
    w_argc = argc;
    w_argv = argv;
    w_random_seed((uint64_t)time(NULL) ^ 0x12345678);
}
