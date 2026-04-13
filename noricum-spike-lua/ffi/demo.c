// Minimal C host that embeds our Rust Lua runtime via the FFI.
// Build: see demo.sh in the same directory.
//
// This file uses ONLY the subset of lua.h we've implemented: state
// creation, library loading, script loading, pcall, value extraction.
// No hidden includes — we declare the symbols inline so the demo is
// explicit about the ABI contract.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct lua_State lua_State;
typedef long long lua_Integer;
typedef double    lua_Number;
typedef int (*lua_CFunction)(lua_State *L);

#define LUA_OK         0
#define LUA_ERRRUN     2
#define LUA_ERRSYNTAX  3
#define LUA_MULTRET   (-1)
#define LUA_TNIL       0
#define LUA_TNUMBER    3
#define LUA_TSTRING    4
#define LUA_TTABLE     5

extern lua_State *luaL_newstate(void);
extern void       lua_close(lua_State *L);
extern void       luaL_openlibs(lua_State *L);
extern int        luaL_loadstring(lua_State *L, const char *s);
extern int        lua_pcall(lua_State *L, int nargs, int nresults, int msgh);
extern int        lua_gettop(lua_State *L);
extern void       lua_settop(lua_State *L, int idx);
extern int        lua_type(lua_State *L, int idx);
extern lua_Integer lua_tointegerx(lua_State *L, int idx, int *isnum);
extern lua_Number  lua_tonumberx(lua_State *L, int idx, int *isnum);
extern const char *lua_tolstring(lua_State *L, int idx, size_t *len);
extern void       lua_pushinteger(lua_State *L, lua_Integer n);
extern void       lua_pushstring(lua_State *L, const char *s);
extern void       lua_pushcfunction(lua_State *L, lua_CFunction fn);
extern void       lua_setglobal(lua_State *L, const char *name);
extern int        lua_getglobal(lua_State *L, const char *name);

// Custom C function we'll expose to Lua as `c_add(a, b)`.
static int c_add(lua_State *L) {
    int isa = 0, isb = 0;
    lua_Integer a = lua_tointegerx(L, 1, &isa);
    lua_Integer b = lua_tointegerx(L, 2, &isb);
    if (!isa || !isb) return 0;
    lua_pushinteger(L, a + b);
    return 1;
}

static void run(lua_State *L, const char *name, const char *src) {
    printf("=== %s ===\n", name);
    int rc = luaL_loadstring(L, src);
    if (rc != LUA_OK) {
        size_t n = 0;
        const char *msg = lua_tolstring(L, -1, &n);
        printf("load error (%d): %.*s\n", rc, (int)n, msg ? msg : "?");
        lua_settop(L, 0);
        return;
    }
    rc = lua_pcall(L, 0, LUA_MULTRET, 0);
    if (rc != LUA_OK) {
        size_t n = 0;
        const char *msg = lua_tolstring(L, -1, &n);
        printf("runtime error (%d): %.*s\n", rc, (int)n, msg ? msg : "?");
        lua_settop(L, 0);
        return;
    }
    printf("-> %d values returned\n", lua_gettop(L));
    lua_settop(L, 0);
}

int main(void) {
    lua_State *L = luaL_newstate();
    if (!L) {
        fprintf(stderr, "luaL_newstate failed\n");
        return 1;
    }
    luaL_openlibs(L);

    // Expose a C function to Lua.
    lua_pushcfunction(L, c_add);
    lua_setglobal(L, "c_add");

    run(L, "hello",
        "print('hello from C-embedded Rust Lua!')");
    run(L, "arith",
        "print(1 + 2, 'factorial 10 =', (function(n) local r=1 for i=1,n do r=r*i end return r end)(10))");
    run(L, "c_fn",
        "print('c_add(7,35) =', c_add(7, 35))");
    run(L, "coroutine",
        "local co = coroutine.create(function() for i=1,3 do coroutine.yield(i*i) end end)\n"
        "local a,b = coroutine.resume(co); print(a,b)\n"
        "local a,b = coroutine.resume(co); print(a,b)\n"
        "local a,b = coroutine.resume(co); print(a,b)");
    run(L, "table_ops",
        "local t = {}\n"
        "for i=1,10 do t[#t+1] = i*i end\n"
        "print(table.concat(t, ','))");
    run(L, "closure",
        "local function make(n) return function() n = n + 1; return n end end\n"
        "local c = make(100); print(c(), c(), c())");

    lua_close(L);
    return 0;
}
