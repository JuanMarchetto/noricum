#include "wrapper.h"

#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"
#include "lctype.h"
#include "lopcodes.h"
#include "lmem.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/*
 * Thin delegations to the Lua public API. The wrapper exists so that
 * macros in the upstream headers (luaL_openlibs, lua_pcall, lua_tostring)
 * can be re-exposed as real C functions the Rust FFI layer can bind.
 */

spike_handle_t wr_newstate(void) {
    return (spike_handle_t) luaL_newstate();
}

void wr_openlibs(spike_handle_t h) {
    if (!h) return;
    luaL_openlibs((lua_State*) h);
}

int wr_loadstring(spike_handle_t h, const char* src) {
    if (!h || !src) return -1;
    return luaL_loadstring((lua_State*) h, src);
}

int wr_pcall(spike_handle_t h, int nargs, int nresults, int errfunc) {
    if (!h) return -1;
    return lua_pcall((lua_State*) h, nargs, nresults, errfunc);
}

long wr_tostring(spike_handle_t h, int idx, char* out, size_t cap) {
    if (!h || !out || cap == 0) return -1;
    lua_State* L = (lua_State*) h;
    size_t len = 0;
    /* lua_tolstring is the real function; lua_tostring is a macro over it. */
    const char* s = lua_tolstring(L, idx, &len);
    if (!s) return -1;
    size_t copy = (len < cap - 1) ? len : cap - 1;
    memcpy(out, s, copy);
    out[copy] = '\0';
    return (long) len;
}

void wr_close(spike_handle_t h) {
    if (!h) return;
    lua_close((lua_State*) h);
}

/*
 * Oracle convenience: read the file into memory, spin up a fresh Lua
 * state, open stdlibs, compile with luaL_loadstring, run with lua_pcall.
 * Returns the state as an opaque handle on success, or NULL if any step
 * failed. On success the chunk's return value(s) are left on the stack.
 */
spike_handle_t wr_open(const char* filename) {
    if (!filename) return NULL;

    FILE* f = fopen(filename, "rb");
    if (!f) return NULL;

    if (fseek(f, 0, SEEK_END) != 0) { fclose(f); return NULL; }
    long raw_size = ftell(f);
    if (raw_size < 0) { fclose(f); return NULL; }
    if (fseek(f, 0, SEEK_SET) != 0) { fclose(f); return NULL; }

    size_t size = (size_t) raw_size;
    char* buf = (char*) malloc(size + 1);
    if (!buf) { fclose(f); return NULL; }

    size_t got = fread(buf, 1, size, f);
    fclose(f);
    if (got != size) { free(buf); return NULL; }
    buf[size] = '\0';

    lua_State* L = luaL_newstate();
    if (!L) { free(buf); return NULL; }
    luaL_openlibs(L);

    int rc = luaL_loadstring(L, buf);
    free(buf);
    if (rc != LUA_OK) { lua_close(L); return NULL; }

    rc = lua_pcall(L, 0, LUA_MULTRET, 0);
    if (rc != LUA_OK) { lua_close(L); return NULL; }

    return (spike_handle_t) L;
}

/*
 * ---------------------------------------------------------------------------
 * lctype oracle shims. Each predicate evaluates the macro from lctype.h
 * for the provided int (to preserve the EOZ = -1 sentinel) and normalizes
 * the result to 1/0.
 * ---------------------------------------------------------------------------
 */

int wr_lctype_byte(int c) {
    if (c < -1 || c > 255) return -1;
    return (int) luai_ctype_[c + 1];
}

int wr_lctype_islalpha(int c) { return lislalpha(c) ? 1 : 0; }
int wr_lctype_islalnum(int c) { return lislalnum(c) ? 1 : 0; }
int wr_lctype_isdigit(int c)  { return lisdigit(c)  ? 1 : 0; }
int wr_lctype_isspace(int c)  { return lisspace(c)  ? 1 : 0; }
int wr_lctype_isprint(int c)  { return lisprint(c)  ? 1 : 0; }
int wr_lctype_isxdigit(int c) { return lisxdigit(c) ? 1 : 0; }

int wr_lctype_tolower(int c) {
    /* Lua's ltolower macro asserts c is A..Z or already unchanged by the
     * transform. We mirror exactly that domain and return the same bits. */
    if (c >= 'A' && c <= 'Z') return c | 0x20;
    return c;
}

