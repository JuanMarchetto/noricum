#include "wrapper.h"

#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"
#include "lctype.h"

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
