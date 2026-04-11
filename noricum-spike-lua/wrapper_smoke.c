/*
 * Pure-C smoke test. Compile and run BEFORE writing any Rust:
 *   gcc wrapper_smoke.c wrapper.c <target>.c -o wrapper_smoke -I.
 *   ./wrapper_smoke
 * Must exit 0. If this fails, wrapper.c has a bug and Rust is not
 * the problem.
 */
#include "wrapper.h"
#include <stdio.h>

int main(void) {
    /* TODO: open a known-good sanity fixture, verify content, exit 0 */
    spike_handle_t h = wr_open("fixtures/sanity_corpus/hello.dat");
    if (!h) {
        fprintf(stderr, "FAIL: wr_open returned NULL\n");
        return 1;
    }
    wr_close(h);
    printf("smoke: OK\n");
    return 0;
}
