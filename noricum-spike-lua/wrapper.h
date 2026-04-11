#ifndef NORICUM_SPIKE_LUA_WRAPPER_H
#define NORICUM_SPIKE_LUA_WRAPPER_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Opaque handle. Internally this is a `lua_State*`, but the Rust side
 * never peeks inside. See docs/methodology/phase-0-oracle-harness.md.
 *
 * Target API surface for the spike:
 *   luaL_newstate, luaL_openlibs, luaL_loadstring,
 *   lua_pcall, lua_tostring, lua_close
 *
 * Several of these are macros in the upstream headers, so the wrapper
 * re-exposes them as real functions so they can be bound via extern "C".
 */
typedef void* spike_handle_t;

/* 1:1 delegations to the six target-API functions/macros. */
spike_handle_t wr_newstate(void);
void           wr_openlibs(spike_handle_t h);
int            wr_loadstring(spike_handle_t h, const char* src);
int            wr_pcall(spike_handle_t h, int nargs, int nresults, int errfunc);
long           wr_tostring(spike_handle_t h, int idx, char* out, size_t cap);
void           wr_close(spike_handle_t h);

/*
 * Convenience oracle entry point used by wrapper_smoke.c and
 * tests/differential_lua.rs: reads the file at `filename`, creates a
 * fresh Lua state, opens stdlibs, loads the file content via
 * luaL_loadstring, and runs it via lua_pcall. On success, the chunk's
 * return value is left on top of the stack and the Lua state is
 * returned as an opaque handle. On any failure, returns NULL.
 */
spike_handle_t wr_open(const char* filename);

/*
 * ---------------------------------------------------------------------------
 * Module-level oracle shims for the full Lua 5.4 port differential tests.
 *
 * Each group below exposes real C functions for the macros / tables that
 * Lua's headers define preprocessor-only, so the Rust side can call into
 * the C oracle via plain extern "C" bindings without re-implementing the
 * header logic.
 *
 * Inputs are `int` (not `unsigned char`) so the shims can carry Lua's
 * `EOZ = -1` sentinel value unchanged.
 * ---------------------------------------------------------------------------
 */

/* lctype — ASCII character classification (ground truth: lctype.{c,h}). */
int wr_lctype_byte(int c);       /* luai_ctype_[c+1]; -1 if c is out of range. */
int wr_lctype_islalpha(int c);
int wr_lctype_islalnum(int c);
int wr_lctype_isdigit(int c);
int wr_lctype_isspace(int c);
int wr_lctype_isprint(int c);
int wr_lctype_isxdigit(int c);
int wr_lctype_tolower(int c);    /* Only defined for 'A'..'Z'; returns (c | 0x20). */

#ifdef __cplusplus
}
#endif

#endif
