package.path = "tests/lua_compat/realworld/?.lua;" .. package.path
local json = require("json")

-- Encode/decode roundtrip
local data = {
  name = "noricum",
  version = 0.1,
  features = {"closures", "coroutines", "metatables"},
  count = 42,
  nested = { a = 1, b = { deep = true } },
}

local s = json.encode(data)
print("encoded length:", #s)

local back = json.decode(s)
print("decoded name:", back.name)
print("decoded version:", back.version)
print("decoded features[1]:", back.features[1])
print("decoded features[3]:", back.features[3])
print("decoded count:", back.count)
print("decoded nested.a:", back.nested.a)
print("decoded nested.b.deep:", back.nested.b.deep)

-- Integers vs floats
print(json.encode(42))
print(json.encode(3.14))
print(json.encode(true))
print(json.encode(false))
print(json.encode(nil))
print(json.encode({1, 2, 3}))
print(json.encode({}))

-- Strings with special chars
print(json.encode("line1\nline2"))
print(json.encode('"quoted"'))
