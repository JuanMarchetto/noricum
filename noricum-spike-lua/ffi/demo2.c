// Embeds Rust Lua + registers a C library via luaL_setfuncs / luaL_newlib
// + tests luaL_argcheck / luaL_checktype / lua_concat / lua_len.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct lua_State lua_State;
typedef long long lua_Integer;
typedef double    lua_Number;
typedef int (*lua_CFunction)(lua_State *L);

#define LUA_OK         0
#define LUA_ERRRUN     2
#define LUA_MULTRET   (-1)
#define LUA_TNIL       0
#define LUA_TNUMBER    3
#define LUA_TSTRING    4
#define LUA_TTABLE     5

typedef struct luaL_Reg {
    const char *name;
    lua_CFunction func;
} luaL_Reg;

extern lua_State *luaL_newstate(void);
extern void       lua_close(lua_State *L);
extern void       luaL_openlibs(lua_State *L);
extern int        luaL_dostring(lua_State *L, const char *s);
extern int        luaL_loadstring(lua_State *L, const char *s);
extern int        lua_pcall(lua_State *L, int n, int r, int m);
extern int        lua_gettop(lua_State *L);
extern void       lua_settop(lua_State *L, int idx);
extern void       lua_pop(lua_State *L, int n);
extern void       lua_pushinteger(lua_State *L, lua_Integer n);
extern void       lua_pushstring(lua_State *L, const char *s);
extern void       lua_pushnil(lua_State *L);
extern void       lua_pushboolean(lua_State *L, int b);
extern lua_Integer lua_tointegerx(lua_State *L, int idx, int *ok);
extern const char *lua_tolstring(lua_State *L, int idx, size_t *len);
extern void       lua_setglobal(lua_State *L, const char *name);
extern int        lua_getglobal(lua_State *L, const char *name);
extern void       lua_concat(lua_State *L, int n);
extern void       lua_len(lua_State *L, int idx);
extern void       luaL_newlib(lua_State *L, const luaL_Reg *reg);
extern int        luaL_checkinteger(lua_State *L, int arg);
extern void       luaL_checktype(lua_State *L, int arg, int t);
extern int        luaL_argerror(lua_State *L, int arg, const char *msg);

static int my_add(lua_State *L) {
    int a = luaL_checkinteger(L, 1);
    int b = luaL_checkinteger(L, 2);
    lua_pushinteger(L, a + b);
    return 1;
}

static int my_strlen(lua_State *L) {
    luaL_checktype(L, 1, LUA_TSTRING);
    size_t n = 0;
    lua_tolstring(L, 1, &n);
    lua_pushinteger(L, (lua_Integer)n);
    return 1;
}

static int my_concat(lua_State *L) {
    int top = lua_gettop(L);
    if (top < 1) {
        return luaL_argerror(L, 1, "at least one argument required");
    }
    lua_concat(L, top);
    return 1;
}

static const luaL_Reg my_lib[] = {
    {"add",    my_add},
    {"strlen", my_strlen},
    {"concat", my_concat},
    {NULL, NULL},
};

int main(void) {
    lua_State *L = luaL_newstate();
    luaL_openlibs(L);

    // Register our C library as `mylib`.
    luaL_newlib(L, my_lib);
    lua_setglobal(L, "mylib");

    const char *script =
        "print('mylib.add(10, 32) =', mylib.add(10, 32))\n"
        "print('mylib.strlen(\"hello\") =', mylib.strlen('hello'))\n"
        "print('mylib.concat(...) =', mylib.concat('a', 'b', 'c'))\n"
        "local ok, err = pcall(mylib.add, 'x', 5)\n"
        "print('error case:', ok, err)\n";
    if (luaL_dostring(L, script) != LUA_OK) {
        size_t n = 0;
        const char *msg = lua_tolstring(L, -1, &n);
        fprintf(stderr, "script error: %.*s\n", (int)n, msg ? msg : "?");
    }

    lua_close(L);
    return 0;
}
