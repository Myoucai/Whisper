#include <stdio.h>
#include "../whisperc/runtime.c"

static int expect(const char *domain, int want) {
    int got = w_public_suffix(domain);
    if (got != want) {
        fprintf(stderr, "psl: %s want=%d got=%d\n", domain, want, got);
        return 1;
    }
    return 0;
}

int main(void) {
    int bad = 0;
    bad |= expect("co.uk", 1);
    bad |= expect("com", 1);
    bad |= expect("github.io", 1);
    bad |= expect("example.co.uk", 0);
    bad |= expect("foo.github.io", 0);
    bad |= expect("www.ck", 0);      // PSL exception
    bad |= expect("foo.ck", 1);      // PSL wildcard
    bad |= expect("city.kawasaki.jp", 0);
    if (!bad) puts("psl-gate: PASS");
    return bad ? 1 : 0;
}
