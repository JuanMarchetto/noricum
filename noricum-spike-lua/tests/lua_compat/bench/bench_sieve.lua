local N = 200000
local p = {}
for i = 2, N do p[i] = true end
for i = 2, math.floor(math.sqrt(N)) do
  if p[i] then
    for j = i * i, N, i do p[j] = false end
  end
end
local count = 0
for i = 2, N do if p[i] then count = count + 1 end end
print(count)
