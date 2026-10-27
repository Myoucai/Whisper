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
#include <errno.h>
#include <math.h>
#ifdef __wasi__
#  define W_WASI 1
#endif
#ifndef W_WASI
#include <setjmp.h>
#endif

#ifdef _WIN32
// winsock2.h must precede windows.h
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <process.h>
#include <io.h>         // _setmode / _fileno, for `print`'s binary stdout
#include <fcntl.h>      // _O_BINARY
#else
#  ifndef W_WASI
#    include <sys/mman.h>
#    include <sys/time.h> // struct timeval for socket timeouts (musl needs this)
#    include <netdb.h>  // getaddrinfo / freeaddrinfo for the HTTP client
#    include <pthread.h> // @9 parallel fan-out
#  endif
#endif

// ── Types ───────────────────────────────────────────────────────────

typedef int64_t  wi64;
typedef double   wf64;
typedef struct { uint64_t v; uint8_t t; } wval;
enum { WT_I64 = 0, WT_F64 = 1, WT_PTR = 2, WT_SIG = 3, WT_BOOL = 4 };
// Signal (L-03): confidence-tagged wrapper, VM Value::Signal(inner, c).
// Transparent for type checks/arithmetic (VM unwraps); conf-of/unwrap
// are the only observers. malloc'd, never freed — conf literals are rare.
typedef struct { wval inner; double conf; } wsig;
static inline wval w_sig(wval inner, double conf) {
    wsig *s = (wsig *)malloc(sizeof(wsig)); s->inner = inner; s->conf = conf;
    wval r; r.v = (uint64_t)s; r.t = WT_SIG; return r;
}
static inline wval w_sig_inner(wval v) { return ((wsig *)v.v)->inner; }
static inline double w_conf(wval v) { return v.t == WT_SIG ? ((wsig *)v.v)->conf : 1.0; }

// Raw identity compare (bits + tag) — used for pointer/identity equality
// where C does not define == on structs.
static inline int w_eq_raw(wval a, wval b) { return a.v == b.v && a.t == b.t; }

// N-1/N-3 值相等：对齐 VM Value::equals——仅 Bool↔I64 跨标签按数值
// （Bool↔F64 与 I64↔F64 在 VM 亦为假，保持 tag 严格）。
static inline int w_val_eq(wval a, wval b) {
    if (a.t == WT_SIG) a = w_sig_inner(a);
    if (b.t == WT_SIG) b = w_sig_inner(b);
    if (a.t == WT_BOOL && b.t == WT_I64) return (a.v ? 1 : 0) == (int64_t)b.v;
    if (a.t == WT_I64 && b.t == WT_BOOL) return (int64_t)a.v == (b.v ? 1 : 0);
    return a.v == b.v && a.t == b.t;
}

// ── Stack ───────────────────────────────────────────────────────────

#define STACK_SIZE 65536
// Thread-local interpreter state: worker threads (serve-concurrent)
// each get a fresh stack; the heap stays per-thread too so arena
// pointers never cross threads.
//
// `__thread` is the GCC/Clang spelling. The C11 keyword `_Thread_local` is
// the portable one, and MSVC needs `__declspec(thread)` — spelled out here
// so the runtime is not silently GCC-only (see docs/independence-design.md,
// Step 6: C is meant to be a *substitutable* portable assembly, which means
// no compiler-specific extension may be load-bearing).
#if defined(_MSC_VER)
#  define W_TLS __declspec(thread)
#elif defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
#  define W_TLS _Thread_local
#else
#  define W_TLS __thread
#endif
static W_TLS wval *w_stack = NULL;   // allocated per thread
static W_TLS int  w_sp = 0;

// ── try (L-01): catchable failure = w_fail; nearest {..} try catches ──
// Value-level failures (type error, division by zero, stack underflow,
// JSON errors) unwind to the innermost try and push [0, message];
// resource failures (heap exhausted, malloc) stay fatal.
#ifndef W_WASI
#define W_TRY_MAX 512
static W_TLS jmp_buf w_try_jb[W_TRY_MAX];
static W_TLS int w_try_base[W_TRY_MAX];
static W_TLS int w_try_sp = 0;
static W_TLS char w_try_msg[512];
static void w_fail(const char *msg) {
    if (w_try_sp > 0) {
        w_try_sp--;
        w_sp = w_try_base[w_try_sp];
        snprintf(w_try_msg, sizeof w_try_msg, "%s", msg);
        longjmp(w_try_jb[w_try_sp], 1);
    }
    fprintf(stderr, "whisper: %s\n", msg);
    exit(1);
}
#else
// WASI has no portable setjmp/longjmp (needs the wasm EH proposal).
// v1: failures are fatal; `try` is explicitly unsupported on this profile.
static void w_fail(const char *msg) {
    fprintf(stderr, "whisper: %s\n", msg);
    exit(1);
}
#endif

// Ambient network is opt-in for native programs too.  WASM stays hard-off:
// its stubs fail before this check and there is no socket import.
static int w_net_enabled(void) {
    const char *v = getenv("WHISPER_NET");
    // Native is the trusted target: keep historical network-on behavior.
    // WHISPER_NET=0 lets a launcher turn it off for native too.
    if (!v || !*v) return 1;
    if (strcmp(v, "0") == 0 || strcmp(v, "false") == 0 ||
        strcmp(v, "no") == 0 || strcmp(v, "off") == 0) return 0;
    return 1;
}
static void w_net_require(const char *what) {
    if (!w_net_enabled()) {
        w_fail("network disabled (WHISPER_NET=0; WASM v1 has no network at all)");
    }
    (void)what;
}

#ifndef W_WASI
#ifdef _WIN32
static CRITICAL_SECTION w_net_cs;
static INIT_ONCE w_net_cs_once = INIT_ONCE_STATIC_INIT;
static BOOL CALLBACK w_net_cs_init(PINIT_ONCE once, PVOID param, PVOID *ctx) {
    (void)once; (void)param; (void)ctx;
    InitializeCriticalSection(&w_net_cs);
    return TRUE;
}
static void w_net_global_lock(void) {
    InitOnceExecuteOnce(&w_net_cs_once, w_net_cs_init, NULL, NULL);
    EnterCriticalSection(&w_net_cs);
}
static void w_net_global_unlock(void) { LeaveCriticalSection(&w_net_cs); }
#else
static pthread_mutex_t w_net_mutex = PTHREAD_MUTEX_INITIALIZER;
static void w_net_global_lock(void) { pthread_mutex_lock(&w_net_mutex); }
static void w_net_global_unlock(void) { pthread_mutex_unlock(&w_net_mutex); }
#endif
#endif

static int w_host_ci_eq(const char *a, size_t an, const char *b, size_t bn) {
    if (an != bn) return 0;
    for (size_t i = 0; i < an; i++) {
        unsigned char x = (unsigned char)a[i], y = (unsigned char)b[i];
        if (x >= 'A' && x <= 'Z') x = (unsigned char)(x + 32);
        if (y >= 'A' && y <= 'Z') y = (unsigned char)(y + 32);
        if (x != y) return 0;
    }
    return 1;
}

static int w_host_allowed(const char *host) {
    const char *e = getenv("WHISPER_NET_ALLOW");
    if (!e || !*e) return 1; // backward-compatible: no allowlist configured
    size_t hn = strlen(host);
    const char *p = e;
    while (*p) {
        while (*p == ',' || *p == ' ' || *p == '\t') p++;
        const char *s = p;
        while (*p && *p != ',' && *p != ' ' && *p != '\t') p++;
        size_t n = (size_t)(p - s);
        if (n == 1 && s[0] == '*') return 1;
        if (n > 2 && s[0] == '*' && s[1] == '.') {
            const char *suf = s + 2;
            size_t sn = n - 2;
            if (hn > sn && host[hn - sn - 1] == '.' &&
                w_host_ci_eq(host + hn - sn, sn, suf, sn)) return 1;
        } else if (w_host_ci_eq(host, hn, s, n)) {
            return 1;
        }
    }
    return 0;
}

static int w_net_allow_guard(const char *host, char *err, size_t errlen) {
    if (w_host_allowed(host)) return 0;
    snprintf(err, errlen, "host not allowed by WHISPER_NET_ALLOW: %s", host);
    return -1;
}

static int w_method_allowed(const char *method) {
    const char *e = getenv("WHISPER_NET_METHODS");
    if (!e || !*e) return 1;
    size_t mn = strlen(method);
    const char *p = e;
    while (*p) {
        while (*p == ',' || *p == ' ' || *p == '\t') p++;
        const char *s = p;
        while (*p && *p != ',' && *p != ' ' && *p != '\t') p++;
        if (w_host_ci_eq(method, mn, s, (size_t)(p - s))) return 1;
    }
    return 0;
}

static int w_port_allowed(int port) {
    const char *e = getenv("WHISPER_NET_PORTS");
    if (!e || !*e) return 1;
    const char *p = e;
    while (*p) {
        while (*p == ',' || *p == ' ' || *p == '\t') p++;
        if (*p >= '0' && *p <= '9') {
            long v = strtol(p, (char **)&p, 10);
            if (v == port) return 1;
        } else {
            while (*p && *p != ',' && *p != ' ' && *p != '\t') p++;
        }
    }
    return 0;
}

static size_t w_response_max(void) {
    const char *e = getenv("WHISPER_NET_MAX_RESPONSE");
    if (!e || !*e) return (size_t)16 * 1024 * 1024;
    unsigned long long v = strtoull(e, NULL, 10);
    if (v == 0) return (size_t)16 * 1024 * 1024;
    if (v > (unsigned long long)(256ULL * 1024 * 1024)) v = 256ULL * 1024 * 1024;
    return (size_t)v;
}

static inline wval W_I64(int64_t x)  { wval r; r.v = (uint64_t)x; r.t = WT_I64; return r; }
static inline wval W_F64(double d)   { wval r; memcpy(&r.v, &d, 8); r.t = WT_F64; return r; }
static inline wval W_PTR(void *p)    { wval r; r.v = (uint64_t)p;  r.t = WT_PTR; return r; }
static inline wval W_BOOL(int64_t b) { wval r; r.v = (uint64_t)b;  r.t = WT_BOOL; return r; }

static inline int w_is_f64(wval v) { return v.t == WT_F64; }
static inline int64_t w_as_i64(wval v) {
    if (v.t == WT_SIG) v = w_sig_inner(v);
    if (v.t == WT_BOOL) return v.v ? 1 : 0;  // N-1 数值协变（bool 即 1/0）
    if (v.t != WT_I64) { w_fail("type error: expected i64"); return 0; }
    return (int64_t)v.v;
}
static inline double w_as_f64(wval v) {
    if (v.t == WT_SIG) v = w_sig_inner(v);
    if (v.t != WT_F64) { w_fail("type error: expected f64"); return 0; }
    double d; memcpy(&d, &v.v, 8); return d;
}
static inline void *w_as_ptr(wval v) {
    if (v.t == WT_SIG) v = w_sig_inner(v);
    if (v.t != WT_PTR) { w_fail("type error: expected ptr"); return NULL; }
    return (void *)v.v;
}
static inline double w_num_f64(wval v) {  // numeric promotion (VM binary_num_op: i64 coerces to f64)
    if (v.t == WT_SIG) v = w_sig_inner(v);
    if (v.t == WT_BOOL) return v.v ? 1.0 : 0.0;
    if (v.t == WT_F64) return w_as_f64(v);
    if (v.t == WT_I64) return (double)(int64_t)v.v;
    w_fail("type error: expected i64 or f64"); return 0;
}

static volatile int _dbg_max_sp = 0;
static volatile int _dbg_call_depth = 0;

static inline void w_push(wval v) {
    if (w_sp >= STACK_SIZE) w_fail("stack overflow");
    w_stack[w_sp++] = v;
}
static wval w_pop(void) {
    if (w_sp <= 0) w_fail("stack underflow");
    return w_stack[--w_sp];
}
static inline wval w_peek(void) {
    if (w_sp <= 0) w_fail("stack underflow");
    return w_stack[w_sp - 1];
}
static inline void w_dup(void)          { w_push(w_peek()); }
static inline void w_swap(void)         { wval a=w_pop(), b=w_pop(); w_push(a); w_push(b); }
static inline void w_drop(void)         { w_sp--; }
static inline void w_rot(void)          { wval c=w_pop(),b=w_pop(),a=w_pop(); w_push(b); w_push(c); w_push(a); }
// Pick: w_pick_n(N) for $N operator, w_pick() reads index from stack
static inline void w_pick_n(int n) { w_push(w_stack[w_sp - 1 - n]); }
static void w_pick(void) { int64_t n = w_as_i64(w_pop()); w_push(w_stack[w_sp - 1 - (int)n]); }

// ── Tag-dispatching polymorphic ops ─────────────────────────────────
// Mirror the VM's binary_num_op: any f64 operand promotes the result to
// f64 (i64 coerces via w_num_f64); division/modulo by zero is an error.
static int wval_is_str(wval v);
static int64_t w_str_lt(const char *a, const char *b);

static void w_op_add(void) { wval b=w_pop(),a=w_pop();
    if (a.t==WT_F64 || b.t==WT_F64) { w_push(W_F64(w_num_f64(a)+w_num_f64(b))); return; }
    w_push(W_I64(w_as_i64(a)+w_as_i64(b))); }
static void w_op_sub(void) { wval b=w_pop(),a=w_pop();
    if (a.t==WT_F64 || b.t==WT_F64) { w_push(W_F64(w_num_f64(a)-w_num_f64(b))); return; }
    w_push(W_I64(w_as_i64(a)-w_as_i64(b))); }
static void w_op_mul(void) { wval b=w_pop(),a=w_pop();
    if (a.t==WT_F64 || b.t==WT_F64) { w_push(W_F64(w_num_f64(a)*w_num_f64(b))); return; }
    w_push(W_I64(w_as_i64(a)*w_as_i64(b))); }
static void w_op_div(void) { wval b=w_pop(),a=w_pop();
    if (a.t==WT_F64 || b.t==WT_F64) { double bb=w_num_f64(b); if (bb==0.0){w_fail("Division by zero");} w_push(W_F64(w_num_f64(a)/bb)); return; }
    int64_t bi=w_as_i64(b); if (bi==0){w_fail("Division by zero");} w_push(W_I64(w_as_i64(a)/bi)); }
static void w_op_mod(void) { wval b=w_pop(),a=w_pop();
    if (a.t==WT_F64 || b.t==WT_F64) { double bb=w_num_f64(b); if (bb==0.0){w_fail("Division by zero");} w_push(W_F64(fmod(w_num_f64(a),bb))); return; }
    int64_t bi=w_as_i64(b); if (bi==0){w_fail("Division by zero");} w_push(W_I64(w_as_i64(a)%bi)); }
// < : both operands strings (PTR in the string arena) → bytewise string
// compare; any f64 → numeric compare; else i64 compare.
static void w_op_lt(void) { wval b=w_pop(),a=w_pop();
    if (a.t==WT_PTR && b.t==WT_PTR && wval_is_str(a) && wval_is_str(b)) {
        w_push(W_BOOL(w_str_lt((const char *)w_as_ptr(a), (const char *)w_as_ptr(b)))); return; }
    if (a.t==WT_F64 || b.t==WT_F64) { w_push(W_BOOL(w_num_f64(a) < w_num_f64(b))); return; }
    w_push(W_BOOL(w_as_i64(a) < w_as_i64(b))); }
static void w_op_gt(void) { wval b=w_pop(),a=w_pop();
    if (a.t==WT_PTR && b.t==WT_PTR && wval_is_str(a) && wval_is_str(b)) {
        w_push(W_BOOL(w_str_lt((const char *)w_as_ptr(b), (const char *)w_as_ptr(a)))); return; }
    if (a.t==WT_F64 || b.t==WT_F64) { w_push(W_BOOL(w_num_f64(a) > w_num_f64(b))); return; }
    w_push(W_BOOL(w_as_i64(a) > w_as_i64(b))); }

// ── Heap allocator ──────────────────────────────────────────────────
// Split-arena bump allocator: strings grow up from the heap base,
// lists grow down from the heap end. A value's type is then decided
// by address range alone — no layout heuristics, no misclassification
// of short strings as lists.

// 16 GB reserve, 8 GB per arena (lazy commit — only touched pages cost RAM).
// The bootstrap lexer consumes source one char per strslice, so cumulative
// string allocation is O(source²): the full self-compile crosses 2 GB,
// which the old 4 GB total / 2 GB per arena could not hold (K-5).
#ifdef W_WASI
// wasm32 linear memory: keep the arena well below 4GB. v1 uses 64MB;
// bump this once the WASI target has a memory policy.
#define HEAP_SIZE (64LL * 1024 * 1024)
#else
#define HEAP_SIZE (16384LL * 1024 * 1024)  // 16 GB total, 8 GB per arena
#endif
static W_TLS char *w_heap = NULL;
static W_TLS size_t w_heap_pos = 0;       // string arena bump (up)
static W_TLS size_t w_heap_list_pos = 0;  // list arena bump (down)
// Worker threads use a smaller arena than the 4GB main heap; the
// split layout (strings up / lists down) works at any size.
static W_TLS long long w_heap_size = 0;

// Commit-on-demand：只保留地址空间（RESERVE），用到哪提交哪。
// 长寿命进程的提交内存随实际用量增长，而非启动即吃满 4GB 提交额。
// Linux/macOS 的 mmap 本就按需换页，无需处理。
static W_TLS size_t w_heap_committed = 0;
static W_TLS size_t w_heap_list_committed = 0;

