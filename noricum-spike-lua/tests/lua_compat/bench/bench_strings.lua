local s = string.rep("abcdefghij", 1000)  -- 10k chars
local count = 0
for i = 1, 200 do
  count = count + #string.upper(s)
end
print(count)
