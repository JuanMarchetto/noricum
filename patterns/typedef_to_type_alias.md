# Pattern: typedef to Type Alias or Newtype

## C Pattern
```c
typedef unsigned char byte_t;
typedef int error_code_t;
typedef void (*event_handler_t)(int event_id, void *context);

typedef struct {
    double x, y;
} Point;

typedef struct node {
    int value;
    struct node *next;
} Node;

error_code_t process(byte_t *data, size_t len);
```

## Rust Equivalent
```rust
type Byte = u8;

// Newtype for type safety (prevents mixing error codes with plain ints)
#[derive(Debug, Clone, Copy, PartialEq)]
struct ErrorCode(i32);

// Function pointer typedef -> fn type or generic
type EventHandler = fn(event_id: i32, context: &mut dyn std::any::Any);

#[derive(Debug, Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

struct Node {
    value: i32,
    next: Option<Box<Node>>,
}

fn process(data: &[u8]) -> Result<(), ErrorCode> {
    Ok(())
}
```

## When to Apply
- C code uses `typedef` for primitive type aliases
- `typedef struct { ... } Name;` pattern for anonymous structs
- Function pointer typedefs
- Key insight: use `type` for simple aliases, newtype pattern (`struct Foo(T)`) when you need type-level distinction, regular `struct` for aggregate types
