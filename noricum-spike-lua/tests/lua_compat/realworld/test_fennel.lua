-- Fennel is a self-hosting Lisp dialect that compiles to Lua.
-- This exercises the Fennel COMPILER itself running on our Lua VM.
package.path = "tests/lua_compat/realworld/?.lua;" .. package.path
local fennel = require("fennel")

local lua = fennel.compileString("(+ 1 2 3)")
print(lua)

-- Run a function definition through the compiler then call it
local sq_lua = fennel.compileString("(fn [x] (* x x))")
print(sq_lua)
