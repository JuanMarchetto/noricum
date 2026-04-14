-- string.dump / load .luac roundtrip
local function test(label, fn, expected)
  local s = string.dump(fn)
  local f2, err = load(s)
  if not f2 then
    print(label, "LOAD FAIL", err)
    return
  end
  local got = f2()
  if got == expected then
    print(label, "OK")
  else
    print(label, "DIFF expected", expected, "got", got)
  end
end

test("simple", function() return 42 end, 42)
test("arith", function() return 1 + 2 * 3 end, 7)
test("string", function() return "hello" end, "hello")
-- Closures with upvalues lose their upvalue values on dump+load
-- (matches C Lua behavior). Skip that case.

-- More complex: multiple returns
local s = string.dump(function() return 1, 2, 3 end)
local f = load(s)
print("multi:", f())

-- Nested function
local nested = string.dump(function()
  local function helper(x) return x * 2 end
  return helper(21)
end)
local fn = load(nested)
print("nested:", fn())
