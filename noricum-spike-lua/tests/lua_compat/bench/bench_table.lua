local t = {}
for i = 1, 100000 do t[i] = i * 3 end
local sum = 0
for i = 1, #t do sum = sum + t[i] end
print(sum)
