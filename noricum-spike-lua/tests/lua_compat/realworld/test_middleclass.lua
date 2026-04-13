package.path = "tests/lua_compat/realworld/?.lua;" .. package.path
local class = require("middleclass")

-- Basic class with methods
local Animal = class("Animal")
function Animal:initialize(name)
  self.name = name
end
function Animal:greet() return "Hi, I'm " .. self.name end

local fido = Animal:new("Fido")
print(fido:greet())
print(fido.class.name)
print(fido:isInstanceOf(Animal))

-- Inheritance
local Dog = class("Dog", Animal)
function Dog:bark() return self.name .. " says woof" end

local rex = Dog:new("Rex")
print(rex:greet())
print(rex:bark())
print(rex:isInstanceOf(Animal), rex:isInstanceOf(Dog))

-- Method override
function Dog:greet() return Animal.greet(self) .. " (a dog)" end
print(rex:greet())
