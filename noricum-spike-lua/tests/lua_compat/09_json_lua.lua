-- Mini JSON encoder — recursive structures, multiple types
local function encode(v, indent)
  indent = indent or 0
  local pad = string.rep("  ", indent)
  local t = type(v)
  if t == "nil" then return "null" end
  if t == "boolean" then return tostring(v) end
  if t == "number" then
    if math.type(v) == "integer" then return tostring(v) end
    return string.format("%.6g", v)
  end
  if t == "string" then
    return '"' .. v:gsub('"', '\\"'):gsub("\n", "\\n") .. '"'
  end
  if t == "table" then
    -- Heuristic: array if numeric keys 1..N, else object
    local n = #v
    local is_array = n > 0
    for k in pairs(v) do
      if type(k) ~= "number" or k % 1 ~= 0 or k < 1 or k > n then
        is_array = false
        break
      end
    end
    if is_array then
      local parts = {}
      for _, item in ipairs(v) do
        parts[#parts+1] = encode(item, indent + 1)
      end
      return "[" .. table.concat(parts, ", ") .. "]"
    else
      local keys = {}
      for k in pairs(v) do keys[#keys+1] = k end
      table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
      local parts = {}
      for _, k in ipairs(keys) do
        parts[#parts+1] = string.format('"%s": %s', tostring(k), encode(v[k], indent + 1))
      end
      return "{" .. table.concat(parts, ", ") .. "}"
    end
  end
  return "null"
end

local data = {
  name = "noricum",
  version = 0.1,
  features = {"closures", "coroutines", "metatables", "TBC"},
  tested = true,
  count = 42,
  inner = {
    deep = {
      value = "ok"
    }
  }
}

print(encode(data))
