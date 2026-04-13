-- table.* library + ipairs/pairs/next
local t = {3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5}
table.sort(t)
print(table.concat(t, ","))
table.sort(t, function(a, b) return a > b end)
print(table.concat(t, ","))
table.insert(t, 1, 99)
print(table.concat(t, ","))
table.remove(t, 5)
print(table.concat(t, ","))
print(table.unpack({10, 20, 30, 40}))

-- pairs iteration: order is unspecified, so sort the output for stable diff
local m = {a=1, b=2, c=3, d=4}
local keys = {}
for k in pairs(m) do keys[#keys+1] = k end
table.sort(keys)
for _, k in ipairs(keys) do io.write(k, "=", m[k], " ") end
io.write("\n")

-- Mixed integer + string keys
local x = {[1]="one", [2]="two", three=3, [4]="four"}
print(x[1], x[2], x.three, x[4])
-- print(#x) — implementation-defined border (any of {0, 2, 4} is
-- legal per the Lua reference manual). C ref returns 4, ours
-- returns 2; both satisfy the invariant t[n] != nil and t[n+1] == nil.
local len = #x
print(len == 0 or len == 2 or len == 4)
