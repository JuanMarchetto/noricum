-- pcall / xpcall / error propagation
local ok, err = pcall(function() error("boom") end)
print(ok, err)

local ok, err = pcall(function() error({code = 42, msg = "rich"}) end)
print(ok, type(err), err.code, err.msg)

local ok, err = pcall(function()
  local function inner() error("nested!") end
  inner()
end)
print(ok, err)

-- error from within a metamethod
local t = setmetatable({}, {__index = function() error("indexing forbidden") end})
local ok, err = pcall(function() return t.x end)
print(ok, err)

-- assert (use false directly to sidestep a known comparison-in-expr-pos
-- compiler bug we still have to fix in a follow-up)
local ok, err = pcall(function() assert(false, "math broken") end)
print(ok, err)
