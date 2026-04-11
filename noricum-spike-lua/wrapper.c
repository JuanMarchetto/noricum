#include "wrapper.h"
/* TODO: include the target library's public header */
/* #include "lua.h" */
#include <stdlib.h>

/*
 * TODO: allocate an internal struct on the heap, call the target
 * library's init function, return the opaque pointer. See
 * docs/methodology/phase-0-oracle-harness.md for the pattern.
 */
spike_handle_t wr_open(const char* filename) {
    (void)filename;
    /* TODO */
    return NULL;
}

int wr_close(spike_handle_t h) {
    if (!h) return -1;
    /* TODO: call target library's end/close, free the wrapper struct */
    free(h);
    return 0;
}
