# Pattern: Header Files to Module System

## C Pattern
```c
// math_utils.h
#ifndef MATH_UTILS_H
#define MATH_UTILS_H

typedef struct {
    double x, y;
} Vec2;

Vec2 vec2_add(Vec2 a, Vec2 b);
Vec2 vec2_scale(Vec2 v, double s);
double vec2_dot(Vec2 a, Vec2 b);

#endif

// math_utils.c
#include "math_utils.h"

Vec2 vec2_add(Vec2 a, Vec2 b) {
    return (Vec2){ a.x + b.x, a.y + b.y };
}

Vec2 vec2_scale(Vec2 v, double s) {
    return (Vec2){ v.x * s, v.y * s };
}

double vec2_dot(Vec2 a, Vec2 b) {
    return a.x * b.x + a.y * b.y;
}
```

## Rust Equivalent
```rust
// math_utils.rs (or math_utils/mod.rs)
#[derive(Debug, Clone, Copy)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub fn add(self, other: Vec2) -> Vec2 {
        Vec2 { x: self.x + other.x, y: self.y + other.y }
    }

    pub fn scale(self, s: f64) -> Vec2 {
        Vec2 { x: self.x * s, y: self.y * s }
    }

    pub fn dot(self, other: Vec2) -> f64 {
        self.x * other.x + self.y * other.y
    }
}
```

## When to Apply
- C code split across `.h` (declarations) and `.c` (definitions)
- `#ifndef`/`#define` include guards
- Functions that operate on a common struct type
- Key insight: Rust's module system (`mod`, `pub`) replaces header files. Free functions operating on a struct become `impl` methods. No forward declarations needed
