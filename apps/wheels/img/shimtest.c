#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <windows.h>

typedef int64_t (*fn12)(int64_t, int64_t, int64_t, int64_t, int64_t,
                        int64_t, int64_t, int64_t, int64_t, int64_t,
                        int64_t, int64_t);

int main() {
    HMODULE lib = LoadLibraryA("C:/msys64/mingw64/bin/libturbojpeg.dll");
    if (!lib) { printf("no dll\n"); return 1; }

    void *(*p_init)(void) = (void *(*)(void))GetProcAddress(lib, "tjInitDecompress");
    int (*p_hdr)(void *, const void *, unsigned long, int *, int *, int *, int *);
    p_hdr = (int (*)(void *, const void *, unsigned long, int *, int *, int *, int *))
            GetProcAddress(lib, "tjDecompressHeader3");
    int (*p_dec9)(void *, const void *, unsigned long, unsigned char *, int, int, int, int, int);
    p_dec9 = (int (*)(void *, const void *, unsigned long, unsigned char *, int, int, int, int, int))
             GetProcAddress(lib, "tjDecompress2");
    printf("syms: init=%p hdr=%p dec9=%p\n", p_init, p_hdr, p_dec9);

    FILE *f = fopen("t.jpg", "rb");
    unsigned char jpeg[4096]; size_t jlen = fread(jpeg, 1, sizeof jpeg, f); fclose(f);

    void *h = p_init();
    int w = -1, hh = -1, pf = -1, ss = -1;
    p_hdr(h, jpeg, jlen, &w, &hh, &pf, &ss);
    printf("hdr: %dx%d pf=%d ss=%d\n", w, hh, pf, ss);

    unsigned char dst[72];
    memset(dst, 0xAA, 72);
    int rc = p_dec9(h, jpeg, jlen, dst, w, 0, hh, 0, 0);
    printf("direct9 rc=%d: %d %d %d %d %d %d\n", rc, dst[0], dst[1], dst[2], dst[3], dst[4], dst[5]);

    // 通过 12 参垫板调用同一个 dec9
    memset(dst, 0xAA, 72);
    fn12 f12 = (fn12)p_dec9;
    int64_t r2 = f12((int64_t)h, (int64_t)jpeg, (int64_t)jlen, (int64_t)dst,
                     w, 0, hh, 0, 0, 0, 0, 0);
    printf("shim12 rc=%lld: %d %d %d %d %d %d\n", (long long)r2, dst[0], dst[1], dst[2], dst[3], dst[4], dst[5]);
    return 0;
}