static void w_heap_commit_through(char *end_a, char *list_front) {
#ifdef _WIN32
    const size_t GRAN = 65536;
    // string arena grows UP from the base: commit [base, end_a)
    size_t need_a = (size_t)(end_a - w_heap);
    if (need_a > w_heap_committed) {
        size_t target = ((need_a + GRAN - 1) / GRAN) * GRAN;
        if (VirtualAlloc(w_heap, target, MEM_COMMIT, PAGE_READWRITE) == NULL) {
            fprintf(stderr, "commit a failed at %zu bytes\n", target);
            exit(1);
        }
        w_heap_committed = target;
    }
    // list arena grows DOWN from the region end: commit the top slice
    // [list_front, end) only — never the whole span from the base.
    size_t used_b = (size_t)(w_heap + w_heap_size - list_front);
    if (used_b > w_heap_list_committed) {
        size_t target = ((used_b + GRAN - 1) / GRAN) * GRAN;
        if (VirtualAlloc(w_heap + w_heap_size - target, target, MEM_COMMIT, PAGE_READWRITE) == NULL) {
            fprintf(stderr, "commit b failed at %zu bytes\n", target);
            exit(1);
        }
        w_heap_list_committed = target;
    }
#else
    (void)end_a; (void)list_front;
#endif
}

static void w_heap_init(void) {
#ifdef _WIN32
    if (w_heap_size == 0) w_heap_size = HEAP_SIZE;
    w_heap = (char *)VirtualAlloc(NULL, (SIZE_T)w_heap_size, MEM_RESERVE, PAGE_READWRITE);
    if (w_heap == NULL) { fprintf(stderr, "VirtualAlloc failed for %lld bytes\n", w_heap_size); exit(1); }
#elif defined(W_WASI)
    if (w_heap_size == 0) w_heap_size = HEAP_SIZE;
    w_heap = (char *)malloc((size_t)w_heap_size);
    if (w_heap == NULL) { fprintf(stderr, "malloc failed for %lld bytes\n", w_heap_size); exit(1); }
#else
    if (w_heap_size == 0) {
        // Allow constrained hosts (CI containers with a low RLIMIT_AS) to
        // request a smaller heap without rebuilding the program.
        const char *hm = getenv("WHISPER_HEAP_MB");
        if (hm && *hm) {
            long mb = strtol(hm, NULL, 10);
            if (mb >= 4 && mb <= 16384) w_heap_size = (long long)mb * 1024LL * 1024LL;
        }
    }
    if (w_heap_size == 0) w_heap_size = HEAP_SIZE;
    {
        // GitHub Linux/macOS runners may cap virtual address space below the
        // 16GB reserve. Keep the lazy reserve but fall back to smaller heaps
        // instead of failing every program at startup.
        size_t want = (size_t)w_heap_size;
#ifdef MAP_NORESERVE
        int flags = MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE;
#else
        int flags = MAP_PRIVATE | MAP_ANONYMOUS;
#endif
        w_heap = MAP_FAILED;
        int got = 0;
        while (want >= (size_t)(16LL * 1024 * 1024)) {
            w_heap = mmap(NULL, want, PROT_READ | PROT_WRITE, flags, -1, 0);
            if (w_heap != MAP_FAILED) { got = 1; break; }
            w_heap = (char *)malloc(want);
            if (w_heap != NULL) { got = 1; break; }
            want /= 2;
        }
        if (!got) {
            fprintf(stderr, "mmap failed: requested=%lld errno=%d\n", w_heap_size, errno);
            perror("mmap");
            exit(1);
        }
        w_heap_size = (long long)want;
    }
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
    w_heap_commit_through(w_heap + w_heap_pos, w_heap + w_heap_size - w_heap_list_pos);
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
    w_heap_commit_through(w_heap + w_heap_pos, w_heap + w_heap_size - w_heap_list_pos);
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
            fprintf(stderr, "  [%d] = %lld (%p)\n", i, (long long)v.v, (void*)v.v);
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

// try runner (L-01): VM Try semantics — run body; success pushes
// [1, top-or-default-0], failure rolls the stack back to entry depth
// and pushes [0, message].
#ifndef W_WASI
static void w_try_call(void (*fn)(void)) {
    if (w_try_sp >= W_TRY_MAX) w_fail("try nesting too deep");
    w_try_base[w_try_sp] = w_sp;
    if (setjmp(w_try_jb[w_try_sp]) == 0) {
        w_try_sp++;
        fn();
        w_try_sp--;
        wval r = (w_sp > 0) ? w_pop() : W_I64(0);
        w_push(W_BOOL(1));
        w_push(r);
    } else {
        w_push(W_BOOL(0));
        w_push(W_PTR(w_str_from_cstr(w_try_msg)));
    }
}
#else
static void w_try_call(void (*fn)(void)) {
    (void)fn;
    w_fail("try is unsupported on wasm profile (no setjmp/longjmp)");
}
#endif

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
    if (v.t != WT_PTR) return 0;
    if (v.v < (uint64_t)(uintptr_t)w_heap || v.v >= (uint64_t)(uintptr_t)(w_heap + w_heap_size)) return 0;
    return v.v >= (uint64_t)(uintptr_t)(w_heap + w_heap_size / 2);
}

static inline wval *w_list_new(int64_t count) {
    wval *p = (wval *)w_alloc_list(sizeof(wval) * ((size_t)count + 1));
    p[0] = W_I64(count);
    return p;
}

static inline int64_t w_list_len(wval *list) {
    if (!list) {
        fprintf(stderr, "w_list_len: NULL, sp=%d\n", w_sp);
        for (int i = (w_sp > 6 ? w_sp - 6 : 0); i < w_sp; i++)
            fprintf(stderr, "  [%d] = %p (%lld)\n", i, (void*)w_stack[i].v, (long long)w_stack[i].v);
        exit(1);
    }
    return w_as_i64(list[0]);
}

static inline wval w_list_nth(wval *list, int64_t n) {
    return list[1 + n];
}

static wval *w_list_append(wval *list, wval elem) {
    int64_t count = w_list_len(list);
    wval *new_list = w_list_new(count + 1);
    memcpy(new_list + 1, list + 1, count * sizeof(wval));
    new_list[1 + count] = elem;
    return new_list;
}

static wval *w_list_prepend(wval elem, wval *list) {
    int64_t count = w_list_len(list);
    wval *new_list = w_list_new(count + 1);
    new_list[1] = elem;
    memcpy(new_list + 2, list + 1, count * sizeof(wval));
    return new_list;
}

static wval *w_list_slice(wval *list, int64_t start, int64_t len) {
    int64_t count = w_list_len(list);
    if (start < 0) start = 0;
    if (len < 0) len = 0;
    if (start > count) start = count;
    if (start + len > count) len = count - start;
    wval *new_list = w_list_new(len);
    memcpy(new_list + 1, list + 1 + start, len * sizeof(wval));
    return new_list;
}

// Sort ordering mirroring the VM's Sort: i64/i64 exact, mixed or f64 →
// f64 compare, str/str → bytewise; non-comparable pairs → "not less"
// (stable insertion sort keeps their relative order, as VM Equal does).
static int w_sort_lt(wval a, wval b) {
    if (a.t == WT_PTR && b.t == WT_PTR && wval_is_str(a) && wval_is_str(b))
        return w_str_lt((const char *)w_as_ptr(a), (const char *)w_as_ptr(b)) != 0;
    if (a.t == WT_I64 && b.t == WT_I64) return w_as_i64(a) < w_as_i64(b);
    if ((a.t == WT_I64 || a.t == WT_F64) && (b.t == WT_I64 || b.t == WT_F64))
        return w_num_f64(a) < w_num_f64(b);
    return 0;
}

static wval *w_list_sort(wval *list) {
    int64_t count = w_list_len(list);
    wval *new_list = w_list_new(count);
    memcpy(new_list + 1, list + 1, count * sizeof(wval));
    // Simple insertion sort (stable; handles i64, f64 and strings)
    for (int64_t i = 1; i < count; i++) {
        wval key = new_list[1 + i];
        int64_t j = i - 1;
        while (j >= 0 && w_sort_lt(key, new_list[1 + j])) {
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
    memcpy(new_list + 1, list + 1, n * sizeof(wval));
    return new_list;
}

static wval *w_list_drop(wval *list, int64_t n) {
    int64_t count = w_list_len(list);
    if (n > count) n = count;
    if (n < 0) n = 0;
    int64_t new_count = count - n;
    wval *new_list = w_list_new(new_count);
    memcpy(new_list + 1, list + 1 + n, new_count * sizeof(wval));
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
    wval *p = (wval *)w_alloc_list(sizeof(wval) * ((size_t)cap0 + 2));
    p[0] = W_I64(cap0);
    p[1] = W_I64(0);
    return p + 1;
}

static wval *w_buf_push(wval *buf, wval elem) {
    wval *meta = buf - 1;
    int64_t cap = w_as_i64(meta[0]);
    int64_t count = w_as_i64(buf[0]);
    if (count >= cap) {
        int64_t ncap = cap * 2;
        wval *np = (wval *)w_alloc_list(sizeof(wval) * ((size_t)ncap + 2));
        np[0] = W_I64(ncap);
        np[1] = W_I64(count);
        memcpy(np + 2, buf + 1, (size_t)count * sizeof(wval));
        buf = np + 1;
    }
    buf[1 + count] = elem;
    buf[0] = W_I64(count + 1);
    return buf;
}

// ── Type predicates (heap pointer heuristic) ───────────────────────
static int wval_is_heap(wval v) {
    if (v.t != WT_PTR) return 0;
    return (v.v >= (uint64_t)(uintptr_t)w_heap && v.v < (uint64_t)(uintptr_t)(w_heap + w_heap_size));
}
static int wval_is_str(wval v) {
    if (v.t == WT_SIG) v = w_sig_inner(v);
    if (!wval_is_heap(v)) return 0;
    return (v.v < (uint64_t)(uintptr_t)(w_heap + w_heap_size / 2));
}
static int wval_is_list(wval v) {
    if (v.t == WT_SIG) v = w_sig_inner(v);
    if (!wval_is_heap(v)) return 0;
    return (v.v >= (uint64_t)(uintptr_t)(w_heap + w_heap_size / 2));
}

// Tag check for structured tokens: tag 5 = braces {}, tag 6 = brackets []
static int is_tag(wval v, int64_t tag) {
    if (!wval_is_heap(v)) return 0;
    wval *p = (wval *)w_as_ptr(v);
    return w_list_len(p) == 2 && w_as_i64(p[1]) == tag;
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
            const char *s = (const char *)w_as_ptr(tok);
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
            if (w_eq_raw(result[1 + j], list[1 + i])) { found = 1; break; }
        }
        if (!found) result = w_list_append(result, list[1 + i]);
    }
    return result;
}

static wval *w_str_split(const char *s, const char *delim) {
    uint32_t slen = w_str_len(s), dlen = w_str_len(delim);
    // two passes: count parts first, allocate the list once, then fill.
    // (w_list_append copies the whole list — appending in a loop is O(n^2),
    //  which cost ~110ms on a 10k-line file before this fix)
    if (dlen == 0) {
        wval *result = w_list_new((int64_t)slen);
        for (uint32_t i = 0; i < slen; i++)
            result[1 + i] = W_PTR(w_str_new(s + i, 1));
        return result;
    }
    uint32_t parts = 1;
    for (uint32_t i = 0; i + dlen <= slen; i++) {
        if (memcmp(s + i, delim, dlen) == 0) {
            parts++;
            i += dlen - 1;
        }
    }
    wval *result = w_list_new((int64_t)parts);
    uint32_t idx = 0, start = 0;
    for (uint32_t i = 0; i + dlen <= slen; i++) {
        if (memcmp(s + i, delim, dlen) == 0) {
            result[1 + idx++] = W_PTR(w_str_new(s + start, i - start));
            start = i + dlen;
            i += dlen - 1;
        }
    }
    result[1 + idx] = W_PTR(w_str_new(s + start, slen - start));
    return result;
}

static wval *w_strchars(const char *s) {
    uint32_t len = w_str_len(s);
    wval *list = w_list_new(len);
    for (uint32_t i = 0; i < len; i++) list[1 + i] = W_I64((int64_t)(unsigned char)s[i]);
    return list;
}

static char *w_charsstr(wval *list) {
    int64_t count = w_list_len(list);
    char *buf = (char *)w_alloc(count + 1);
    for (int64_t i = 0; i < count; i++) buf[i] = (char)w_as_i64(list[1 + i]);
    return w_str_new(buf, count);
}

// StrIter: pushes (first-char, rest-string); on empty pushes (-1, "")
static void w_striter(const char *s) {
    if (w_str_len(s) == 0) {
        w_push(W_I64(-1));
        w_push(W_PTR(w_str_new("", 0)));
    } else {
        w_push(W_I64((int64_t)(unsigned char)s[0]));
        w_push(W_PTR(w_str_new(s + 1, w_str_len(s) - 1)));
    }
}

static char *w_strjoin(wval *list, const char *sep) {
    int64_t count = w_list_len(list);
    uint32_t sl = w_str_len(sep);
    size_t total = 0;
    for (int64_t i = 0; i < count; i++)
        total += w_str_len((const char *)w_as_ptr(list[1 + i])) + (i > 0 ? sl : 0);
    char *buf = (char *)w_alloc(total + 1);
    size_t pos = 0;
    for (int64_t i = 0; i < count; i++) {
        if (i > 0) {
            memcpy(buf + pos, sep, sl);
            pos += sl;
        }
        const char *s = (const char *)w_as_ptr(list[1 + i]);
        uint32_t l = w_str_len(s);
        memcpy(buf + pos, s, l);
        pos += l;
    }
    return w_str_new(buf, total);
}

