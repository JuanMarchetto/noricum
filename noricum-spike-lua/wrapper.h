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

/* lopcodes — opcode metadata + instruction helpers (lopcodes.{c,h}). */
int wr_lopcodes_num_opcodes(void);
int wr_lopcodes_opmode(int op);       /* luaP_opmodes[op]; -1 if op out of range. */
int wr_lopcodes_get_opcode(unsigned int i);   /* GET_OPCODE(i) */
int wr_lopcodes_getarg_a(unsigned int i);
int wr_lopcodes_getarg_b(unsigned int i);
int wr_lopcodes_getarg_c(unsigned int i);
int wr_lopcodes_getarg_k(unsigned int i);
int wr_lopcodes_getarg_vb(unsigned int i);
int wr_lopcodes_getarg_vc(unsigned int i);
int wr_lopcodes_getarg_bx(unsigned int i);
int wr_lopcodes_getarg_sbx(unsigned int i);
int wr_lopcodes_getarg_ax(unsigned int i);
int wr_lopcodes_getarg_sj(unsigned int i);
int wr_lopcodes_is_ot(unsigned int i);
int wr_lopcodes_is_it(unsigned int i);
/* Opcode enum values, exposed so the Rust side can assert that its
 * OpCode discriminants match C without redeclaring the enum. */
int wr_lopcodes_op_tailcall(void);
int wr_lopcodes_op_setlist(void);
int wr_lopcodes_op_call(void);
int wr_lopcodes_op_return(void);
int wr_lopcodes_op_extraarg(void);

/* lmem — memory manager growth strategy (lmem.{c,h}).
 *
 * Invokes the real luaM_growaux_ with size_elems == 0 (no actual
 * allocation) under a protected call. Writes the new size into
 * *out_size on success. Returns 1 on success, 0 if luaM_growaux_
 * raised "too many X", or -1 if the oracle state itself failed to
 * initialize. */
int wr_lmem_grow_array_size(int size_in, int nelems, int limit, int* out_size);

/* lzio — buffered input stream (lzio.{c,h}).
 *
 * Spins up an ephemeral lua_State, wires a chunked in-memory reader
 * into a real ZIO, and runs the operation through it. All three
 * helpers work over the same (src, src_len, chunk_size) fixture so
 * the Rust port can test chunk-boundary behavior identically. */

/* Returns the number of missing bytes (0 on a full read). Writes up
 * to `n` bytes into `out_buf`. */
size_t wr_lzio_read(const char* src, size_t src_len, size_t chunk_size,
                    unsigned char* out_buf, size_t n);

/* Returns 1 on success, 0 on EOF or cross-chunk shortfall. Copies up
 * to `n` bytes into `out_buf` if the contiguous block exists. */
int wr_lzio_getaddr(const char* src, size_t src_len, size_t chunk_size,
                    unsigned char* out_buf, size_t n);

/* Returns the first byte of the first chunk as an unsigned int (0-255),
 * or -1 on EOZ. Mirrors luaZ_fill's return semantics. */
int wr_lzio_fill_first(const char* src, size_t src_len, size_t chunk_size);

/* lobject — pure utility helpers (lobject.c). Each shim calls the real
 * function; no lua_State needed. */
unsigned int wr_lobject_ceillog2(unsigned int x);
int          wr_lobject_hexavalue(int c);
unsigned int wr_lobject_codeparam(unsigned int p);
long long    wr_lobject_applyparam(unsigned int p, long long x);

/* Writes UTF-8 bytes backwards into `buff` (must be >= 8 bytes).
 * Returns the number of bytes written. `buff` layout matches
 * luaO_utf8esc: the encoded bytes occupy buff[8 - n .. 8]. */
int wr_lobject_utf8esc(unsigned char* buff, unsigned int x);

/* Invoke luaO_rawarith through an ephemeral state under lua_pcall.
 *
 * Operands are encoded as (tag, int_value, float_value) pairs where
 * tag == 0 means "use int_value" and tag == 1 means "use float_value".
 *
 * Return codes:
 *   1 -> arith succeeded; *out_tag / *out_int / *out_float hold the
 *        result, with *out_tag == 0 for an integer result and 1 for
 *        a float result.
 *   0 -> operands could not be converted (luaO_rawarith returned 0;
 *        caller should try a metamethod).
 *  -1 -> runtime error (div-by-zero raised via luaG_runerror).
 *  -2 -> ephemeral state failed to initialize.
 */
int wr_lobject_rawarith(int op,
                        int t1, long long i1, double f1,
                        int t2, long long i2, double f2,
                        int* out_tag, long long* out_int, double* out_float);

/* lstring — hash function oracle.
 *
 * luaS_hash itself is `static` in lstring.c, so we can't call it
 * directly. Instead, we spin up an ephemeral lua_State with the
 * provided seed, create a string through the public API, and return
 * its hash. Short strings store the hash in ts->hash after interning;
 * long strings go through luaS_hashlongstr which also uses the same
 * luaS_hash internally with the same seed.
 *
 * Returns the 32-bit hash on success, or 0 on failure. Since the
 * empty string with seed 0 legitimately hashes to 0, callers should
 * prefer a non-zero sanity seed when checking for errors. */
unsigned int wr_lstring_hash(const char* bytes, size_t len, unsigned int seed);

#ifdef __cplusplus
}
#endif

#endif
