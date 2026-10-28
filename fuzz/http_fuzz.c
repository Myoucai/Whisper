// libFuzzer harness for the native C HTTP/URL parsers.
// Build (Linux, clang):
//   clang -fsanitize=fuzzer,address -g -O1 -I whisperc fuzz/http_fuzz.c -o fuzz-http
// The runtime is header-like; including it gives access to the static parsers.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "../whisperc/runtime.c"

int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size) {
    if (size == 0 || size > 65536) return 0;
    char *s = (char *)malloc(size + 1);
    if (!s) return 0;
    memcpy(s, data, size);
    s[size] = 0;
    char err[256];

    char *headers = w_headers_json(s, size);
    free(headers);

    size_t body_len = 0;
    char *dechunked = w_dechunk(s, &body_len);
    free(dechunked);

    char *user_headers = w_user_headers(s, err, sizeof(err));
    free(user_headers);

    unsigned char *raw = NULL;
    size_t raw_len = 0;
    if (w_hex_decode(s, &raw, &raw_len, err, sizeof(err)) == 0) free(raw);

    w_url u;
    if (w_url_parse(s, &u) == 0) w_url_free(&u);

    free(s);
    return 0;
}
