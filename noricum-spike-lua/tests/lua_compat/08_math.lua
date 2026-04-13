-- Math library
print(math.pi)
print(math.huge, -math.huge)
print(math.maxinteger, math.mininteger)
print(math.floor(3.7), math.ceil(3.2), math.abs(-5))
print(math.sqrt(16), math.sqrt(2))
print(math.max(1, 5, 3, 9, 2), math.min(1, 5, 3, 9, 2))
print(math.modf(3.75))
print(math.fmod(10, 3))
print(math.tointeger(3.0), math.tointeger(3.5))
print(math.type(1), math.type(1.0), math.type("x"))

-- Integer vs float arithmetic
print(1 + 2, 1.0 + 2.0, 1 + 2.0)
print(7 / 2, 7 // 2, 7 % 2)
print(2 ^ 10)  -- 1024.0 (float because ^ always returns float)

-- bitwise
print(5 & 3, 5 | 3, 5 ~ 3, ~5)
print(1 << 4, 256 >> 2)
