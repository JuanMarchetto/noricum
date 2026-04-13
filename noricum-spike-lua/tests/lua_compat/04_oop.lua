-- Object-oriented programming via metatables
local Shape = {}
Shape.__index = Shape
function Shape.new(name) return setmetatable({name = name}, Shape) end
function Shape:describe() return string.format("Shape(%s)", self.name) end
function Shape:area() return 0 end

local Circle = setmetatable({}, {__index = Shape})
Circle.__index = Circle
function Circle.new(r)
  local c = Shape.new("Circle")
  c.r = r
  return setmetatable(c, Circle)
end
function Circle:area() return math.pi * self.r * self.r end

local Rect = setmetatable({}, {__index = Shape})
Rect.__index = Rect
function Rect.new(w, h)
  local r = Shape.new("Rect")
  r.w, r.h = w, h
  return setmetatable(r, Rect)
end
function Rect:area() return self.w * self.h end

local shapes = { Circle.new(5), Rect.new(3, 4), Circle.new(1) }
for _, s in ipairs(shapes) do
  print(s:describe(), string.format("area=%.4f", s:area()))
end
