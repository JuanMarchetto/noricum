-- recursive fibonacci — exercises function calls + closures
local function fib(n)
  if n < 2 then return n end
  return fib(n - 1) + fib(n - 2)
end
for i = 0, 25 do
  io.write(fib(i), " ")
end
io.write("\n")
