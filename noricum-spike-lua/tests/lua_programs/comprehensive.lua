-- Classes via metatables
local Vec = {}
Vec.__index = Vec

function Vec.new(x, y)
  return setmetatable({x = x, y = y}, Vec)
end

function Vec:length()
  return math.sqrt(self.x * self.x + self.y * self.y)
end

function Vec:add(other)
  return Vec.new(self.x + other.x, self.y + other.y)
end

-- Iterate pairs
local v1 = Vec.new(3, 4)
local v2 = Vec.new(1, 2)
local v3 = v1:add(v2)
print("v1 =", v1.x, v1.y, "length =", v1:length())
print("v3 = v1+v2 =", v3.x, v3.y)

-- Strings
local s = "Hello, World"
print("upper:", s:upper())
print("sub(1,5):", s:sub(1, 5))
print("len:", #s)

-- Table ops
local nums = {3, 1, 4, 1, 5, 9, 2, 6, 5, 3}
table.sort(nums)
io.write("sorted: ")
for _, n in ipairs(nums) do io.write(n, " ") end
io.write("\n")

-- Closures
local function counter()
  local n = 0
  return function()
    n = n + 1
    return n
  end
end
local c = counter()
print("counts:", c(), c(), c())

-- pcall
local ok, err = pcall(function() error("boom") end)
print("pcall:", ok, err)

-- for loop break
for i = 1, 100 do
  if i > 3 then break end
  print("i=", i)
end
