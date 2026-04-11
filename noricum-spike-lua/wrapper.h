#ifndef NORICUM_SPIKE_LUA_WRAPPER_H
#define NORICUM_SPIKE_LUA_WRAPPER_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Opaque handle. Caller never peeks inside. Fill in the functions below
 * to match the target library's public API. See
 * docs/methodology/phase-0-oracle-harness.md for the naming convention.
 */
typedef void* spike_handle_t;

/* TODO: replace these stubs with actual wrapper functions for lua. */
spike_handle_t wr_open(const char* filename);
int            wr_close(spike_handle_t h);

#ifdef __cplusplus
}
#endif

#endif
