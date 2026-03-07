# Pattern: #define Macros to const/const fn

## C Pattern
```c
#define MAX_BUFFER_SIZE 1024
#define PI 3.14159265358979
#define SQUARE(x) ((x) * (x))
#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define ARRAY_LEN(arr) (sizeof(arr) / sizeof((arr)[0]))

void process(void) {
    char buf[MAX_BUFFER_SIZE];
    double area = PI * SQUARE(radius);
    int bigger = MAX(x, y);
}
```

## Rust Equivalent
```rust
const MAX_BUFFER_SIZE: usize = 1024;
const PI: f64 = std::f64::consts::PI;

const fn square(x: i32) -> i32 {
    x * x
}

fn max_val(a: i32, b: i32) -> i32 {
    a.max(b)
}

fn process(radius: f64, x: i32, y: i32) {
    let mut buf = [0u8; MAX_BUFFER_SIZE];
    let area = PI * (radius * radius);
    let bigger = x.max(y);
}
```

## When to Apply
- C code uses `#define` for numeric/string constants
- Function-like macros that could be `const fn` or generic functions
- `sizeof`-based array length macros (use `.len()` in Rust)
- Key insight: `const` for values, `const fn` for compile-time functions, generics or std methods for type-generic operations
