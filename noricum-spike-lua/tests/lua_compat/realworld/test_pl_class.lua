-- Penlight `class` module: OOP utility
package.path = "tests/lua_compat/realworld/?.lua;" .. package.path
local class = require("pl_class")

-- Animal base
local Animal = class()
function Animal:_init(name)
  self.name = name
end
function Animal:greet()
  return "Hi, " .. self.name
end

local fido = Animal("Fido")
print(fido:greet())

-- Dog inheriting Animal
local Dog = class(Animal)
function Dog:_init(name)
  self:super(name)
end
function Dog:bark()
  return self.name .. " says woof"
end

local rex = Dog("Rex")
print(rex:greet())
print(rex:bark())

-- Identity / class chain
print(rex:is_a(Dog), rex:is_a(Animal))
