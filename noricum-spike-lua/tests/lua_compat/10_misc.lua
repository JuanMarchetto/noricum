-- Miscellaneous: goto, labels, multiple assignment, string methods via colon

-- goto/labels
for i = 1, 5 do
  if i % 2 == 0 then goto skip end
  io.write(i, " ")
  ::skip::
end
io.write("\n")

-- Multiple assignment
local a, b, c = 1, 2, 3
print(a, b, c)
a, b = b, a
print(a, b)

-- Multiple return into multi-local
local function pair() return 10, 20 end
local x, y = pair()
print(x, y)

-- String methods via colon
print(("hello"):upper(), ("world"):len(), ("abcdef"):sub(2, 4))

-- repeat/until
local n = 0
repeat
  n = n + 1
until n >= 5
print(n)

-- and/or short-circuit
local function side(x, y) print("side", x); return y end
print(side(1, false) and side(2, true))
print(side(3, true) or side(4, false))

-- numeric for downto
local sum = 0
for i = 10, 1, -2 do sum = sum + i end
print(sum)