/*
 * ---------------------------------------------------------------------------
 * lopcodes oracle shims. Expose the opmode table byte-by-byte, all
 * instruction-decoding macros as real functions, and the two helper
 * functions luaP_isOT / luaP_isIT. Also export a handful of opcode
 * discriminants so the Rust side can pin its enum.
 * ---------------------------------------------------------------------------
 */

int wr_lopcodes_num_opcodes(void) { return NUM_OPCODES; }

int wr_lopcodes_opmode(int op) {
    if (op < 0 || op >= NUM_OPCODES) return -1;
    return (int) luaP_opmodes[op];
}

int wr_lopcodes_get_opcode(unsigned int i) { return (int) GET_OPCODE((Instruction) i); }
int wr_lopcodes_getarg_a(unsigned int i)   { return GETARG_A((Instruction) i); }
int wr_lopcodes_getarg_b(unsigned int i)   { return GETARG_B((Instruction) i); }
int wr_lopcodes_getarg_c(unsigned int i)   { return GETARG_C((Instruction) i); }
int wr_lopcodes_getarg_k(unsigned int i)   { return GETARG_k((Instruction) i) ? 1 : 0; }
int wr_lopcodes_getarg_vb(unsigned int i)  { return GETARG_vB((Instruction) i); }
int wr_lopcodes_getarg_vc(unsigned int i)  { return GETARG_vC((Instruction) i); }
int wr_lopcodes_getarg_bx(unsigned int i)  { return GETARG_Bx((Instruction) i); }
int wr_lopcodes_getarg_sbx(unsigned int i) { return GETARG_sBx((Instruction) i); }
int wr_lopcodes_getarg_ax(unsigned int i)  { return GETARG_Ax((Instruction) i); }
int wr_lopcodes_getarg_sj(unsigned int i)  { return GETARG_sJ((Instruction) i); }

int wr_lopcodes_is_ot(unsigned int i) { return luaP_isOT((Instruction) i); }
int wr_lopcodes_is_it(unsigned int i) { return luaP_isIT((Instruction) i); }

int wr_lopcodes_op_tailcall(void) { return (int) OP_TAILCALL; }
int wr_lopcodes_op_setlist(void)  { return (int) OP_SETLIST; }
int wr_lopcodes_op_call(void)     { return (int) OP_CALL; }
int wr_lopcodes_op_return(void)   { return (int) OP_RETURN; }
int wr_lopcodes_op_extraarg(void) { return (int) OP_EXTRAARG; }

/*
 * ---------------------------------------------------------------------------
 * lmem oracle shim. Invokes luaM_growaux_ with size_elems == 0 so the
 * size-selection logic runs but no memory is actually allocated (by
 * the realloc contract, frealloc(ud, NULL, 0, 0) is a no-op). The call
 * is wrapped in lua_pcall so the "too many X" luaG_runerror path is
 * caught instead of aborting the process.
 * ---------------------------------------------------------------------------
 */

typedef struct {
    int size_in;
    int nelems;
    int limit;
    int size_out;
} wr_grow_probe_t;

static int wr_grow_probe_body(lua_State* L) {
    wr_grow_probe_t* p = (wr_grow_probe_t*) lua_touserdata(L, 1);
    int psize = p->size_in;
    /* size_elems = 0 => osize = 0, nsize = 0 in saferealloc; no alloc. */
    (void) luaM_growaux_(L, NULL, p->nelems, &psize, 0, p->limit, "x");
    p->size_out = psize;
    return 0;
}

int wr_lmem_grow_array_size(int size_in, int nelems, int limit, int* out_size) {
    if (!out_size) return -1;
    lua_State* L = luaL_newstate();
    if (!L) return -1;
    wr_grow_probe_t probe;
    probe.size_in = size_in;
    probe.nelems = nelems;
    probe.limit = limit;
    probe.size_out = size_in;
    lua_pushcfunction(L, wr_grow_probe_body);
    lua_pushlightuserdata(L, &probe);
    int rc = lua_pcall(L, 1, 0, 0);
    int ok = (rc == LUA_OK);
    if (ok) *out_size = probe.size_out;
    lua_close(L);
    return ok ? 1 : 0;
}
