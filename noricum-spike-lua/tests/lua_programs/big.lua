local function fact(n)
  if n <= 1 then return 1 else return n * fact(n - 1) end
end
print("10! = " .. fact(10))
print(math.pi)
print(string.upper("hello, world"))
for i = 1, 5 do print("i=" .. i) end

local function fib(n)
  if n < 2 then return n else return fib(n-1) + fib(n-2) end
end
print("fib(15) = " .. fib(15))

local sum = 0
for i = 1, 100 do sum = sum + i end
print("1..100 sum = " .. sum)
