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
#include <sys/mman.h>

// ── Types ───────────────────────────────────────────────────────────

typedef int64_t  wi64;
typedef double   wf64;
typedef uint64_t wval;  // all values are 64-bit (i64, f64, pointer, bool)

// ── Stack ───────────────────────────────────────────────────────────

#define STACK_SIZE 65536
static wval w_stack[STACK_SIZE];
static int  w_sp = 0;

static inline void w_push(wval v)      { w_stack[w_sp++] = v; }
static inline wval w_pop(void)          { return w_stack[--w_sp]; }
static inline wval w_peek(void)         { return w_stack[w_sp - 1]; }
static inline void w_dup(void)          { w_push(w_peek()); }
static inline void w_swap(void)         { wval a=w_pop(), b=w_pop(); w_push(a); w_push(b); }
static inline void w_drop(void)         { w_sp--; }
static inline void w_rot(void)          { wval c=w_pop(),b=w_pop(),a=w_pop(); w_push(b); w_push(c); w_push(a); }
static inline void w_pick(int n)        { w_push(w_stack[w_sp - 1 - n]); }

// ── Heap allocator ──────────────────────────────────────────────────

#define HEAP_SIZE (16 * 1024 * 1024)  // 16 MB
static char *w_heap = NULL;
static size_t w_heap_pos = 0;

static void w_heap_init(void) {
    w_heap = mmap(NULL, HEAP_SIZE, PROT_READ|PROT_WRITE,
                  MAP_PRIVATE|MAP_ANONYMOUS, -1, 0);
    if (w_heap == MAP_FAILED) { perror("mmap"); exit(1); }
}

static void *w_alloc(size_t bytes) {
    // Align to 8 bytes
    bytes = (bytes + 7) & ~7;
    if (w_heap_pos + bytes > HEAP_SIZE) {
        fprintf(stderr, "whisper: heap exhausted\n");
        exit(1);
    }
    void *p = w_heap + w_heap_pos;
    w_heap_pos += bytes;
    return p;
}

// ── String operations ───────────────────────────────────────────────
// String layout: [len:4B][char data...][null terminator]
// The pointer points to the char data (past the 4-byte length prefix).

static inline uint32_t w_str_len(const char *s) {
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
    int len = snprintf(buf, sizeof(buf), "%ld", n);
    return w_str_new(buf, len);
}

static int64_t w_str_to_i64(const char *s) {
    return strtoll(s, NULL, 10);
}

static char *w_ctos(int64_t c) {
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

// ── List operations ─────────────────────────────────────────────────
// List layout: [count:8B][elem0:8B][elem1:8B]...
// Pointer points to the count field.

static inline wval *w_list_new(int64_t count) {
    wval *p = w_alloc(8 * (count + 1));
    p[0] = (wval)count;
    return p;
}

static inline int64_t w_list_len(wval *list) {
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

// ── IO operations ───────────────────────────────────────────────────

static void w_print_i64(int64_t n) { printf("%ld\n", n); }
static void w_print_f64(double f) { printf("%g\n", f); }
static void w_print_bool(int64_t b) { printf("%s\n", b ? "#t" : "#f"); }
static void w_print_str(const char *s) {
    uint32_t len = w_str_len(s);
    fwrite(s, 1, len, stdout);
    putchar('\n');
}

static void w_print_val(wval v) {
    // Heuristic: if it looks like a heap pointer, print as string or list
    if (v >= (wval)w_heap && v < (wval)(w_heap + HEAP_SIZE)) {
        // Could be a string or list pointer
        // Strings have a 4-byte length prefix, lists have 8-byte count
        // For now, try printing as string
        const char *s = (const char *)v;
        uint32_t len = w_str_len(s);
        if (len < 100000) {  // sanity check
            fwrite(s, 1, len, stdout);
            putchar('\n');
            return;
        }
    }
    // Default: print as i64
    printf("%ld\n", (int64_t)v);
}

static char *w_read_line(void) {
    char buf[4096];
    if (!fgets(buf, sizeof(buf), stdin)) return w_str_new("", 0);
    size_t len = strlen(buf);
    if (len > 0 && buf[len-1] == '\n') len--;
    return w_str_new(buf, len);
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

static uint64_t w_rng_state = 0;

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
#include <unistd.h>

static int64_t w_now(void) {
    return (int64_t)time(NULL);
}

static void w_sleep(int64_t ms) {
    usleep((useconds_t)(ms * 1000));
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

static void w_init(int argc, char **argv) {
    w_heap_init();
    w_argc = argc;
    w_argv = argv;
    w_random_seed((uint64_t)time(NULL) ^ 0x12345678);
}
