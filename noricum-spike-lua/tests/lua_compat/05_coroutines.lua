-- Coroutine producer/consumer pattern + wrap
local function producer(n)
  for i = 1, n do coroutine.yield(i, i*i) end
end

local function consume(p)
  while true do
    local i, sq = p()
    if i == nil then break end
    io.write(string.format("[%d->%d] ", i, sq))
  end
  io.write("\n")
end

consume(coroutine.wrap(function() producer(8) end))

-- Manual create + resume
local co = coroutine.create(function(a, b)
  local c = coroutine.yield(a + b)
  return c * 10
end)
print(coroutine.resume(co, 3, 4))
print(coroutine.resume(co, 7))
print(coroutine.status(co))
