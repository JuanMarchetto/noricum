# Pattern: Enum/Switch Dispatch to Match

## C Pattern
```c
typedef enum {
    SHAPE_CIRCLE,
    SHAPE_RECT,
    SHAPE_TRIANGLE
} ShapeType;

typedef struct {
    ShapeType type;
    union {
        struct { double radius; } circle;
        struct { double width, height; } rect;
        struct { double base, h; } triangle;
    };
} Shape;

double area(const Shape *s) {
    switch (s->type) {
        case SHAPE_CIRCLE:
            return 3.14159265 * s->circle.radius * s->circle.radius;
        case SHAPE_RECT:
            return s->rect.width * s->rect.height;
        case SHAPE_TRIANGLE:
            return 0.5 * s->triangle.base * s->triangle.h;
        default:
            return 0.0;
    }
}
```

## Rust Equivalent
```rust
enum Shape {
    Circle { radius: f64 },
    Rect { width: f64, height: f64 },
    Triangle { base: f64, height: f64 },
}

impl Shape {
    fn area(&self) -> f64 {
        match self {
            Shape::Circle { radius } => {
                std::f64::consts::PI * radius * radius
            }
            Shape::Rect { width, height } => width * height,
            Shape::Triangle { base, height } => 0.5 * base * height,
        }
    }
}
```

## When to Apply
- C uses an enum tag + union (tagged union) pattern
- `switch` statement dispatches on the tag
- Struct holds a `type` field alongside a `union` of data variants
- Key insight: C tagged unions map directly to Rust enums with data
