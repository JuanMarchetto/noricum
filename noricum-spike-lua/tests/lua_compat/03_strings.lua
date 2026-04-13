-- String library workout: gsub, gmatch, format, find, sub, byte
local s = "The quick brown fox jumps over the lazy dog"
print(#s)
print(string.upper(s))
print(string.reverse(s))
print(string.sub(s, 5, 9))
print(string.find(s, "fox"))
local words = {}
for w in string.gmatch(s, "%a+") do words[#words+1] = w end
print(table.concat(words, "|"))
print((string.gsub(s, "%a+", function(w) return w:upper() end)))
print(string.format("len=%d first=%s last=%s", #s, s:sub(1, 3), s:sub(-3)))
print(string.format("%05d %.4f %x %s", 42, math.pi, 255, "hi"))
print(string.byte("A"), string.char(65))
print(string.rep("ab", 5, "-"))