static int64_t w_listfind(wval *list, wval elem) {
    int64_t count = w_list_len(list);
    int elem_is_str = wval_is_str(elem);
    for (int64_t i = 0; i < count; i++) {
        wval v = list[1 + i];
        if (w_eq_raw(v, elem)) return i;
        if (elem_is_str && wval_is_str(v) &&
            w_str_eq((const char *)w_as_ptr(v), (const char *)w_as_ptr(elem))) return i;
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

// L-1 `print`: write a string to stdout verbatim — no trailing newline, no
// quoting, no rewriting. (Verbatim-ness also relies on stdout being binary;
// w_init switches it there, so this is a plain write.)
static void w_print_raw(const char *s) {
    fputs(s, stdout);
    fflush(stdout);
}

static int w_pair_is_marker(wval pv);  // defined with the map helpers
static void w_dump_val(wval v) {
    if (v.t == WT_SIG) { w_dump_val(w_sig_inner(v)); return; }  // transparent print
    if (v.t == WT_PTR && v.v >= (uint64_t)(uintptr_t)w_heap && v.v < (uint64_t)(uintptr_t)(w_heap + w_heap_size)) {
        if (w_is_list_val(v)) {
            wval *l = (wval *)w_as_ptr(v);
            int64_t count = w_as_i64(l[0]);
            if (count < 10000000) {
                if (count > 0 && w_pair_is_marker(l[1])) {  // map: {"k": v, ...}
                    printf("{");
                    int first = 1;
                    for (int64_t i = 0; i < count; i++) {
                        if (w_pair_is_marker(l[1 + i])) continue;
                        wval *pair = (wval *)w_as_ptr(l[1 + i]);
                        if (!first) printf(", ");
                        first = 0;
                        w_dump_val(pair[1]);
                        printf(": ");
                        w_dump_val(pair[2]);
                    }
                    printf("}");
                    return;
                }
                printf("[");
                for (int64_t i = 0; i < count; i++) {
                    if (i > 0) printf(" ");
                    w_dump_val(l[1 + i]);
                }
                printf("]");
                return;
            }
        } else {
            const char *s = (const char *)w_as_ptr(v);
            uint32_t len = w_str_len(s);
            if (len < 10000000) {
                fwrite(s, 1, len, stdout);
                return;
            }
        }
    }
    // f64: %g approximates Rust's Display (shortest roundtrip) for
    // common magnitudes; extreme exponents still differ (1e+10 vs
    // 10000000000) — accepted compromise for this pass.
    if (v.t == WT_F64) { printf("%g", w_as_f64(v)); return; }
    if (v.t == WT_BOOL) { fputs(v.v ? "#t" : "#f", stdout); return; }  // VM Display 对齐
    // Default: print as i64
    printf("%lld", (long long)(int64_t)v.v);
}

static char *w_read_line(void) {
    char buf[4096];
    if (!fgets(buf, sizeof(buf), stdin)) return w_str_new("", 0);
    size_t len = strlen(buf);
    if (len > 0 && buf[len-1] == '\n') len--;
    if (len > 0 && buf[len-1] == '\r') len--;
    return w_str_new(buf, len);
}

// ── P0 vocabulary (v1.0.3): ASCII case / trim / format ──────────────

static wval w_str_case(wval s, int upper) {
    const char *p = (const char *)w_as_ptr(s);
    uint32_t n = w_str_len(p);
    char *buf = (char *)malloc(n > 0 ? n : 1);
    if (!buf) return W_PTR(w_str_new("", 0));
    for (uint32_t i = 0; i < n; i++) {
        unsigned char c = (unsigned char)p[i];
        if (upper && c >= 'a' && c <= 'z') c -= 32;
        else if (!upper && c >= 'A' && c <= 'Z') c += 32;
        buf[i] = (char)c;
    }
    wval r = W_PTR(w_str_new(buf, n));
    free(buf);
    return r;
}

static wval w_str_strip(wval s) {
    const char *p = (const char *)w_as_ptr(s);
    uint32_t n = w_str_len(p), a = 0, b = n;
    while (a < b && (p[a]==' '||p[a]=='\t'||p[a]=='\r'||p[a]=='\n')) a++;
    while (b > a && (p[b-1]==' '||p[b-1]=='\t'||p[b-1]=='\r'||p[b-1]=='\n')) b--;
    return W_PTR(w_str_new(p + a, b - a));
}

// ── P1 vocabulary (v1.0.4): directory ops + UTC strftime ────────────
// ── P2 vocabulary (v1.0.5): minimal backtracking regex ──────────────
// ── FFI (v1.0.8): native library loading and symbol calls ───────────
// Mirrors crates/whisper-core/src/ffi.rs. Handles/fnptrs are raw i64s.

#ifndef W_WASI
#ifdef _WIN32
#include <windows.h>
static wval w_ffi_open(wval path) {
    const char *p = (const char *)w_as_ptr(path);
    HMODULE h = LoadLibraryA(p);
    return W_I64((int64_t)(uintptr_t)h);
}
static void *w_ffi_sym(int64_t handle, const char *name) {
    if (handle <= 0) return NULL;
    return (void *)GetProcAddress((HMODULE)handle, name);
}
#else
#include <dlfcn.h>
static void *w_ffi_open(const char *path) {
    return dlopen(path, RTLD_NOW);
}
static void *w_ffi_sym(int64_t handle, const char *name) {
    if (handle <= 0) return NULL;
    return dlsym((void *)handle, name);
}
#endif

// fmt: first char = return type, rest = arg types (i=i64, p=string ptr,
// d=f64 [rejected], v=void). Max 8 args. The 8-register generic shim
// matches both win-x64 and SysV for integer/pointer args.
static int64_t w_ffi_call(int64_t fnptr, const char *fmt, wval *args) {
    typedef int64_t (*fn8)(int64_t, int64_t, int64_t, int64_t, int64_t,
                           int64_t, int64_t, int64_t, int64_t, int64_t,
                           int64_t, int64_t);
    if (!fnptr) return 0;
    size_t fl = w_str_len(fmt);
    if (fl == 0) return 0;
    const char *at = fmt + 1;
    size_t na = fl - 1;
    int64_t n = w_as_i64(args[0]);
    if ((size_t)n != na) return 0; // arity mismatch: silent 0 (sentinel)
    if (na > 12) return 0;
    int64_t a[12] = {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
    for (int64_t i = 0; i < (int64_t)na; i++) {
        char t = at[i];
        if (t == 'd') return 0; // f64 args unsupported in v1.0.8
        wval v = args[1 + i];
        if (t == 'p') {
            // heap strings pass through; buf-alloc'd (malloc'd) raw
            // pointers are valid too — only reject obvious garbage
            // (small integers that are not plausible addresses)
            a[i] = (int64_t)v.v;
        } else {
            a[i] = (int64_t)v.v;
        }
    }
    fn8 f = (fn8)fnptr;
    // void return: the callee leaves rax undefined; treating it as i64
    // matches the VM (which returns the register's bits too).
    return f(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7],
             a[8], a[9], a[10], a[11]);
}
#endif // !W_WASI

// ── P2 vocabulary (v1.0.6): struct pack/unpack, little-endian ───────
// Mirrors crates/whisper-core/src/structpack.rs byte for byte.

static int w_sp_width(uint8_t c) {
    switch (c) {
    case 'i': case 'I': return 4;
    case 'h': case 'H': return 2;
    case 'b': case 'B': return 1;
    case 'q': case 'Q': case 'd': return 8;
    default: return -1; // 's' variable / unknown
    }
}

// stack order at call: fmt on top, values under it
static wval w_pack(wval fmt, wval values) {
    const char *f = (const char *)w_as_ptr(fmt);
    uint32_t fl = w_str_len(f);
    if (fl > 0 && f[0] == '<') { f++; fl--; }
    wval *vals = (wval *)w_as_ptr(values);
    int64_t n = w_as_i64(vals[0]);
    char *buf = (char *)malloc((size_t)(fl * 8 + 9 + n * 16));
    if (!buf) return W_PTR(w_str_new("", 0));
    uint32_t o = 0;
    int64_t vi = 0, i = 0;
    while (i < (int64_t)fl) {
        uint8_t c = (uint8_t)f[i]; i++;
        if (c == 's') {
            if (vi >= n) { free(buf); return W_PTR(w_str_new("", 0)); }
            wval v = vals[1 + vi]; vi++;
            if (!wval_is_str(v)) {
                free(buf); return W_PTR(w_str_new("", 0));
            }
            const char *s = (const char *)w_as_ptr(v);
            uint32_t sl = w_str_len(s);
            memcpy(buf + o, s, sl); o += sl;
            buf[o++] = 0;
            continue;
        }
        int wdt = w_sp_width(c);
        if (wdt < 0 || vi >= n) { free(buf); return W_PTR(w_str_new("", 0)); }
        wval v = vals[1 + vi]; vi++;
        uint64_t raw;
        if (c == 'd') {
            // f64 travels as its bit pattern in the C chain
            if (v.t == WT_PTR) {
                free(buf); return W_PTR(w_str_new("", 0));
            }
            raw = v.v;
        } else {
            if (v.t == WT_PTR) {
                free(buf); return W_PTR(w_str_new("", 0));
            }
            raw = (uint64_t)w_as_i64(v);
        }
        for (int k = 0; k < wdt; k++) buf[o + k] = (char)((raw >> (8 * k)) & 0xFF);
        o += (uint32_t)wdt;
    }
    if (vi != n) { free(buf); return W_PTR(w_str_new("", 0)); }
    wval r = W_PTR(w_str_new(buf, o));
    free(buf);
    return r;
}

static wval w_unpack(wval fmt, wval bytes) {
    const char *f = (const char *)w_as_ptr(fmt);
    const char *b = (const char *)w_as_ptr(bytes);
    uint32_t fl = w_str_len(f), bl = w_str_len(b);
    if (fl > 0 && f[0] == '<') { f++; fl--; }
    wval *lst = w_list_new((int64_t)fl);
    int64_t n = 0;
    uint32_t pos = 0;
    int64_t i = 0;
    while (i < (int64_t)fl) {
        uint8_t c = (uint8_t)f[i]; i++;
        if (c == 's') {
            uint32_t start = pos;
            while (pos < bl && b[pos] != 0) pos++;
            if (pos == bl) { return W_PTR(w_list_new(0)); }
            lst[1 + n] = W_PTR(w_str_new(b + start, pos - start)); n++;
            pos += 1;
            continue;
        }
        int wdt = w_sp_width(c);
        if (wdt < 0 || pos + wdt > bl) { return W_PTR(w_list_new(0)); }
        uint64_t raw = 0;
        for (int k = 0; k < wdt; k++)
            raw |= ((uint64_t)(uint8_t)b[pos + k]) << (8 * k);
        pos += (uint32_t)wdt;
        int64_t v;
        if (c == 'i' || c == 'h' || c == 'b' || c == 'q') {
            int sh = 64 - 8 * wdt;
            v = ((int64_t)(raw << sh)) >> sh;
        } else {
            v = (int64_t)raw;
        }
        lst[1 + n] = W_I64(v); n++;
    }
    // n == fl for fixed fields; rebuild if 's' shrank the count
    if (n != (int64_t)fl) {
        wval *l2 = w_list_new(n);
        for (int64_t k = 0; k < n; k++) l2[1 + k] = lst[1 + k];
        return W_PTR(l2);
    }
    return W_PTR(lst);
}

// Mirrors crates/whisper-core/src/rematch.rs byte for byte.

static int w_re_class_match(const char *spec, uint32_t slen, uint8_t b) {
    int neg = 0;
    if (slen > 0 && spec[0] == '^') { neg = 1; spec++; slen--; }
    uint32_t i = 0; int hit = 0;
    while (i < slen) {
        if (i + 2 < slen && spec[i + 1] == '-') {
            if ((uint8_t)spec[i] <= b && b <= (uint8_t)spec[i + 2]) hit = 1;
            i += 3;
        } else {
            if ((uint8_t)spec[i] == b) hit = 1;
            i += 1;
        }
    }
    return hit != neg;
}

static int w_re_esc_match(uint8_t e, uint8_t b) {
    switch (e) {
    case 'd': return b >= '0' && b <= '9';
    case 'D': return !(b >= '0' && b <= '9');
    case 'w': return (b >= 'a' && b <= 'z') || (b >= 'A' && b <= 'Z')
                  || (b >= '0' && b <= '9') || b == '_';
    case 'W': return !((b >= 'a' && b <= 'z') || (b >= 'A' && b <= 'Z')
                  || (b >= '0' && b <= '9') || b == '_');
    case 's': return b == ' ' || b == 9 || b == 13 || b == 10;
    case 'S': return !(b == ' ' || b == 9 || b == 13 || b == 10);
    default:  return e == b;
    }
}

#define W_RE_MAXPAT 4096

static void w_re_precompute(const char *p, uint32_t n, uint32_t *m) {
    uint32_t stack[256]; int sp = 0; int in_class = 0;
    for (uint32_t i = 0; i < n; i++) m[i] = 0;
    for (uint32_t i = 0; i < n; i++) {
        switch (p[i]) {
        case '\\': i++; break;
        case '[': if (!in_class) { in_class = 1; stack[sp++] = i; } break;
        case ']': if (in_class) { in_class = 0; if (sp) m[stack[--sp]] = i; } break;
        case '(': if (!in_class) stack[sp++] = i; break;
        case ')': if (!in_class) { if (sp) m[stack[--sp]] = i; } break;
        }
    }
}

static int w_re_match_seq(const char *p, uint32_t start, uint32_t end,
                          const char *t, uint32_t ti, uint32_t tlen,
                          const uint32_t *m);

static int w_re_match_at(const char *p, uint32_t pi, uint32_t pend,
                         const char *t, uint32_t ti, uint32_t tlen,
                         const uint32_t *m) {
    int depth = 0, in_class = 0;
    for (uint32_t i = pi; i < pend; i++) {
        switch (p[i]) {
        case '\\': i++; break;
        case '[': in_class = 1; break;
        case ']': in_class = 0; break;
        case '(': if (!in_class) depth++; break;
        case ')': if (!in_class) depth--; break;
        case '|': if (!in_class && depth == 0) {
            int e = w_re_match_seq(p, pi, i, t, ti, tlen, m);
            if (e >= 0) return e;
            return w_re_match_at(p, i + 1, pend, t, ti, tlen, m);
        }
        }
    }
    return w_re_match_seq(p, pi, pend, t, ti, tlen, m);
}

static void w_re_atom_bounds(const char *p, uint32_t start, const uint32_t *m,
                             uint32_t *lo, uint32_t *hi, uint32_t *after) {
    if (p[start] == '(' || p[start] == '[') {
        *lo = start; *hi = m[start] + 1; *after = m[start] + 1;
    } else if (p[start] == '\\') {
        *lo = start; *hi = start + 2; *after = start + 2;
    } else {
        *lo = start; *hi = start + 1; *after = start + 1;
    }
}

static int w_re_atom_once(const char *p, uint32_t lo, uint32_t hi,
                          const char *t, uint32_t ti, uint32_t tlen,
                          const uint32_t *m) {
    if (lo >= hi) return (int)ti;
    switch (p[lo]) {
    case '.':
        return ti < tlen ? (int)(ti + 1) : -1;
    case '[':
        if (ti < tlen && w_re_class_match(p + lo + 1, hi - 1 - (lo + 1),
                                          (uint8_t)t[ti]))
            return (int)(ti + 1);
        return -1;
    case '(':
        if (hi - lo < 2) return (int)ti;
        return w_re_match_at(p, lo + 1, hi - 1, t, ti, tlen, m);
    case '^': return ti == 0 ? (int)ti : -1;
    case '$': return ti == tlen ? (int)ti : -1;
    case '\\':
        if (ti < tlen && w_re_esc_match((uint8_t)p[lo + 1], (uint8_t)t[ti]))
            return (int)(ti + 1);
        return -1;
    default:
        if (ti < tlen && (uint8_t)t[ti] == (uint8_t)p[lo]) return (int)(ti + 1);
        return -1;
    }
}

static int w_re_match_seq(const char *p, uint32_t start, uint32_t end,
                          const char *t, uint32_t ti, uint32_t tlen,
                          const uint32_t *m) {
    if (start == end) return (int)ti;
    uint32_t a_lo, a_hi, after;
    w_re_atom_bounds(p, start, m, &a_lo, &a_hi, &after);
    int zero_width = p[a_lo] == '^' || p[a_lo] == '$'
                     || (p[a_lo] == '(' && a_hi <= a_lo + 2);
    uint32_t qmin = 1, qmax = 1, next = after;
    if (!zero_width && next < end) {
        switch (p[next]) {
        case '*': qmin = 0; qmax = 0xFFFFFFFFu; next++; break;
        case '+': qmin = 1; qmax = 0xFFFFFFFFu; next++; break;
        case '?': qmin = 0; qmax = 1; next++; break;
        }
    }
    static uint32_t ends[W_RE_MAXPAT + 1];
    uint32_t nends = 1;
    ends[0] = ti; uint32_t cur = ti;
    for (;;) {
        if (nends > qmax) break;
        int n = w_re_atom_once(p, a_lo, a_hi, t, cur, tlen, m);
        if (n < 0) break;
        if (zero_width) { ends[nends++] = (uint32_t)n; break; }
        if ((uint32_t)n > cur) { cur = (uint32_t)n; ends[nends++] = cur; }
        else break;
    }
    if (nends < qmin + 1) return -1;
    while (nends > qmin) {
        nends--;
        int r = w_re_match_seq(p, next, end, t, ends[nends], tlen, m);
        if (r >= 0) return r;
    }
    return -1;
}

static wval w_strmatch(const char *sv, const char *pv) {
    uint32_t sl = w_str_len(sv), pl = w_str_len(pv);

    static uint32_t m[W_RE_MAXPAT];
    if (pl > W_RE_MAXPAT) return W_I64(0);
    w_re_precompute(pv, pl, m);
    int e = w_re_match_at(pv, 0, pl, sv, 0, sl, m);
    return W_I64(e >= 0 && (uint32_t)e == sl ? 1 : 0);
}

static wval w_strfind_re(const char *sv, const char *pv) {
    uint32_t sl = w_str_len(sv), pl = w_str_len(pv);
    static uint32_t m[W_RE_MAXPAT];
    if (pl > W_RE_MAXPAT) return W_I64(-1);
    w_re_precompute(pv, pl, m);
    for (uint32_t start = 0; start <= sl; start++) {
        if (w_re_match_at(pv, 0, pl, sv, start, sl, m) >= 0)
            return W_I64((int64_t)start);
    }
    return W_I64(-1);
}


#include <sys/stat.h>
#ifdef _WIN32
#include <direct.h>
#include <dirent.h>
#define w_mkdir_impl(p) _mkdir(p)
#else
#include <sys/stat.h>
#include <dirent.h>
#define w_mkdir_impl(p) mkdir(p, 0755)
#endif

static wval w_mkdir(wval path) {
    return W_I64(w_mkdir_impl((const char *)w_as_ptr(path)) == 0 ? 0 : -1);
}

// list of entry names, sorted bytewise; empty list on error
static int w_cstr_cmp(const void *a, const void *b) {
    return strcmp(*(const char **)a, *(const char **)b);
}
static wval w_listdir(wval path) {
    const char *p = (const char *)w_as_ptr(path);
    DIR *d = opendir(p);
    if (!d) return W_PTR(w_list_new(0));
    char **names = NULL;
    int64_t n = 0, cap = 0;
    struct dirent *e;
    while ((e = readdir(d)) != NULL) {
        if (n == cap) {
            cap = cap ? cap * 2 : 16;
            names = (char **)realloc(names, cap * sizeof(char *));
            if (!names) { closedir(d); return W_PTR(w_list_new(0)); }
        }
        // copy into the string arena so it outlives the DIR handle
        uint32_t l = (uint32_t)strlen(e->d_name);
        names[n] = (char *)w_str_new(e->d_name, l);
        n++;
    }
    closedir(d);
    if (n > 1) qsort(names, (size_t)n, sizeof(char *), w_cstr_cmp);
    wval *lst = w_list_new(n);
    for (int64_t i = 0; i < n; i++) lst[1 + i] = W_PTR(names[i]);
    free(names);
    return W_PTR(lst);
}

// UTC formatting, subset %Y %m %d %H %M %S %% — mirrors the Rust
// strftime::format_utc byte for byte (no platform strftime on purpose).
static void w_civil_from_days(int64_t z, int64_t *y, int *mo, int *dd) {
    z += 719468;
    int64_t era = (z >= 0 ? z : z - 146096) / 146097;
    uint64_t doe = (uint64_t)(z - era * 146097);
    uint64_t yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    int64_t yy = (int64_t)yoe + era * 400;
    uint64_t doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    uint64_t mp = (5 * doy + 2) / 153;
    *dd = (int)(doy - (153 * mp + 2) / 5 + 1);
    *mo = (int)(mp < 10 ? mp + 3 : mp - 9);
    *y = (*mo <= 2) ? yy + 1 : yy;
}
static wval w_strftime_utc(int64_t ts, const char *fmt) {
    int64_t days = ts / 86400;
    int64_t secs = ts % 86400;
    if (secs < 0) { secs += 86400; days -= 1; }
    int64_t y; int mo, dd;
    w_civil_from_days(days, &y, &mo, &dd);
    char tmp[8];
    const char *f = fmt;
    uint32_t n = w_str_len(f);
    // worst case growth: each %X becomes <= 2 digits replaced by <= 4
    char *buf = (char *)malloc(n * 4 + 8);
    if (!buf) return W_PTR(w_str_new("", 0));
    uint32_t o = 0;
    for (uint32_t i = 0; i < n; i++) {
        if (f[i] == '%' && i + 1 < n) {
            const char *rep = NULL;
            int reqlen = 0;
            switch (f[i + 1]) {
            case 'Y': snprintf(tmp, sizeof(tmp), "%04lld", (long long)y);
                      rep = tmp; reqlen = (int)strlen(tmp); break;
            case 'm': snprintf(tmp, sizeof(tmp), "%02d", mo);
                      rep = tmp; reqlen = 2; break;
            case 'd': snprintf(tmp, sizeof(tmp), "%02d", dd);
                      rep = tmp; reqlen = 2; break;
            case 'H': snprintf(tmp, sizeof(tmp), "%02lld",
                      (long long)(secs / 3600)); rep = tmp; reqlen = 2; break;
            case 'M': snprintf(tmp, sizeof(tmp), "%02lld",
                      (long long)((secs % 3600) / 60)); rep = tmp; reqlen = 2; break;
            case 'S': snprintf(tmp, sizeof(tmp), "%02lld",
                      (long long)(secs % 60)); rep = tmp; reqlen = 2; break;
            case '%': rep = "%"; reqlen = 1; break;
            }
            if (rep) {
                memcpy(buf + o, rep, (size_t)reqlen);
                o += (uint32_t)reqlen;
                i += 1;
                continue;
            }
        }
        buf[o++] = f[i];
    }
    wval r = W_PTR(w_str_new(buf, o));
    free(buf);
    return r;
}

// fmt: replace the first {} in fstr with value rendered as text
// (string ptr => raw string; f64 => %g; anything else => i64 — mirrors
// the VM's Fmt rendering: I64/F64 to_string, Str unquoted)
static wval w_fmt(wval f, wval v) {
    const char *fs = (const char *)w_as_ptr(f);
    uint32_t n = w_str_len(fs);
    for (uint32_t i = 0; i + 1 < n; i++) {
        if (fs[i] == '{' && fs[i+1] == '}') {
            const char *vs;
            char tmp[32];
            uint32_t vn;
            if (wval_is_str(v)) {
                vs = (const char *)w_as_ptr(v);
                vn = w_str_len(vs);
            } else if (v.t == WT_F64) {
                vs = tmp;
                snprintf(tmp, sizeof(tmp), "%g", w_as_f64(v));
                vn = (uint32_t)strlen(tmp);
            } else {
                vs = tmp;
                snprintf(tmp, sizeof(tmp), "%lld", (long long)w_as_i64(v));
                vn = (uint32_t)strlen(tmp);
            }
            // new = fs[:i] + vs + fs[i+2:]
            uint32_t total = i + vn + (n - i - 2);
            char *buf = (char *)malloc(total > 0 ? total : 1);
            if (!buf) return W_PTR(w_str_new("", 0));
            memcpy(buf, fs, i);
            memcpy(buf + i, vs, vn);
            memcpy(buf + i + vn, fs + i + 2, n - i - 2);
            wval r = W_PTR(w_str_new(buf, total));
            free(buf);
            return r;
        }
    }
    return W_PTR(w_str_new(fs, n)); // no placeholder: unchanged
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


// ── Map operations (association list) ───────────────────────────────

// Maps (L-02): marker-prefixed pair lists — element[0] is a magic
// [MAGIC, 0] pair so maps and plain lists of pairs stay distinguishable
// (VM has Value::Map; the C chain has one pointer type). json-stringify
// serializes marker maps as objects, plain lists as arrays — the L-08
// heuristic corruption cannot recur.
#define W_MAP_MAGIC 0x774D4150ull // "wMAP"
static int w_pair_is_marker(wval pv) {
    if (pv.t != WT_PTR || !wval_is_list(pv)) return 0;
    wval *p = (wval *)pv.v;
    return w_list_len(p) == 2 && p[1].t == WT_I64 && p[1].v == W_MAP_MAGIC;
}
static wval *w_map_new(void) {
    wval *m = w_list_new(1);
    wval *mk = w_list_new(2);
    mk[1] = W_I64((int64_t)W_MAP_MAGIC);
    mk[2] = W_I64(0);
    m[1] = W_PTR(mk);
    return m;
}

static wval w_map_get(wval *map, const char *key) {
    int64_t count = w_list_len(map);
    for (int64_t i = 0; i < count; i++) {
        if (w_pair_is_marker(map[1 + i])) continue;
        wval *pair = (wval *)w_as_ptr(map[1 + i]);
        if (pair && w_list_len(pair) == 2) {
            const char *k = (const char *)w_as_ptr(pair[1]);
            if (w_str_eq(k, key)) return pair[2];
        }
    }
    return W_I64(0);
}

static int64_t w_map_has(wval *map, const char *key) {
    int64_t count = w_list_len(map);
    for (int64_t i = 0; i < count; i++) {
        if (w_pair_is_marker(map[1 + i])) continue;
        wval *pair = (wval *)w_as_ptr(map[1 + i]);
        if (pair && w_list_len(pair) == 2) {
            const char *k = (const char *)w_as_ptr(pair[1]);
            if (w_str_eq(k, key)) return 1;
        }
    }
    return 0;
}

static wval *w_map_set(wval *map, const char *key, wval value) {
    int64_t count = w_list_len(map);
    int64_t ins_at = count;  // default: append at end
    // Key exists → update in place; otherwise find the sorted insert
    // position. M-1: the VM Map is a BTreeMap, so map-keys / iteration
    // order is bytewise-sorted in BOTH implementations.
    for (int64_t i = 0; i < count; i++) {
        if (w_pair_is_marker(map[1 + i])) continue;
        wval *pair = (wval *)w_as_ptr(map[1 + i]);
        if (pair && w_list_len(pair) == 2) {
            const char *k = (const char *)w_as_ptr(pair[1]);
            if (w_str_eq(k, key)) {
                // Update in place (mutation for simplicity)
                pair[2] = value;
                return map;
            }
            if (w_str_lt(key, k)) { ins_at = i; break; }
        }
    }
    // Add new entry at the sorted position
    wval *pair = w_list_new(2);
    pair[1] = W_PTR((void *)key);
    pair[2] = value;
    wval *new_map = w_list_new(count + 1);
    int64_t j = 0;
    for (int64_t i = 0; i < count; i++, j++) {
        if (i == ins_at) j++;  // reserve the slot for the new pair
        new_map[1 + j] = map[1 + i];
    }
    new_map[1 + ins_at] = W_PTR((void *)pair);
    return new_map;
}

static wval *w_map_del(wval *map, const char *key) {
    int64_t count = w_list_len(map);
    wval *new_map = w_list_new(0);
    for (int64_t i = 0; i < count; i++) {
        if (w_pair_is_marker(map[1 + i])) continue;
        wval *pair = (wval *)w_as_ptr(map[1 + i]);
        if (pair && w_list_len(pair) == 2) {
            const char *k = (const char *)w_as_ptr(pair[1]);
            if (!w_str_eq(k, key)) {
                new_map = w_list_append(new_map, W_PTR((void *)pair));
            }
        }
    }
    return new_map;
}

static wval *w_map_keys(wval *map) {
    int64_t count = w_list_len(map);
    wval *keys = w_list_new(0);
    for (int64_t i = 0; i < count; i++) {
        if (w_pair_is_marker(map[1 + i])) continue;
        wval *pair = (wval *)w_as_ptr(map[1 + i]);
        keys = w_list_append(keys, pair ? pair[1] : W_I64(0));
    }
    return keys;
}

// ── JSON (L-02): parity with VM json-parse/json-stringify ──────────
// Objects → marker maps with keys inserted in sorted (bytewise) order —
// serde_json's BTreeMap prints objects sorted, and so does stringify.
// Booleans surface as I64 1/0 (the C chain has no bool tag; spec §4).
// \uXXXX escapes decode BMP-only (no surrogate pairs) — documented gap.
static const char *w_json_ws(const char *p) {
    while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
    return p;
}
static wval w_json_value(const char **pp);

static char *w_json_str(const char **pp) {  // *pp at opening quote
    const char *p = *pp + 1;
    size_t cap = 32, len = 0;
    char *b = (char *)malloc(cap);
    while (*p && *p != '"') {
        unsigned cp = (unsigned char)*p;
        if (*p == '\\' && p[1]) {
            p++;
            switch (*p) {
                case 'n': cp = '\n'; break;
                case 't': cp = '\t'; break;
                case 'r': cp = '\r'; break;
                case 'b': cp = '\b'; break;
                case 'f': cp = '\f'; break;
                case 'u': {
                    cp = 0;
                    for (int i = 1; i <= 4; i++) {
                        char h = p[i]; cp <<= 4;
                        if (h >= '0' && h <= '9') cp |= (unsigned)(h - '0');
                        else if (h >= 'a' && h <= 'f') cp |= (unsigned)(h - 'a' + 10);
                        else if (h >= 'A' && h <= 'F') cp |= (unsigned)(h - 'A' + 10);
                        else w_fail("JSON parse: bad \\u escape");
                    }
                    p += 4;
                    break;
                }
                default: cp = (unsigned char)*p; break;  // \" \\ \/ pass through
            }
        }
        if (len + 4 >= cap) { cap *= 2; b = (char *)realloc(b, cap); }
        if (cp < 0x80) {
            b[len++] = (char)cp;
        } else if (cp < 0x800) {
            b[len++] = (char)(0xC0 | (cp >> 6));
            b[len++] = (char)(0x80 | (cp & 0x3F));
        } else {
            b[len++] = (char)(0xE0 | (cp >> 12));
            b[len++] = (char)(0x80 | ((cp >> 6) & 0x3F));
            b[len++] = (char)(0x80 | (cp & 0x3F));
        }
        p++;
    }
    if (*p != '"') w_fail("JSON parse: unterminated string");
    *pp = p + 1;
    char *s = w_str_new(b, (uint32_t)len);
    free(b);
    return s;
}

static wval w_json_value(const char **pp) {
    const char *p = w_json_ws(*pp);
    if (*p == '{') {
        p = w_json_ws(p + 1);
        size_t n = 0, cap = 8;
        char **ks = (char **)malloc(cap * sizeof(char *));
        wval *vs = (wval *)malloc(cap * sizeof(wval));
        if (*p != '}') {
            for (;;) {
                if (*p != '"') w_fail("JSON parse: expected string key");
                ks[n] = w_json_str(&p);
                p = w_json_ws(p);
                if (*p != ':') w_fail("JSON parse: expected ':'");
                p = w_json_ws(p + 1);
                vs[n] = w_json_value(&p);
                n++;
                if (n == cap) {
                    cap *= 2;
                    ks = (char **)realloc(ks, cap * sizeof(char *));
                    vs = (wval *)realloc(vs, cap * sizeof(wval));
                }
                p = w_json_ws(p);
                if (*p == ',') { p = w_json_ws(p + 1); continue; }
                break;
            }
        }
        if (*p != '}') w_fail("JSON parse: expected '}'");
        *pp = p + 1;
        for (size_t i = 1; i < n; i++) {  // BTreeMap parity: sort keys bytewise
            char *k = ks[i]; wval v = vs[i]; size_t j = i;
            while (j > 0 && strcmp(ks[j - 1], k) > 0) { ks[j] = ks[j - 1]; vs[j] = vs[j - 1]; j--; }
            ks[j] = k; vs[j] = v;
        }
        wval *m = w_map_new();
        for (size_t i = 0; i < n; i++) m = w_map_set(m, ks[i], vs[i]);
        free(ks); free(vs);
        return W_PTR(m);
    }
    if (*p == '[') {
        p = w_json_ws(p + 1);
        size_t n = 0, cap = 8;
        wval *es = (wval *)malloc(cap * sizeof(wval));
        if (*p != ']') {
            for (;;) {
                es[n++] = w_json_value(&p);
                if (n == cap) { cap *= 2; es = (wval *)realloc(es, cap * sizeof(wval)); }
                p = w_json_ws(p);
                if (*p == ',') { p = w_json_ws(p + 1); continue; }
                break;
            }
        }
        if (*p != ']') w_fail("JSON parse: expected ']'");
        *pp = p + 1;
        wval *l = w_list_new((int64_t)n);
        for (size_t i = 0; i < n; i++) l[1 + i] = es[i];
        free(es);
        return W_PTR(l);
    }
    if (*p == '"') { char *s = w_json_str(&p); *pp = p; return W_PTR(s); }
    if (!strncmp(p, "true", 4))  { *pp = p + 4; return W_BOOL(1); }  // VM: Bool
    if (!strncmp(p, "false", 5)) { *pp = p + 5; return W_BOOL(0); }
    if (!strncmp(p, "null", 4))  { *pp = p + 4; return W_I64(0); }
    {   // number: int unless it carries . e E
        const char *q = p; int isf = 0;
        if (*q == '-' || *q == '+') q++;
        while ((*q >= '0' && *q <= '9') || *q == '.' || *q == 'e' || *q == 'E' || *q == '+' || *q == '-') {
            if (*q == '.' || *q == 'e' || *q == 'E') isf = 1;
            q++;
        }
        if (q == p) w_fail("JSON parse: unexpected character");
        char *end;
        if (isf) {
            double d = strtod(p, &end);
            *pp = end;
            return W_F64(d);
        }
        long long iv = strtoll(p, &end, 10);
        *pp = end;
        return W_I64((int64_t)iv);
    }
}

static wval w_json_parse_cstr(const char *s) {
    const char *p = s;
    wval r = w_json_value(&p);
    p = w_json_ws(p);
    if (*p) w_fail("JSON parse: trailing characters");
    return r;
}

// growable output buffer for stringify
typedef struct { char *p; size_t len, cap; } w_sb;
static void w_sb_put(w_sb *b, const char *s, size_t n) {
    if (b->len + n > b->cap) {
        size_t nc = b->cap ? b->cap * 2 : 64;
        while (nc < b->len + n) nc *= 2;
        b->p = (char *)realloc(b->p, nc);
        b->cap = nc;
    }
    memcpy(b->p + b->len, s, n);
    b->len += n;
}
static void w_sb_c(w_sb *b, char c) { w_sb_put(b, &c, 1); }

static void w_tls_json_escape(w_sb *b, const char *s, uint32_t n) {
    w_sb_c(b, '"');
    for (uint32_t i = 0; i < n; i++) {
        unsigned char c = (unsigned char)s[i];
        switch (c) {
            case '"':  w_sb_put(b, "\\\"", 2); break;
            case '\\': w_sb_put(b, "\\\\", 2); break;
            case '\n': w_sb_put(b, "\\n", 2); break;
            case '\r': w_sb_put(b, "\\r", 2); break;
            case '\t': w_sb_put(b, "\\t", 2); break;
            case '\b': w_sb_put(b, "\\b", 2); break;
            case '\f': w_sb_put(b, "\\f", 2); break;
            default:
                if (c < 0x20) { char t[8]; snprintf(t, sizeof t, "\\u%04x", c); w_sb_put(b, t, 6); }
                else w_sb_c(b, (char)c);
        }
    }
    w_sb_c(b, '"');
}

// f64 text: ryu parity for common shapes — integral → "4.0"; otherwise
// %g with exponent fixup (1e+30 → 1e30, 1.5e-07 → 1.5e-7).
static void w_json_num(w_sb *b, double d) {
    char t[64];
    if (d == (double)(long long)d && fabs(d) < 1e15) {
        snprintf(t, sizeof t, "%.1f", d);
        w_sb_put(b, t, strlen(t));
        return;
    }
    snprintf(t, sizeof t, "%g", d);
    char *e = strchr(t, 'e');
    if (e) {
        char fix[64]; size_t o = (size_t)(e - t);
        memcpy(fix, t, o);
        e++;                    // skip 'e'
        int neg = 0;
        if (*e == '+') e++;
        else if (*e == '-') { neg = 1; e++; }
        while (*e == '0' && e[1] >= '0' && e[1] <= '9') e++;  // strip leading zeros
        fix[o++] = 'e';
        if (neg) fix[o++] = '-';
        size_t el = strlen(e);
        memcpy(fix + o, e, el);
        fix[o + el] = 0;
        w_sb_put(b, fix, o + el);
        return;
    }
    w_sb_put(b, t, strlen(t));
}

static void w_json_dump(w_sb *b, wval v) {
    if (v.t == WT_SIG) { w_json_dump(b, w_sig_inner(v)); return; }
    if (v.t == WT_BOOL) { w_sb_put(b, v.v ? "true" : "false", v.v ? 4 : 5); return; }
    if (v.t == WT_I64) { char t[32]; snprintf(t, sizeof t, "%lld", (long long)(int64_t)v.v); w_sb_put(b, t, strlen(t)); return; }
    if (v.t == WT_F64) { w_json_num(b, w_as_f64(v)); return; }
    if (v.t != WT_PTR) { w_sb_put(b, "null", 4); return; }
    if (wval_is_str(v)) {
        const char *s = (const char *)w_as_ptr(v);
        w_tls_json_escape(b, s, w_str_len(s));
        return;
    }
    wval *l = (wval *)w_as_ptr(v);
    int64_t n = w_list_len(l);
    if (n > 0 && w_pair_is_marker(l[1])) {
        // object: collect + sort keys (BTreeMap parity), compact emit
        char **ks = (char **)malloc((size_t)n * sizeof(char *));
        wval *vs = (wval *)malloc((size_t)n * sizeof(wval));
        int64_t m = 0;
        for (int64_t i = 0; i < n; i++) {
            if (w_pair_is_marker(l[1 + i])) continue;
            wval *pair = (wval *)w_as_ptr(l[1 + i]);
            ks[m] = (char *)w_as_ptr(pair[1]);
            vs[m] = pair[2];
            m++;
        }
        for (int64_t i = 1; i < m; i++) {
            char *k = ks[i]; wval v2 = vs[i]; int64_t j = i;
            while (j > 0 && strcmp(ks[j - 1], k) > 0) { ks[j] = ks[j - 1]; vs[j] = vs[j - 1]; j--; }
            ks[j] = k; vs[j] = v2;
        }
        w_sb_c(b, '{');
        for (int64_t i = 0; i < m; i++) {
            if (i) w_sb_c(b, ',');
            w_tls_json_escape(b, ks[i], w_str_len(ks[i]));
            w_sb_c(b, ':');
            w_json_dump(b, vs[i]);
        }
        w_sb_c(b, '}');
        free(ks); free(vs);
        return;
    }
    w_sb_c(b, '[');
    for (int64_t i = 0; i < n; i++) {
        if (i) w_sb_c(b, ',');
        w_json_dump(b, l[1 + i]);
    }
    w_sb_c(b, ']');
}

static char *w_json_stringify(wval v) {
    w_sb b = {0};
    w_json_dump(&b, v);
    char *s = w_str_new(b.p, (uint32_t)(b.len ? b.len : 0));
    free(b.p);
    return s;
}


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
// NOTE: renamed from w_bitand/w_bitor/w_bitxor/w_bitnot — the transpiler
// prefixes user words with w_, and std/math defines bitand/bitor/bitxor/
// bitnot words, whose generated w_bitand() etc. collided with these.
static inline int64_t w_bit_and(int64_t a, int64_t b) { return a & b; }
static inline int64_t w_bit_or(int64_t a, int64_t b)  { return a | b; }
static inline int64_t w_bit_xor(int64_t a, int64_t b) { return a ^ b; }
static inline int64_t w_bit_not(int64_t a) { return ~a; }

// ── Program args ────────────────────────────────────────────────────

static int w_argc = 0;
static char **w_argv = NULL;

static wval *w_args(void) {
    wval *list = w_list_new(w_argc);
    for (int i = 0; i < w_argc; i++) {
        list[1 + i] = W_PTR(w_str_from_cstr(w_argv[i]));
    }
    return list;
}

// ── Initialization ──────────────────────────────────────────────────

// ── Type check functions (for Whisper list?, i64?, str?, bool?) ─────
static void w_list_(void) {
    wval v = w_pop();
    w_push(W_BOOL(w_is_list_val(v) ? 1L : 0L));
}
static void w_i64_(void) {
    wval v = w_pop();
    w_push(W_BOOL(v.t == WT_I64 ? 1L : 0L));
}
static void w_str_(void) {
    wval v = w_pop();
    w_push(W_BOOL(wval_is_str(v) ? 1L : 0L));
}
static void w_bool_(void) {
    // VM Bool?: 仅 Bool 值为真（I64 1 不是 bool）——有 WT_BOOL 后精确判定
    wval v = w_pop();
    if (v.t == WT_SIG) v = w_sig_inner(v);
    w_push(W_BOOL(v.t == WT_BOOL ? 1L : 0L));
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
    wval *tmp = (wval *)malloc((count > 0 ? count : 1) * sizeof(wval));
    if (!tmp) return w_list_new(0);
    int64_t kept = 0;
    for (int64_t i = 0; i < count; i++) {
        w_push(list[1 + i]);
        f();
        if (w_as_i64(w_pop()) != 0) tmp[kept++] = list[1 + i];
    }
    wval *result = w_list_new(kept);
    for (int64_t j = 0; j < kept; j++) result[1 + j] = tmp[j];
    free(tmp);
    return result;
}

// `call` (opcode CapExec): invoke the quotation on top of the stack.
//
// A quotation literal is emitted as a raw function pointer — cgen writes
// `w_push(W_PTR((void*)W_qN))`, and the generated body is an ordinary
// `void(void)` function, which is exactly the `wfn` shape the higher-order
// helpers above already consume. So a value that is WT_PTR but NOT inside
// the arena is a code pointer; a string or list lives in the arena and is
// rejected. That heap test is the same heuristic the type predicates use.
//
// The Rust VM reaches the identical behaviour through `Opcode::CapExec`'s
// `Value::Ref(code)` arm. Failure goes through w_fail so that
// `{ ... call } try` catches it, matching the VM's catchable TypeMismatch.
//
// (cgen has always emitted `W_call()` for this — it classifies `call` as a
// word reference — but nothing ever defined the symbol, so `call` compiled
// to a gcc error on the C chain while working on the VM. Defining it here
// keeps compiler.all.ws frozen, exactly as with the `@n` capability ABI.)
static void W_call(void) {
    wval v = w_pop();
    if (v.t == WT_SIG) v = w_sig_inner(v);
    if (v.t != WT_PTR || wval_is_heap(v)) {
        w_fail("call: expected quotation");
    }
    ((wfn)(uintptr_t)v.v)();
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
#ifndef W_WASI
#ifdef _WIN32
typedef SOCKET w_socket_t;
#define W_BAD_SOCKET INVALID_SOCKET
#define w_closesock closesocket
#else
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <unistd.h>
#include <fcntl.h>
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
    w_net_require("listen");
    w_socket_t s = socket(AF_INET, SOCK_STREAM, 0);
    if (s == W_BAD_SOCKET) return W_I64(-1);
    int opt = 1;
    setsockopt(s, SOL_SOCKET, SO_REUSEADDR, (const char *)&opt, sizeof(opt));
    struct sockaddr_in a;
    memset(&a, 0, sizeof(a));
    a.sin_family = AF_INET;
    a.sin_addr.s_addr = htonl(INADDR_ANY);
    a.sin_port = htons((unsigned short)port);
    if (bind(s, (struct sockaddr *)&a, sizeof(a)) != 0) { w_closesock(s); return W_I64(-1); }
    if (listen(s, 16) != 0) { w_closesock(s); return W_I64(-1); }
    return W_I64((int64_t)s);
}

// accept(server) -> conn handle | -1 (blocking)
static wval w_sock_accept(wval srv) {
    w_net_require("accept");
    w_socket_t c = accept((w_socket_t)w_as_i64(srv), NULL, NULL);
    if (c == W_BAD_SOCKET) return W_I64(-1);
    return W_I64((int64_t)c);
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
    w_net_require("recv");
    w_socket_t c = (w_socket_t)w_as_i64(h);
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
    return W_PTR(w_str_new(buf, (uint32_t)len));
}

// send(conn, data) -> bytes sent | -1
static wval w_sock_send(wval h, const char *s) {
    w_net_require("send");
    w_socket_t c = (w_socket_t)w_as_i64(h);
    int n = (int)send(c, s, w_str_len(s), 0);
    return W_I64((int64_t)n);
}

static void w_sock_close(wval h) {
    w_closesock((w_socket_t)w_as_i64(h));
}

// ── HTTP client (@2 http-get / @3 http-post) ────────────────────────
// Mirrors whisper-core/src/capability.rs (parse_url / http_request /
// extract_body) so both chains put the same bytes on the wire and return the
// same body:
//
//   METHOD <path> HTTP/1.1\r\nHost: <host>\r\nConnection: close\r\n
//   [Content-Type: application/json\r\nContent-Length: <n>\r\n]\r\n<body>
//
// and the returned value is everything after the first "\r\n\r\n" (the whole
// response when the terminator is missing).
//
// TLS is NOT implemented here — and it is not implemented in the VM either:
// capability.rs refuses https:// outright rather than sending plaintext to
// 443. We refuse it the same way, with the same reasoning. Writing a TLS
// client is a separate project (docs/independence-design.md, Step 2b).

typedef struct {
    int use_tls;
    char *host;
    int port;
    char *path;
} w_url;

static char *w_dup_range(const char *s, size_t n) {
    char *p = (char *)malloc(n + 1);
    if (!p) return NULL;
    memcpy(p, s, n);
    p[n] = 0;
    return p;
}

// "http://host[:port][/path]" → w_url. Returns 0 on success, -1 on a
// malformed URL. The path always starts with '/', matching the VM's
// `format!("/{path}")`.
static int w_url_parse(const char *url, w_url *out) {
    const char *rest, *slash, *colon = NULL;
    size_t hplen, i;
    out->use_tls = 0;
    out->host = NULL;
    out->path = NULL;
    out->port = 80;
    if (strncmp(url, "https://", 8) == 0) {
        out->use_tls = 1;
        out->port = 443;
        rest = url + 8;
    } else if (strncmp(url, "http://", 7) == 0) {
        rest = url + 7;
    } else {
        return -1;
    }
    slash = strchr(rest, '/');
    hplen = slash ? (size_t)(slash - rest) : strlen(rest);
    if (hplen == 0) return -1;
    // Port is split from the right, same as `host_port.rsplit_once(':')`.
    for (i = 0; i < hplen; i++)
        if (rest[i] == ':') colon = rest + i;
    if (colon) {
        long p = strtol(colon + 1, NULL, 10);
        if (p <= 0 || p > 65535) return -1;
        out->port = (int)p;
        out->host = w_dup_range(rest, (size_t)(colon - rest));
    } else {
        out->host = w_dup_range(rest, hplen);
    }
    if (!out->host) return -1;
    if (slash) {
        out->path = w_dup_range(slash, strlen(slash));
    } else {
        out->path = w_dup_range("/", 1);
    }
    if (!out->path) {
        free(out->host);
        out->host = NULL;
        return -1;
    }
    return 0;
}

static void w_url_free(w_url *u) {
    free(u->host);
    free(u->path);
    u->host = NULL;
    u->path = NULL;
}

static int w_net_private_strict(void) {
    const char *v = getenv("WHISPER_NET_ALLOW_PRIVATE");
    if (v && (strcmp(v, "1") == 0 || strcmp(v, "true") == 0 || strcmp(v, "on") == 0)) return 0;
    v = getenv("WHISPER_NET_DENY_PRIVATE");
    if (v && (strcmp(v, "1") == 0 || strcmp(v, "true") == 0 || strcmp(v, "on") == 0)) return 1;
    // An allowlist is a security boundary: private destinations are denied by
    // default unless the launcher explicitly opts back in.
    v = getenv("WHISPER_NET_ALLOW");
    return v && *v;
}

static int w_ip4_private(uint32_t ip) {
    uint32_t a = ip >> 24, b = (ip >> 16) & 0xff;
    if (a == 10 || a == 127 || a == 0) return 1;
    if (a == 169 && b == 254) return 1;
    if (a == 172 && b >= 16 && b <= 31) return 1;
    if (a == 192 && b == 168) return 1;
    if (a == 100 && b >= 64 && b <= 127) return 1;
    if (a == 198 && (b == 18 || b == 19)) return 1;
    if (a >= 224) return 1;
    return 0;
}

static int w_ip6_private(const unsigned char *b) {
    static const unsigned char loop[16] = {0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1};
    static const unsigned char any[16] = {0};
    if (memcmp(b, loop, 16) == 0 || memcmp(b, any, 16) == 0) return 1;
    if ((b[0] & 0xfe) == 0xfc) return 1;          // fc00::/7 ULA
    if (b[0] == 0xfe && (b[1] & 0xc0) == 0x80) return 1; // fe80::/10
    if (b[0] == 0xff) return 1;                    // multicast
    // IPv4-mapped ::ffff:a.b.c.d
    int mapped = 1;
    for (int i = 0; i < 10; i++) if (b[i] != 0) mapped = 0;
    if (b[10] != 0xff || b[11] != 0xff) mapped = 0;
    if (mapped) {
        uint32_t ip = ((uint32_t)b[12] << 24) | ((uint32_t)b[13] << 16) |
                      ((uint32_t)b[14] << 8) | b[15];
        return w_ip4_private(ip);
    }
    return 0;
}

static int w_addr_private(const struct addrinfo *p) {
    if (p->ai_family == AF_INET) {
        const struct sockaddr_in *s = (const struct sockaddr_in *)p->ai_addr;
        return w_ip4_private(ntohl(s->sin_addr.s_addr));
    }
    if (p->ai_family == AF_INET6) {
        const struct sockaddr_in6 *s = (const struct sockaddr_in6 *)p->ai_addr;
        return w_ip6_private((const unsigned char *)&s->sin6_addr);
    }
    return 1;
}

static int w_host_is_local(const char *host) {
    size_t n = strlen(host);
    if (w_host_ci_eq(host, n, "localhost", 9)) return 1;
    const char *suffixes[] = {".localhost", ".local", ".internal"};
    for (size_t i = 0; i < 3; i++) {
        size_t sn = strlen(suffixes[i]);
        if (n > sn && w_host_ci_eq(host + n - sn, sn, suffixes[i], sn)) return 1;
    }
    return 0;
}

static int w_net_timeout_sec(void) {
    const char *e = getenv("WHISPER_NET_TIMEOUT");
    if (!e || !*e) return 30;
    long v = strtol(e, NULL, 10);
    if (v <= 0) return 0; // 0 disables the socket timeout
    if (v > 300) v = 300;
    return (int)v;
}

static void w_set_socket_timeout(w_socket_t s) {
    int sec = w_net_timeout_sec();
    if (sec == 0) return;
#ifdef _WIN32
    DWORD ms = (DWORD)sec * 1000;
    setsockopt(s, SOL_SOCKET, SO_RCVTIMEO, (const char *)&ms, sizeof(ms));
    setsockopt(s, SOL_SOCKET, SO_SNDTIMEO, (const char *)&ms, sizeof(ms));
#else
    struct timeval tv;
    tv.tv_sec = sec;
    tv.tv_usec = 0;
    setsockopt(s, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
    setsockopt(s, SOL_SOCKET, SO_SNDTIMEO, &tv, sizeof(tv));
#endif
}

static int w_set_nonblocking(w_socket_t fd, int on) {
#ifdef _WIN32
    u_long mode = on ? 1 : 0;
    return ioctlsocket(fd, FIONBIO, &mode) == 0 ? 0 : -1;
#else
    int flags = fcntl(fd, F_GETFL, 0);
    if (flags < 0) return -1;
    if (on) flags |= O_NONBLOCK;
    else flags &= ~O_NONBLOCK;
    return fcntl(fd, F_SETFL, flags) == 0 ? 0 : -1;
#endif
}

static int w_connect_timeout(w_socket_t fd, const struct sockaddr *addr, int len) {
    int sec = w_net_timeout_sec();
    if (sec <= 0) return connect(fd, addr, len);
    if (w_set_nonblocking(fd, 1) != 0) return connect(fd, addr, len);
    int r = connect(fd, addr, len);
    if (r == 0) {
        w_set_nonblocking(fd, 0);
        return 0;
    }
#ifdef _WIN32
    int pending = WSAGetLastError() == WSAEWOULDBLOCK ||
                  WSAGetLastError() == WSAEINPROGRESS;
#else
    int pending = errno == EINPROGRESS;
#endif
    if (!pending) {
        w_set_nonblocking(fd, 0);
        return -1;
    }
    fd_set wf;
    FD_ZERO(&wf);
    FD_SET(fd, &wf);
    struct timeval tv;
    tv.tv_sec = sec;
    tv.tv_usec = 0;
#ifdef _WIN32
    int sel = select(0, NULL, &wf, NULL, &tv);
#else
    int sel = select(fd + 1, NULL, &wf, NULL, &tv);
#endif
    w_set_nonblocking(fd, 0);
    if (sel <= 0) return -1;
    int soerr = 0;
#ifdef _WIN32
    int elen = sizeof(soerr);
#else
    socklen_t elen = sizeof(soerr);
#endif
    if (getsockopt(fd, SOL_SOCKET, SO_ERROR, (char *)&soerr, &elen) != 0) return -1;
    return soerr == 0 ? 0 : -1;
}

static w_socket_t w_tcp_connect(const char *host, int port, char *err, size_t errlen) {
    struct addrinfo hints, *res = NULL, *p;
    char portstr[16];
    w_socket_t fd = W_BAD_SOCKET;
    if (err && errlen) err[0] = 0;
    int strict = w_net_private_strict();
    if (strict && w_host_is_local(host)) {
        snprintf(err, errlen, "destination %s is local/private (WHISPER_NET_ALLOW_PRIVATE=1 to override)", host);
        return W_BAD_SOCKET;
    }
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    snprintf(portstr, sizeof(portstr), "%d", port);
    if (getaddrinfo(host, portstr, &hints, &res) != 0) {
        snprintf(err, errlen, "DNS resolve failed: %s", host);
        return W_BAD_SOCKET;
    }
    if (strict) {
        for (p = res; p; p = p->ai_next) {
            if (w_addr_private(p)) {
                snprintf(err, errlen, "destination %s resolves to a private/link-local address", host);
                freeaddrinfo(res);
                return W_BAD_SOCKET;
            }
        }
    }
    for (p = res; p; p = p->ai_next) {
        fd = socket(p->ai_family, p->ai_socktype, p->ai_protocol);
        if (fd == W_BAD_SOCKET) continue;
        if (w_connect_timeout(fd, p->ai_addr, (int)p->ai_addrlen) == 0) {
            w_set_socket_timeout(fd);
            break;
        }
        w_closesock(fd);
        fd = W_BAD_SOCKET;
    }
    freeaddrinfo(res);
    if (fd == W_BAD_SOCKET) snprintf(err, errlen, "Connect to %s:%d failed", host, port);
    return fd;
}

// Read until the peer closes (the request asks for `Connection: close`, and
// the VM's read_to_string behaves the same). Returns a malloc'd NUL-terminated
// buffer, or NULL with `err` filled in.
static char *w_http_read_all(w_socket_t s, char *err, size_t errlen, size_t max) {
    size_t cap = 65536, len = 0;
    if (max == 0) max = 16 * 1024 * 1024;
    if (cap > max) cap = max;
    if (cap == 0) cap = 1;
    char *buf = (char *)malloc(cap);
    if (!buf) {
        snprintf(err, errlen, "out of memory");
        return NULL;
    }
    for (;;) {
        int n;
        if (len + 1 >= cap) {
            char *nb;
            if (cap >= max) {
                snprintf(err, errlen, "response exceeds WHISPER_NET_MAX_RESPONSE");
                free(buf);
                return NULL;
            }
            size_t ncap = cap * 2;
            if (ncap > max) ncap = max;
            nb = (char *)realloc(buf, ncap);
            if (!nb) {
                snprintf(err, errlen, "out of memory");
                free(buf);
                return NULL;
            }
            cap = ncap;
            buf = nb;
        }
        n = (int)recv(s, buf + len, (int)(cap - 1 - len), 0);
        if (n == 0) break;
        if (n < 0) {
#ifdef _WIN32
            int e = WSAGetLastError();
            if (e == WSAETIMEDOUT || e == WSAEWOULDBLOCK) {
                free(buf); snprintf(err, errlen, "network read timeout"); return NULL;
            }
#else
            if (errno == EAGAIN || errno == EWOULDBLOCK) {
                free(buf); snprintf(err, errlen, "network read timeout"); return NULL;
            }
            if (errno == EINTR) continue;
#endif
            break;
        }
        len += (size_t)n;
    }
    buf[len] = 0;
    return buf;
}

// GET/POST → response body. Returns a malloc'd string, or NULL with `err`
// filled in.
static char *w_http_request(const char *method, const char *url, const char *body,
                            char *err, size_t errlen) {
    w_net_require("http");
    w_url u;
    w_socket_t s;
    char *req = NULL, *resp = NULL, *out = NULL;
    size_t need, bodylen = strlen(body);
    int sent = 0, off = 0;
    if (w_url_parse(url, &u) != 0) {
        snprintf(err, errlen, "Invalid URL: %s", url);
        return NULL;
    }
    if (w_net_allow_guard(u.host, err, errlen) != 0) {
        w_url_free(&u);
        return NULL;
    }
    if (!w_method_allowed(method)) {
        snprintf(err, errlen, "method not allowed by WHISPER_NET_METHODS: %s", method);
        w_url_free(&u);
        return NULL;
    }
    if (!w_port_allowed(u.port)) {
        snprintf(err, errlen, "port not allowed by WHISPER_NET_PORTS: %d", u.port);
        w_url_free(&u);
        return NULL;
    }
    if (u.use_tls) {
        snprintf(err, errlen,
                 "HTTPS is not yet supported by @2/@3 (http-get/http-post). Use an http:// URL, "
                 "or @6 (http-req), which does speak TLS. Plain TCP to port 443 cannot carry TLS "
                 "traffic, so refusing is the only safe answer.");
        w_url_free(&u);
        return NULL;
    }
    s = w_tcp_connect(u.host, u.port, err, errlen);
    if (s == W_BAD_SOCKET) {
        w_url_free(&u);
        return NULL;
    }
    need = strlen(method) + strlen(u.path) + strlen(u.host) + bodylen + 256;
    req = (char *)malloc(need);
    if (!req) {
        snprintf(err, errlen, "out of memory");
        w_closesock(s);
        w_url_free(&u);
        return NULL;
    }
    if (bodylen == 0) {
        snprintf(req, need, "%s %s HTTP/1.1\r\nHost: %s\r\nConnection: close\r\n\r\n",
                 method, u.path, u.host);
    } else {
        snprintf(req, need,
                 "%s %s HTTP/1.1\r\nHost: %s\r\nConnection: close\r\n"
                 "Content-Type: application/json\r\nContent-Length: %lu\r\n\r\n%s",
                 method, u.path, u.host, (unsigned long)bodylen, body);
    }
    w_url_free(&u);
    {
        size_t reqlen = strlen(req);
        while (off < (int)reqlen) {
            sent = (int)send(s, req + off, (int)(reqlen - (size_t)off), 0);
            if (sent <= 0) break;
            off += sent;
        }
        if (off < (int)reqlen) {
            snprintf(err, errlen, "send failed");
            free(req);
            w_closesock(s);
            return NULL;
        }
    }
    free(req);
    resp = w_http_read_all(s, err, errlen, w_response_max());
    w_closesock(s);
    if (!resp) return NULL;
    {
        char *sep = strstr(resp, "\r\n\r\n");
        out = w_dup_range(sep ? sep + 4 : resp, strlen(sep ? sep + 4 : resp));
    }
    free(resp);
    if (!out) snprintf(err, errlen, "out of memory");
    return out;
}

// ── @6: native HTTP(S) session (OPTIONAL dynamic OpenSSL) ─────────────
// No link-time OpenSSL dependency: ordinary builds still work without it.
// If libssl is absent or lacks verification symbols, @6 fails explicitly.
typedef void *(*w_tls_method_fn)(void);
typedef void *(*w_tls_ctx_new_fn)(void *);
typedef void (*w_tls_ctx_free_fn)(void *);
typedef void (*w_tls_ctx_verify_fn)(void *, int, void *);
typedef int (*w_tls_ctx_paths_fn)(void *);
typedef int (*w_tls_ctx_ca_fn)(void *, const char *, const char *);
typedef void *(*w_tls_ssl_new_fn)(void *);
typedef void (*w_tls_ssl_free_fn)(void *);
typedef int (*w_tls_ssl_fd_fn)(void *, int);
typedef long (*w_tls_ctrl_fn)(void *, int, long, void *);
typedef int (*w_tls_connect_fn)(void *);
typedef int (*w_tls_write_fn)(void *, const void *, int);
typedef int (*w_tls_read_fn)(void *, void *, int);
typedef int (*w_tls_error_fn)(void *, int);
typedef int (*w_tls_shutdown_fn)(void *);
typedef int (*w_tls_set1_host_fn)(void *, const char *);

typedef struct {
    void *lib, *ctx, *ssl;
    w_tls_ctx_free_fn ctx_free;
    w_tls_ssl_free_fn ssl_free;
    w_tls_write_fn write;
    w_tls_read_fn read;
    w_tls_error_fn get_error;
} w_tls;

static int w_tls_load(w_tls *t, char *err, size_t errlen) {
    memset(t, 0, sizeof(*t));
#ifdef _WIN32
    const char *names[] = {"libssl-3-x64.dll", "libssl-3.dll",
                           "libssl-1_1-x64.dll", "libssl-1_1.dll", "libssl.dll"};
    for (size_t i = 0; i < sizeof(names)/sizeof(names[0]) && !t->lib; i++)
        t->lib = (void *)LoadLibraryA(names[i]);
#else
#ifdef __APPLE__
    const char *names[] = {"libssl.3.dylib", "libssl.dylib",
                           "/opt/homebrew/opt/openssl@3/lib/libssl.3.dylib",
                           "/usr/local/opt/openssl@3/lib/libssl.3.dylib"};
#else
    const char *names[] = {"libssl.so.3", "libssl.so.1.1", "libssl.so"};
#endif
    for (size_t i = 0; i < sizeof(names)/sizeof(names[0]) && !t->lib; i++)
        t->lib = dlopen(names[i], RTLD_NOW);
#endif
    if (!t->lib) {
        snprintf(err, errlen, "HTTPS unavailable: cannot load libssl");
        return -1;
    }
#ifdef _WIN32
#define W_TLS_SYM(n) ((void *)GetProcAddress((HMODULE)t->lib, n))
#else
#define W_TLS_SYM(n) dlsym(t->lib, n)
#endif
    w_tls_method_fn method = (w_tls_method_fn)W_TLS_SYM("TLS_client_method");
    w_tls_ctx_new_fn ctx_new = (w_tls_ctx_new_fn)W_TLS_SYM("SSL_CTX_new");
    w_tls_ctx_verify_fn ctx_verify = (w_tls_ctx_verify_fn)W_TLS_SYM("SSL_CTX_set_verify");
    w_tls_ctx_paths_fn ctx_paths = (w_tls_ctx_paths_fn)W_TLS_SYM("SSL_CTX_set_default_verify_paths");
    w_tls_ctx_ca_fn ctx_ca = (w_tls_ctx_ca_fn)W_TLS_SYM("SSL_CTX_load_verify_locations");
    w_tls_ssl_new_fn ssl_new = (w_tls_ssl_new_fn)W_TLS_SYM("SSL_new");
    w_tls_ssl_fd_fn ssl_fd = (w_tls_ssl_fd_fn)W_TLS_SYM("SSL_set_fd");
    w_tls_ctrl_fn ctrl = (w_tls_ctrl_fn)W_TLS_SYM("SSL_ctrl");
    w_tls_connect_fn connect = (w_tls_connect_fn)W_TLS_SYM("SSL_connect");
    w_tls_set1_host_fn set1_host = (w_tls_set1_host_fn)W_TLS_SYM("SSL_set1_host");
    t->ctx_free = (w_tls_ctx_free_fn)W_TLS_SYM("SSL_CTX_free");
    t->ssl_free = (w_tls_ssl_free_fn)W_TLS_SYM("SSL_free");
    t->write = (w_tls_write_fn)W_TLS_SYM("SSL_write");
    t->read = (w_tls_read_fn)W_TLS_SYM("SSL_read");
    t->get_error = (w_tls_error_fn)W_TLS_SYM("SSL_get_error");
#undef W_TLS_SYM
    if (!method || !ctx_new || !ctx_verify || !ssl_new || !ssl_fd || !ctrl ||
        !connect || !set1_host || !t->ctx_free || !t->ssl_free ||
        !t->write || !t->read || !t->get_error) {
        snprintf(err, errlen, "HTTPS unavailable: libssl is missing required symbols");
        return -1;
    }
    t->ctx = ctx_new(method());
    if (!t->ctx) { snprintf(err, errlen, "HTTPS unavailable: SSL_CTX_new failed"); return -1; }
    ctx_verify(t->ctx, 1 /*SSL_VERIFY_PEER*/, NULL);
    const char *ca = getenv("WHISPER_TLS_CA");
    if (ca && *ca) {
        if (!ctx_ca || ctx_ca(t->ctx, ca, NULL) != 1) {
            snprintf(err, errlen, "HTTPS: cannot load WHISPER_TLS_CA=%s", ca);
            return -1;
        }
    } else if (!ctx_paths || ctx_paths(t->ctx) != 1) {
        snprintf(err, errlen, "HTTPS: no system CA store (set WHISPER_TLS_CA)");
        return -1;
    }
    return 0;
}

static void w_tls_destroy(w_tls *t) {
    if (t->ssl && t->ssl_free) t->ssl_free(t->ssl);
    if (t->ctx && t->ctx_free) t->ctx_free(t->ctx);
    t->ssl = t->ctx = NULL;
}

static int w_tls_connect(w_tls *t, const char *host, w_socket_t s, char *err, size_t errlen) {
#ifdef _WIN32
#define W_TLS_SYM(n) ((void *)GetProcAddress((HMODULE)t->lib, n))
#else
#define W_TLS_SYM(n) dlsym(t->lib, n)
#endif
    w_tls_ssl_new_fn ssl_new = (w_tls_ssl_new_fn)W_TLS_SYM("SSL_new");
    w_tls_ssl_fd_fn ssl_fd = (w_tls_ssl_fd_fn)W_TLS_SYM("SSL_set_fd");
    w_tls_ctrl_fn ctrl = (w_tls_ctrl_fn)W_TLS_SYM("SSL_ctrl");
    w_tls_connect_fn connect = (w_tls_connect_fn)W_TLS_SYM("SSL_connect");
    w_tls_set1_host_fn set1_host = (w_tls_set1_host_fn)W_TLS_SYM("SSL_set1_host");
#undef W_TLS_SYM
    if (!ssl_new || !ssl_fd || !ctrl || !connect || !set1_host) return -1;
    t->ssl = ssl_new(t->ctx);
    if (!t->ssl) { snprintf(err, errlen, "HTTPS: SSL_new failed"); return -1; }
    if (ssl_fd(t->ssl, (int)s) != 1) { snprintf(err, errlen, "HTTPS: SSL_set_fd failed"); return -1; }
    if (ctrl(t->ssl, 55 /*SSL_CTRL_SET_TLSEXT_HOSTNAME*/,
             0 /*TLSEXT_NAMETYPE_host_name*/, (void *)host) != 1) {
        snprintf(err, errlen, "HTTPS: SNI setup failed"); return -1;
    }
    if (set1_host(t->ssl, host) != 1) {
        snprintf(err, errlen, "HTTPS: hostname verification setup failed"); return -1;
    }
    if (connect(t->ssl) != 1) {
        snprintf(err, errlen, "HTTPS: TLS handshake failed (certificate/hostname?)");
        return -1;
    }
    return 0;
}

static char *w_tls_read_all(w_tls *t, char *err, size_t errlen, size_t max) {
    size_t cap = 65536, len = 0;
    if (max == 0) max = 16 * 1024 * 1024;
    if (cap > max) cap = max;
    if (cap == 0) cap = 1;
    char *buf = (char *)malloc(cap);
    if (!buf) { snprintf(err, errlen, "out of memory"); return NULL; }
    for (;;) {
        if (len + 1 >= cap) {
            if (cap >= max) {
                free(buf);
                snprintf(err, errlen, "response exceeds WHISPER_NET_MAX_RESPONSE");
                return NULL;
            }
            size_t ncap = cap * 2;
            if (ncap > max) ncap = max;
            char *nb = (char *)realloc(buf, ncap);
            if (!nb) { free(buf); snprintf(err, errlen, "out of memory"); return NULL; }
            cap = ncap;
            buf = nb;
        }
        int want = (int)(cap - 1 - len);
        if (want > 16384) want = 16384;
        int n = t->read(t->ssl, buf + len, want);
        if (n > 0) { len += (size_t)n; continue; }
        if (n == 0) break;
        int e = t->get_error ? t->get_error(t->ssl, n) : -1;
        if (e == 2 /*SSL_ERROR_WANT_READ*/ || e == 3 /*WANT_WRITE*/) {
            free(buf);
            snprintf(err, errlen, "network read timeout");
            return NULL;
        }
        free(buf);
        snprintf(err, errlen, "HTTPS: TLS read failed");
        return NULL;
    }
    buf[len] = 0;
    return buf;
}

static char *w_tls_json_escape_str(const char *s) {
    size_t n = strlen(s), cap = n * 6 + 1, k = 0;
    char *out = (char *)malloc(cap);
    if (!out) return NULL;
    for (size_t i = 0; i < n; i++) {
        unsigned char c = (unsigned char)s[i];
        if (c == '"' || c == '\\') { out[k++] = '\\'; out[k++] = (char)c; }
        else if (c == '\n') { out[k++] = '\\'; out[k++] = 'n'; }
        else if (c == '\r') { out[k++] = '\\'; out[k++] = 'r'; }
        else if (c < 0x20) { out[k++] = '?'; }
        else out[k++] = (char)c;
    }
    out[k] = 0;
    return out;
}

static char *w_headers_json(const char *p, size_t n) {
    size_t cap = n * 2 + 4, k = 1;
    char *out = (char *)malloc(cap);
    if (!out) return NULL;
    out[0] = '{'; out[1] = 0;
    const char *line = p, *end = p + n;
    while (line < end) {
        const char *e = line;
        while (e < end && *e != '\n') e++;
        const char *colon = line;
        while (colon < e && *colon != ':') colon++;
        if (colon < e) {
            const char *a = line, *b = colon;
            while (a < b && (*a == ' ' || *a == '\t')) a++;
            while (b > a && (b[-1] == ' ' || b[-1] == '\t' || b[-1] == '\r')) b--;
            const char *v = colon + 1, *ve = e;
            while (v < ve && (*v == ' ' || *v == '\t')) v++;
            while (ve > v && (ve[-1] == ' ' || ve[-1] == '\t' || ve[-1] == '\r')) ve--;
            char *key = w_dup_range(a, (size_t)(b - a));
            char *val = w_dup_range(v, (size_t)(ve - v));
            char *ke = key ? w_tls_json_escape_str(key) : NULL;
            char *ve2 = val ? w_tls_json_escape_str(val) : NULL;
            if (ke && ve2) {
                size_t need = k + strlen(ke) + strlen(ve2) + 9;
                if (need > cap) {
                    cap = need * 2;
                    char *nb = (char *)realloc(out, cap);
                    if (!nb) { free(ke); free(ve2); free(key); free(val); free(out); return NULL; }
                    out = nb;
                }
                k += (size_t)sprintf(out + k, "%s\"%s\":\"%s\"",
                                     k > 1 ? "," : "", ke, ve2);
            }
            free(ke); free(ve2); free(key); free(val);
        }
        line = e < end ? e + 1 : end;
    }
    out[k++] = '}'; out[k] = 0;
    return out;
}

static int w_ci_eq_n(const char *a, const char *b, size_t n) {
    for (size_t i = 0; i < n; i++) {
        unsigned char x = (unsigned char)a[i], y = (unsigned char)b[i];
        if (x >= 'A' && x <= 'Z') x = (unsigned char)(x + 32);
        if (y >= 'A' && y <= 'Z') y = (unsigned char)(y + 32);
        if (x != y) return 0;
    }
    return 1;
}

// Parse a flat JSON object of string headers. Host/Content-Length/Cookie are
// managed by the runtime; values containing CR/LF are rejected (injection).
static char *w_user_headers(const char *json, char *err, size_t errlen) {
    size_t cap = 256, len = 0;
    char *out = (char *)malloc(cap);
    if (!out) { snprintf(err, errlen, "out of memory"); return NULL; }
    out[0] = 0;
    const char *p = json;
    if (!p || *p != '{') { snprintf(err, errlen, "headers-json must be an object"); free(out); return NULL; }
    p++;
    for (;;) {
        while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') p++;
        if (*p == '}') break;
        if (*p != '"') { snprintf(err, errlen, "bad headers-json key"); free(out); return NULL; }
        p++;
        const char *ks = p;
        while (*p && *p != '"') p++;
        if (!*p) { snprintf(err, errlen, "unterminated headers-json key"); free(out); return NULL; }
        size_t klen = (size_t)(p - ks);
        p++;
        while (*p == ' ' || *p == '\t') p++;
        if (*p != ':') { snprintf(err, errlen, "bad headers-json separator"); free(out); return NULL; }
        p++;
        while (*p == ' ' || *p == '\t') p++;
        if (*p != '"') { snprintf(err, errlen, "headers-json values must be strings"); free(out); return NULL; }
        p++;
        const char *vs = p;
        while (*p && *p != '"') {
            if (*p == '\\' && p[1]) p += 2;
            else if (*p == '\r' || *p == '\n') {
                snprintf(err, errlen, "headers-json value contains CR/LF"); free(out); return NULL;
            } else p++;
        }
        if (!*p) { snprintf(err, errlen, "unterminated headers-json value"); free(out); return NULL; }
        size_t vlen = (size_t)(p - vs);
        p++;
        int skip = w_ci_eq_n(ks, "host", klen) || w_ci_eq_n(ks, "content-length", klen) ||
                   w_ci_eq_n(ks, "cookie", klen);
        if (!skip) {
            size_t need = len + klen + vlen + 5;
            if (need > cap) {
                cap = need * 2;
                char *nb = (char *)realloc(out, cap);
                if (!nb) { snprintf(err, errlen, "out of memory"); free(out); return NULL; }
                out = nb;
            }
            memcpy(out + len, ks, klen); len += klen;
            out[len++] = ':'; out[len++] = ' ';
            memcpy(out + len, vs, vlen); len += vlen;
            out[len++] = '\r'; out[len++] = '\n'; out[len] = 0;
        }
        while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') p++;
        if (*p == ',') { p++; continue; }
        if (*p == '}') break;
        snprintf(err, errlen, "bad headers-json object"); free(out); return NULL;
    }
    return out;
}

static char *w_cookie_jar = NULL; // process-level @6 cookie jar, "k=v; ..."
static char *w_audit_log = NULL;  // process-level @7 audit, one line per request
static char *w_audit_json = NULL; // entries (no surrounding brackets) for JSON mode

static void w_audit_add(const char *method, const char *url, const char *host,
                        long status, size_t bytes) {
    w_net_global_lock();
    char line[1024];
    snprintf(line, sizeof(line), "%s %s %ld\n", method, url, status);
    size_t n = (w_audit_log ? strlen(w_audit_log) : 0) + strlen(line) + 1;
    char *p = (char *)realloc(w_audit_log, n);
    if (p) {
        if (!w_audit_log) p[0] = 0;
        w_audit_log = p;
        strcat(w_audit_log, line);
    }
    char *em = w_tls_json_escape_str(method);
    char *eu = w_tls_json_escape_str(url);
    char *eh = w_tls_json_escape_str(host);
    if (!em || !eu || !eh) {
        free(em); free(eu); free(eh);
        w_net_global_unlock();
        return;
    }
    size_t need = strlen(em) + strlen(eu) + strlen(eh) + 128;
    char *entry = (char *)malloc(need);
    if (entry) {
        snprintf(entry, need,
                 "{\"method\":\"%s\",\"url\":\"%s\",\"host\":\"%s\","
                 "\"status\":%ld,\"bytes\":%lu}",
                 em, eu, eh, status, (unsigned long)bytes);
        size_t jn = (w_audit_json ? strlen(w_audit_json) : 0) + strlen(entry) + 2;
        char *jp = (char *)realloc(w_audit_json, jn);
        if (jp) {
            if (!w_audit_json) jp[0] = 0;
            w_audit_json = jp;
            if (w_audit_json[0]) strcat(w_audit_json, ",");
            strcat(w_audit_json, entry);
        }
        free(entry);
    }
    free(em); free(eu); free(eh);
    w_net_global_unlock();
}

static void w_jar_update(const char *resp, size_t n) {
    w_net_global_lock();
    const char *line = resp, *end = resp + n;
    while (line < end) {
        const char *e = line;
        while (e < end && *e != '\n') e++;
        if ((size_t)(e - line) > 11 && w_ci_eq_n(line, "set-cookie:", 11)) {
            const char *v = line + 11, *ve = e;
            while (v < ve && (*v == ' ' || *v == '\t')) v++;
            const char *semi = v;
            while (semi < ve && *semi != ';') semi++;
            while (semi > v && (semi[-1] == '\r' || semi[-1] == ' ' || semi[-1] == '\t')) semi--;
            const char *eq = v;
            while (eq < semi && *eq != '=') eq++;
            if (eq < semi) {
                size_t nlen = (size_t)(eq - v), vlen = (size_t)(semi - eq - 1);
                char *newjar = (char *)malloc((w_cookie_jar ? strlen(w_cookie_jar) : 0) + nlen + vlen + 4);
                if (newjar) {
                    size_t k = 0;
                    if (w_cookie_jar && *w_cookie_jar) {
                        const char *q = w_cookie_jar;
                        while (*q) {
                            const char *qe = strchr(q, ';');
                            size_t qn = qe ? (size_t)(qe - q) : strlen(q);
                            const char *qeq = memchr(q, '=', qn);
                            int same = qeq && (size_t)(qeq - q) == nlen && w_ci_eq_n(q, v, nlen);
                            if (!same && qn > 0) {
                                if (k) newjar[k++] = ';'; newjar[k++] = ' ';
                                memcpy(newjar + k, q, qn); k += qn;
                            }
                            if (!qe) break;
                            q = qe + 1;
                            while (*q == ' ') q++;
                        }
                    }
                    if (k) newjar[k++] = ';'; newjar[k++] = ' ';
                    memcpy(newjar + k, v, nlen); k += nlen;
                    newjar[k++] = '=';
                    memcpy(newjar + k, eq + 1, vlen); k += vlen;
                    newjar[k] = 0;
                    free(w_cookie_jar);
                    w_cookie_jar = newjar;
                }
            }
        }
        line = e < end ? e + 1 : end;
    }
    w_net_global_unlock();
}

static char *w_jar_header(void) {
    w_net_global_lock();
    if (!w_cookie_jar || !*w_cookie_jar) {
        char *e = (char *)malloc(1); if (e) e[0] = 0;
        w_net_global_unlock();
        return e;
    }
    size_t n = strlen(w_cookie_jar);
    char *out = (char *)malloc(n + 12);
    if (!out) { w_net_global_unlock(); return NULL; }
    sprintf(out, "Cookie: %s\r\n", w_cookie_jar);
    w_net_global_unlock();
    return out;
}

typedef struct { long status; char *headers; char *body; } w_http_session_resp;

static int w_ci_contains(const char *hay, const char *needle) {
    size_t n = strlen(needle);
    for (const char *p = hay; *p; p++) {
        size_t i = 0;
        while (i < n && p[i] && (p[i] | 0x20) == (needle[i] | 0x20)) i++;
        if (i == n) return 1;
    }
    return 0;
}

// Decode HTTP/1.1 chunked transfer coding. Returns NULL for malformed input.
static char *w_dechunk(const char *s, size_t *outlen) {
    size_t cap = 1024, n = 0;
    char *out = (char *)malloc(cap);
    if (!out) return NULL;
    const char *p = s;
    for (;;) {
        char *end = NULL;
        unsigned long sz = strtoul(p, &end, 16);
        if (end == p) { free(out); return NULL; }
        p = end;
        while (*p == ';') { while (*p && *p != '\r' && *p != '\n') p++; }
        while (*p == '\r' || *p == '\n') p++;
        if (sz == 0) break;
        if (strlen(p) < sz) { free(out); return NULL; }
        if (n + sz + 1 > cap) {
            cap = (n + sz + 1) * 2;
            char *nb = (char *)realloc(out, cap);
            if (!nb) { free(out); return NULL; }
            out = nb;
        }
        memcpy(out + n, p, sz);
        n += sz;
        p += sz;
        while (*p == '\r' || *p == '\n') p++;
    }
    out[n] = 0;
    *outlen = n;
    return out;
}

static int w_hex_decode(const char *s, unsigned char **out, size_t *outlen,
                        char *err, size_t errlen) {
    size_t n = strlen(s);
    if (n % 2) { snprintf(err, errlen, "http-req-raw: odd hex length"); return -1; }
    unsigned char *p = (unsigned char *)malloc(n / 2 + 1);
    if (!p) { snprintf(err, errlen, "out of memory"); return -1; }
    for (size_t i = 0; i < n; i += 2) {
        int hi = s[i] >= '0' && s[i] <= '9' ? s[i] - '0' :
                 s[i] >= 'a' && s[i] <= 'f' ? s[i] - 'a' + 10 :
                 s[i] >= 'A' && s[i] <= 'F' ? s[i] - 'A' + 10 : -1;
        int lo = s[i+1] >= '0' && s[i+1] <= '9' ? s[i+1] - '0' :
                 s[i+1] >= 'a' && s[i+1] <= 'f' ? s[i+1] - 'a' + 10 :
                 s[i+1] >= 'A' && s[i+1] <= 'F' ? s[i+1] - 'A' + 10 : -1;
        if (hi < 0 || lo < 0) { free(p); snprintf(err, errlen, "http-req-raw: bad hex"); return -1; }
        p[i/2] = (unsigned char)((hi << 4) | lo);
    }
    p[n/2] = 0;
    *out = p;
    *outlen = n / 2;
    return 0;
}

static int w_http_session(const char *method, const char *url, const char *headers_json,
                          const char *body, int raw_body, w_http_session_resp *out,
                          char *err, size_t errlen) {
    w_net_require("http-session");
    w_url u;
    if (w_url_parse(url, &u) != 0) { snprintf(err, errlen, "Invalid URL: %s", url); return -1; }
    if (w_net_allow_guard(u.host, err, errlen) != 0) { w_url_free(&u); return -1; }
    if (!w_method_allowed(method)) {
        snprintf(err, errlen, "method not allowed by WHISPER_NET_METHODS: %s", method);
        w_url_free(&u); return -1;
    }
    if (!w_port_allowed(u.port)) {
        snprintf(err, errlen, "port not allowed by WHISPER_NET_PORTS: %d", u.port);
        w_url_free(&u); return -1;
    }
    char *uh = w_user_headers(headers_json ? headers_json : "{}", err, errlen);
    if (!uh) { w_url_free(&u); return -1; }
    char *ck = w_jar_header();
    if (!ck) { free(uh); w_url_free(&u); snprintf(err, errlen, "out of memory"); return -1; }
    unsigned char *raw = NULL;
    size_t bodylen = raw_body ? 0 : strlen(body);
    if (raw_body && w_hex_decode(body, &raw, &bodylen, err, errlen) != 0) {
        free(uh); free(ck); w_url_free(&u); return -1;
    }
    w_socket_t s = w_tcp_connect(u.host, u.port, err, errlen);
    if (s == W_BAD_SOCKET) {
        free(uh); free(ck); free(raw); w_url_free(&u); return -1;
    }
    w_tls tls;
    int use_tls = u.use_tls;
    if (use_tls) {
        if (w_tls_load(&tls, err, errlen) != 0) {
            free(uh); free(ck); free(raw); w_closesock(s); w_url_free(&u); return -1;
        }
        if (w_tls_connect(&tls, u.host, s, err, errlen) != 0) {
            w_tls_destroy(&tls); free(uh); free(ck); free(raw); w_closesock(s); w_url_free(&u); return -1;
        }
    }
    size_t need = strlen(method) + strlen(u.path) + strlen(u.host) + bodylen +
                  strlen(uh) + strlen(ck) + 256;
    char *req = (char *)malloc(need);
    if (!req) {
        snprintf(err, errlen, "out of memory");
        if (use_tls) w_tls_destroy(&tls);
        free(uh); free(ck); free(raw); w_closesock(s); w_url_free(&u); return -1;
    }
    if (raw_body)
        snprintf(req, need, "%s %s HTTP/1.1\r\nHost: %s\r\nConnection: close\r\n"
                 "Content-Length: %lu\r\n%s%s\r\n",
                 method, u.path, u.host, (unsigned long)bodylen, uh, ck);
    else if (bodylen == 0)
        snprintf(req, need, "%s %s HTTP/1.1\r\nHost: %s\r\nConnection: close\r\n%s%s\r\n",
                 method, u.path, u.host, uh, ck);
    else
        snprintf(req, need, "%s %s HTTP/1.1\r\nHost: %s\r\nConnection: close\r\n"
                 "Content-Type: application/json\r\nContent-Length: %lu\r\n%s%s\r\n%s",
                 method, u.path, u.host, (unsigned long)bodylen, uh, ck, body);
    free(uh); free(ck);
    size_t reqlen = strlen(req);
    int off = 0;
    while (off < (int)reqlen) {
        int n = use_tls ? tls.write(tls.ssl, req + off, (int)(reqlen - (size_t)off))
                        : (int)send(s, req + off, (int)(reqlen - (size_t)off), 0);
        if (n <= 0) break;
        off += n;
    }
    free(req);
    if (off == (int)reqlen && raw_body && bodylen > 0) {
        size_t woff = 0;
        while (woff < bodylen) {
            int n = use_tls ? tls.write(tls.ssl, raw + woff, (int)(bodylen - woff))
                            : (int)send(s, (const char *)raw + woff, (int)(bodylen - woff), 0);
            if (n <= 0) break;
            woff += (size_t)n;
        }
        if (woff < bodylen) off = 0;
        else off = (int)reqlen;
    }
    free(raw);
    if (off < (int)reqlen) {
        snprintf(err, errlen, "send failed");
        if (use_tls) w_tls_destroy(&tls);
        w_closesock(s); w_url_free(&u); return -1;
    }
    char *resp = use_tls ? w_tls_read_all(&tls, err, errlen, w_response_max())
                         : w_http_read_all(s, err, errlen, w_response_max());
    char *audit_host = w_dup_range(u.host, strlen(u.host));
    if (use_tls) w_tls_destroy(&tls);
    w_closesock(s); w_url_free(&u);
    if (!resp) { free(audit_host); return -1; }
    char *hdr_end = strstr(resp, "\r\n\r\n");
    if (!hdr_end) { free(resp); free(audit_host); snprintf(err, errlen, "invalid HTTP response"); return -1; }
    w_jar_update(resp, (size_t)(hdr_end - resp));
    long status = 0;
    if (strncmp(resp, "HTTP/", 5) == 0) {
        char *sp = strchr(resp, ' ');
        if (sp) status = strtol(sp + 1, NULL, 10);
    }
    out->headers = w_headers_json(resp, (size_t)(hdr_end - resp));
    out->body = w_dup_range(hdr_end + 4, strlen(hdr_end + 4));
    out->status = status;
    if (out->headers && (status == 301 || status == 302 || status == 303 ||
                         status == 307 || status == 308) &&
        w_ci_contains(out->headers, "\"location\":")) {
        const char *rm = getenv("WHISPER_NET_REDIRECT");
        if (rm && strcmp(rm, "follow") == 0) {
            snprintf(err, errlen, "redirect follow is not implemented; use manual/deny");
            free(out->headers); free(out->body); free(resp);
            free(audit_host);
            return -1;
        }
        if (rm && strcmp(rm, "deny") == 0) {
            snprintf(err, errlen, "redirect denied by WHISPER_NET_REDIRECT=deny");
            free(out->headers); free(out->body); free(resp);
            free(audit_host);
            return -1;
        }
    }
    if (out->headers && w_ci_contains(out->headers, "chunked")) {
        size_t blen = 0;
        char *dec = w_dechunk(out->body ? out->body : "", &blen);
        if (dec) {
            (void)blen;
            free(out->body);
            out->body = dec;
        }
    }
    w_audit_add(method, url, audit_host ? audit_host : "",
                status, out->body ? strlen(out->body) : 0);
    free(audit_host);
    free(resp);
    if (!out->headers || !out->body) { snprintf(err, errlen, "out of memory"); return -1; }
    return 0;
}

#endif // !W_WASI

#ifdef W_WASI
// Socket/server words compile (so the module links) but fail explicitly at
// runtime: WASI Preview 1 has no socket API, and v1 does not provide a host
// network import.
static wval w_sock_listen(int64_t port) {
    (void)port;
    w_fail("listen is unsupported on wasm profile (WASI P1 has no socket API)");
    return W_I64(-1);
}
static wval w_sock_accept(wval srv) {
    (void)srv;
    w_fail("accept is unsupported on wasm profile (WASI P1 has no socket API)");
    return W_I64(-1);
}
static wval w_sock_recv(wval h) {
    (void)h;
    w_fail("recv is unsupported on wasm profile (WASI P1 has no socket API)");
    return W_I64(-1);
}
static wval w_sock_send(wval h, const char *s) {
    (void)h; (void)s;
    w_fail("send is unsupported on wasm profile (WASI P1 has no socket API)");
    return W_I64(-1);
}
static void w_sock_close(wval h) {
    (void)h;
    w_fail("close is unsupported on wasm profile (WASI P1 has no socket API)");
}
static void w_serve_concurrent(int64_t port, wfn handler) {
    (void)port; (void)handler;
    w_fail("serve-concurrent is unsupported on wasm profile (no threads/sockets in v1)");
}
#endif // W_WASI

// Capabilities the C chain does not implement. Defined (so the build succeeds
// for programs that merely *import* std/http) but fatal with a message that
// names the root cause the moment one is actually called — instead of the
// `implicit declaration of function 'W__6'` gcc used to report.
static void w_cap_unimplemented(int id, const char *name) {
    char msg[256];
#ifdef W_WASI
    snprintf(msg, sizeof(msg),
             "capability @%d (%s) is unsupported on wasm profile; "
             "see docs/wasm-wasi-plan.md.",
             id, name);
#else
    snprintf(msg, sizeof(msg),
             "capability @%d (%s) is not implemented on the C chain; it is "
             "VM-only for now. See docs/independence-design.md Step 2b and "
             "tests/step2b_preregistered.md.",
             id, name);
#endif
    w_fail(msg);
}

// read-file(path) -> contents | "" on error
static wval w_read_file(const char *path) {
    FILE *f = fopen(path, "rb");
    if (!f) return W_PTR(w_str_new("", 0));
    fseek(f, 0, SEEK_END);
    long sz = ftell(f);
    fseek(f, 0, SEEK_SET);
    if (sz < 0) { fclose(f); return W_PTR(w_str_new("", 0)); }
    char *buf = (char *)w_alloc((size_t)sz + 1);
    size_t rd = fread(buf, 1, (size_t)sz, f);
    fclose(f);
    return W_PTR(w_str_new(buf, (uint32_t)rd));
}

// write-file(path, data) -> bytes written | -1
static wval w_write_file(const char *path, const char *data) {
    FILE *f = fopen(path, "wb");
    if (!f) return W_I64(-1);
    size_t wr = fwrite(data, 1, w_str_len(data), f);
    fclose(f);
    return W_I64((int64_t)wr);
}

// getenv(name) -> value | "" if unset
static wval w_env_get(const char *name) {
    const char *v = getenv(name);
    if (!v) return W_PTR(w_str_new("", 0));
    return W_PTR(w_str_new(v, (uint32_t)strlen(v)));
}

// ── Capability ABI for the c2 chain (Step 2a: independence pass) ─────
// `@n` capability tokens compile to `W__<n>()` calls. The self-hosted cgen
// emits "W_" + name + "()" for every word-shaped token, and classify hands
// it `_4` for `@4` — so the emitted call is `W__4()`. That name is a
// classify artifact (ideally `@n` would be a capability token, not a word
// reference), but it IS the ABI the generated C uses, so the runtime
// implements it here rather than editing compiler.all.ws: that file is the
// frozen fixed-point artifact, and changing it would force a seed
// regeneration plus a full bootstrap re-verification.
//
// Operands live on the Whisper stack — these shims take no C arguments,
// exactly like the VM's `Opcode::CapCall(id)`. Semantics mirror
// crates/whisper-core/src/capability.rs so the two chains stay bit-identical.
//
//   @0  file read   path -> contents        (FileReadCap)
//   @1  file write  path data -> bytes      (FileWriteCap)
//   @2  http get    url -> body             (HttpGetCap)
//   @3  http post   url body -> body        (HttpPostCap)
//   @4  env         name -> value           (EnvCap)
//   @5  exec        cmd -> [status out err] (ExecCap)
//
// NOT implemented: 6/7/8/9 (the HTTP *session* family — cookie jar, audit log,
// raw channel, parallel sessions). They are defined below as functions that
// fail loudly when called, so a program that merely imports std/http still
// builds and runs as long as it only touches the implemented ids.
//
// HTTPS is refused by @2/@3 (as it is by the VM) rather than silently sent as
// plaintext to port 443. See docs/independence-design.md, Step 2b, and the
// pre-registered criteria in tests/step2b_preregistered.md.

static void W__0(void) {                        // @0 file read
    const char *path = (const char *)w_as_ptr(w_pop());
    w_push(w_read_file(path));
}

static void W__1(void) {                        // @1 file write
    const char *data = (const char *)w_as_ptr(w_pop());
    const char *path = (const char *)w_as_ptr(w_pop());
    w_push(w_write_file(path, data));
}

#ifndef W_WASI
static void W__2(void) {                        // @2 http get
    const char *url = (const char *)w_as_ptr(w_pop());
    char err[512];
    char *body = w_http_request("GET", url, "", err, sizeof(err));
    if (!body) w_fail(err);
    w_push(W_PTR(w_str_new(body, (uint32_t)strlen(body))));
    free(body);
}

static void W__3(void) {                        // @3 http post
    const char *body_arg = (const char *)w_as_ptr(w_pop());
    const char *url = (const char *)w_as_ptr(w_pop());
    char err[512];
    char *body = w_http_request("POST", url, body_arg, err, sizeof(err));
    if (!body) w_fail(err);
    w_push(W_PTR(w_str_new(body, (uint32_t)strlen(body))));
    free(body);
}
#else
static void W__2(void) { w_fail("capability @2 (http-get) is unsupported on wasm profile"); }
static void W__3(void) { w_fail("capability @3 (http-post) is unsupported on wasm profile"); }
#endif

static void W__4(void) {                        // @4 env
    const char *name = (const char *)w_as_ptr(w_pop());
    w_push(w_env_get(name));
}

#ifndef W_WASI
static void W__6(void) {                        // @6 http session
    const char *body = (const char *)w_as_ptr(w_pop());
    const char *headers = (const char *)w_as_ptr(w_pop());
    const char *url = (const char *)w_as_ptr(w_pop());
    const char *method = (const char *)w_as_ptr(w_pop());
    char err[512];
    w_http_session_resp r;
    memset(&r, 0, sizeof(r));
    if (w_http_session(method, url, headers, body, 0, &r, err, sizeof(err)) != 0) w_fail(err);
    w_push(W_I64(r.status));
    w_push(W_PTR(w_str_new(r.headers, (uint32_t)strlen(r.headers))));
    w_push(W_PTR(w_str_new(r.body, (uint32_t)strlen(r.body))));
    free(r.headers);
    free(r.body);
}
#else
static void W__6(void) { w_fail("capability @6 (http-req (session)) is unsupported on wasm profile"); }
#endif
#ifndef W_WASI
static void W__7(void) {                        // @7 http audit
    w_net_global_lock();
    const char *mode = getenv("WHISPER_NET_AUDIT");
    if (mode && strcmp(mode, "json") == 0) {
        const char *entries = w_audit_json ? w_audit_json : "";
        size_t n = strlen(entries);
        char *out = (char *)malloc(n + 3);
        if (out) {
            sprintf(out, "[%s]", entries);
            w_push(W_PTR(w_str_new(out, (uint32_t)strlen(out))));
            free(out);
        } else {
            w_push(W_PTR(w_str_new("[]", 2)));
        }
        w_net_global_unlock();
        return;
    }
    const char *log = w_audit_log ? w_audit_log : "";
    size_t n = strlen(log);
    if (n > 0 && log[n - 1] == '\n') n--;
    w_push(W_PTR(w_str_new(log, (uint32_t)n)));
    w_net_global_unlock();
}
#else
static void W__7(void) { w_fail("capability @7 (http-audit) is unsupported on wasm profile"); }
#endif
#ifndef W_WASI
static void W__8(void) {                        // @8 http session with raw hex body
    const char *body = (const char *)w_as_ptr(w_pop());
    const char *headers = (const char *)w_as_ptr(w_pop());
    const char *url = (const char *)w_as_ptr(w_pop());
    const char *method = (const char *)w_as_ptr(w_pop());
    char err[512];
    w_http_session_resp r;
    memset(&r, 0, sizeof(r));
    if (w_http_session(method, url, headers, body, 1, &r, err, sizeof(err)) != 0) w_fail(err);
    w_push(W_I64(r.status));
    w_push(W_PTR(w_str_new(r.headers, (uint32_t)strlen(r.headers))));
    w_push(W_PTR(w_str_new(r.body, (uint32_t)strlen(r.body))));
    free(r.headers);
    free(r.body);
}
#else
static void W__8(void) { w_fail("capability @8 (http-req-raw) is unsupported on wasm profile"); }
#endif

#ifndef W_WASI
typedef struct {
    const char *method;
    const char *url;
    const char *headers;
    const char *body;
    w_http_session_resp r;
    int rc;
    char err[256];
} w_par_job;

#ifdef _WIN32
static DWORD WINAPI w_par_worker(LPVOID p) {
    w_par_job *j = (w_par_job *)p;
    j->rc = w_http_session(j->method, j->url, j->headers, j->body, 0,
                           &j->r, j->err, sizeof(j->err));
    return 0;
}
#else
static void *w_par_worker(void *p) {
    w_par_job *j = (w_par_job *)p;
    j->rc = w_http_session(j->method, j->url, j->headers, j->body, 0,
                           &j->r, j->err, sizeof(j->err));
    return NULL;
}
#endif

static void W__9(void) {                        // @9 parallel fan-out
    const char *body = (const char *)w_as_ptr(w_pop());
    const char *headers = (const char *)w_as_ptr(w_pop());
    const char *url = (const char *)w_as_ptr(w_pop());
    const char *method = (const char *)w_as_ptr(w_pop());
    int64_t count = w_as_i64(w_pop());
    if (count < 1 || count > 16) w_fail("@9 count must be in 1..16");
    w_par_job *jobs = (w_par_job *)calloc((size_t)count, sizeof(w_par_job));
    if (!jobs) w_fail("out of memory");
#ifdef _WIN32
    HANDLE *threads = (HANDLE *)calloc((size_t)count, sizeof(HANDLE));
#else
    pthread_t *threads = (pthread_t *)calloc((size_t)count, sizeof(pthread_t));
#endif
    if (!threads) { free(jobs); w_fail("out of memory"); }
    int *started = (int *)calloc((size_t)count, sizeof(int));
    if (!started) { free(jobs); free(threads); w_fail("out of memory"); }
    for (int64_t i = 0; i < count; i++) {
        jobs[i].method = method;
        jobs[i].url = url;
        jobs[i].headers = headers;
        jobs[i].body = body;
        jobs[i].r.status = 0;
#ifdef _WIN32
        threads[i] = CreateThread(NULL, 0, w_par_worker, &jobs[i], 0, NULL);
        started[i] = threads[i] != NULL;
#else
        started[i] = pthread_create(&threads[i], NULL, w_par_worker, &jobs[i]) == 0;
#endif
    }
    for (int64_t i = 0; i < count; i++) {
        if (!started[i]) continue;
#ifdef _WIN32
        WaitForSingleObject(threads[i], INFINITE);
        CloseHandle(threads[i]);
#else
        pthread_join(threads[i], NULL);
#endif
    }
    for (int64_t i = 0; i < count; i++) {
        if (!started[i]) {
            free(jobs[i].r.headers); free(jobs[i].r.body);
            free(jobs); free(threads); free(started);
            w_fail("thread create failed");
        }
        if (jobs[i].rc != 0) {
            for (int64_t k = 0; k < count; k++) {
                free(jobs[k].r.headers); free(jobs[k].r.body);
            }
            free(jobs); free(threads); free(started);
            w_fail(jobs[i].err[0] ? jobs[i].err : "@9 request failed");
        }
    }
    size_t cap = 256, len = 0;
    char *out = (char *)malloc(cap);
    if (!out) { free(jobs); free(threads); free(started); w_fail("out of memory"); }
    out[len++] = '[';
    for (int64_t i = 0; i < count; i++) {
        const char *esc = jobs[i].r.body ? jobs[i].r.body : "";
        char *eb = w_tls_json_escape_str(esc);
        const char *h = jobs[i].r.headers ? jobs[i].r.headers : "{}";
        size_t need = len + strlen(eb ? eb : "") + strlen(h) + 96;
        if (need > cap) {
            cap = need * 2;
            char *nb = (char *)realloc(out, cap);
            if (!nb) { free(eb); free(out); free(jobs); free(threads); free(started); w_fail("out of memory"); }
            out = nb;
        }
        len += (size_t)sprintf(out + len,
                              "%s{\"status\":%ld,\"headers\":%s,\"body\":\"%s\"}",
                              i ? "," : "", jobs[i].r.status, h, eb ? eb : "");
        free(eb);
        free(jobs[i].r.headers); jobs[i].r.headers = NULL;
        free(jobs[i].r.body); jobs[i].r.body = NULL;
    }
    out[len++] = ']';
    out[len] = 0;
    w_push(W_PTR(w_str_new(out, (uint32_t)len)));
    free(out); free(jobs); free(threads); free(started);
}
#else
static void W__9(void) { w_fail("capability @9 (http-req-parallel) is unsupported on wasm profile"); }
#endif

#ifndef W_WASI
// exec (@5). popen() captures one stream only, so stderr is redirected to a
// temp file and read back. The command is wrapped in a group — `( cmd )`
// groups in both POSIX sh and cmd.exe — so the redirection covers the whole
// command rather than just its last clause. stdin is fed from the null
// device to match Rust's `Command::output()`, which is what stops a command
// that reads stdin from hanging the VM forever.
#ifdef _WIN32
#  define W_GETPID _getpid
#  define W_POPEN  _popen
#  define W_PCLOSE _pclose
#else
#  include <unistd.h>
#  include <sys/wait.h>
#  define W_GETPID getpid
#  define W_POPEN  popen
#  define W_PCLOSE pclose
#endif

static char *w_capture(FILE *f, size_t *out_len) {
    size_t cap = 8192, len = 0;
    char *buf = (char *)malloc(cap);
    *out_len = 0;
    if (!buf) return NULL;
    for (;;) {
        size_t rd = fread(buf + len, 1, cap - len - 1, f);
        len += rd;
        if (len + 1 >= cap) {
            char *nb = (char *)realloc(buf, cap * 2);
            if (!nb) break;
            buf = nb;
            cap *= 2;
        }
        if (rd == 0) break;
    }
    buf[len] = 0;
    *out_len = len;
    return buf;
}

static char *w_slurp_file(const char *path) {
    FILE *f = fopen(path, "rb");
    if (!f) return NULL;
    size_t len = 0;
    char *buf = w_capture(f, &len);
    fclose(f);
    return buf;
}

// VM applies String::trim() to both streams; mirror it.
static void w_trim_inplace(char *s) {
    size_t n;
    if (!s) return;
    n = strlen(s);
    while (n > 0) {
        char c = s[n - 1];
        if (c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\v' || c == '\f') s[--n] = 0;
        else break;
    }
}

// L-1/W-6: `@10` = exec-raw. Identical to `@5` except that the two streams are
// returned **verbatim** instead of trimmed. `@5` keeps its trim on purpose —
// command-substitution idioms (`"cmd" @5 1 @nth strtoi64`) rely on it — so
// byte-exact forwarding gets its own id rather than changing an existing
// contract. Same authority: the CLI binds @10 wherever it binds @5.
static void w_exec_impl(int trim) {
    const char *cmd = (const char *)w_as_ptr(w_pop());
    char errfile[512];
    char *wrapped, *out, *err;
    size_t outlen = 0;
    int64_t status = -1;
    FILE *p;
    wval *lst;
    const char *tmpd;
#ifdef _WIN32
    tmpd = getenv("TEMP");
    if (!tmpd) tmpd = ".";
    snprintf(errfile, sizeof errfile, "%s\\whisper_exec_%ld.err", tmpd, (long)W_GETPID());
#else
    tmpd = getenv("TMPDIR");
    if (!tmpd) tmpd = "/tmp";
    snprintf(errfile, sizeof errfile, "%s/whisper_exec_%ld.err", tmpd, (long)W_GETPID());
#endif
    wrapped = (char *)w_alloc(strlen(cmd) + strlen(errfile) + 48);
    // The grouping parentheses must not be space-separated from the command:
    // cmd.exe's `echo` swallows the rest of the line, so `( echo x )` prints
    // "x " — a stray trailing space that `@5`'s trim used to hide and that
    // `@10` (verbatim) exposes. `(cmd)` groups identically without it.
#ifdef _WIN32
    sprintf(wrapped, "(%s) < NUL 2>\"%s\"", cmd, errfile);
#else
    sprintf(wrapped, "( %s ) < /dev/null 2>\"%s\"", cmd, errfile);
#endif
    p = W_POPEN(wrapped, "r");
    if (p) {
        int rc;
#ifdef _WIN32
        // The pipe read end is opened in text mode by _popen, which would
        // collapse the child's CRLF to LF and make `@10` (verbatim) diverge
        // from the VM, whose Command::output() captures raw bytes.
        _setmode(_fileno(p), _O_BINARY);
#endif
        out = w_capture(p, &outlen);
        rc = W_PCLOSE(p);
#ifdef _WIN32
        status = (int64_t)rc;
#else
        status = (rc >= 0 && WIFEXITED(rc)) ? (int64_t)WEXITSTATUS(rc) : -1;
#endif
    } else {
        out = NULL;
    }
    err = w_slurp_file(errfile);
    remove(errfile);
    // Both fallbacks must stay malloc'd — they are freed below, and arena
    // memory must never be handed to free().
    if (!out) { out = (char *)malloc(1); if (out) out[0] = 0; }
    if (!err) { err = (char *)malloc(1); if (err) err[0] = 0; }
    if (trim) {
        if (out) w_trim_inplace(out);
        if (err) w_trim_inplace(err);
    }
    lst = w_list_new(3);
    lst[1] = W_I64(status);
    lst[2] = W_PTR(w_str_from_cstr(out ? out : ""));
    lst[3] = W_PTR(w_str_from_cstr(err ? err : ""));
    w_push(W_PTR(lst));
    free(out);
    free(err);
}

static void W__5(void)  { w_exec_impl(1); }     // @5  exec（两流 trim）
static void W__10(void) { w_exec_impl(0); }     // @10 exec-raw（两流原样）
#else
static void W__5(void)  { w_fail("capability @5 (exec) is unsupported on wasm profile"); }
static void W__10(void) { w_fail("capability @10 (exec-raw) is unsupported on wasm profile"); }
#endif

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
#elif defined(W_WASI)
static void w_kv_lock_enter(void) { }
static void w_kv_lock_leave(void) { }
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
            wval r = W_PTR(w_str_new(kv_vals[i], w_str_len(kv_vals[i])));
            w_kv_lock_leave();
            return r;
        }
    }
    w_kv_lock_leave();
    return W_PTR(w_str_new("", 0)); // missing -> empty string (VM parity)
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
        l[1 + i] = W_PTR(w_str_new(kv_keys[i], w_str_len(kv_keys[i])));
    w_kv_lock_leave();
    return l;
}

// ── Concurrent serve (v1.0.2) ────────────────────────────────────────
#ifndef W_WASI
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
    w_push(W_I64((int64_t)job->conn));
    job->handler();
    w_sock_close(W_I64((int64_t)job->conn));
    w_worker_cleanup();
    free(job);
    return 0;
}

static void w_serve_concurrent(int64_t port, wfn handler) {
    w_net_require("serve-concurrent");
    wval srv = w_sock_listen(port);
    if (w_as_i64(srv) < 0) { fprintf(stderr, "serve-concurrent: listen failed\n"); exit(1); }
    for (;;) {
        w_socket_t c = accept((w_socket_t)w_as_i64(srv), NULL, NULL);
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
#endif // !W_WASI

static void w_init(int argc, char **argv) {
#ifdef _WIN32
    SetUnhandledExceptionFilter(_seh_handler);
    // L-1: stdout is a byte stream, not a text stream. Windows' default text
    // mode rewrites every '\n' as CRLF — but `print` promises verbatim bytes,
    // and the VM's stdout has never translated anything, so the two chains
    // disagreed on every newline (the Rust tests had to strip '\r' to compare).
    // Switching here rather than lazily inside `print` matters: text
    // translation is applied when the buffer is *flushed*, so a lazy switch
    // would silently retranslate earlier output and make `.` depend on
    // whether some later code happened to call `print`.
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    if (w_stack == NULL) {
        w_stack = (wval *)malloc(sizeof(wval) * STACK_SIZE);
        if (w_stack == NULL) { fprintf(stderr, "stack alloc failed\n"); exit(1); }
    }
    w_heap_init();
#ifndef W_WASI
    w_net_init();
#endif
    w_argc = argc;
    w_argv = argv;
    w_random_seed((uint64_t)time(NULL) ^ 0x12345678);
}
