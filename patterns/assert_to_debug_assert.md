# Pattern: assert() to debug_assert!/assert!

## C Pattern
```c
#include <assert.h>

void process(int *data, size_t len) {
    assert(data != NULL);
    assert(len > 0);
    assert(len <= MAX_SIZE);

    for (size_t i = 0; i < len; i++) {
        assert(data[i] >= 0);
        data[i] *= 2;
    }
}

int divide(int a, int b) {
    assert(b != 0);
    return a / b;
}
```

## Rust Equivalent
```rust
fn process(data: &mut [i32]) {
    assert!(!data.is_empty(), "data must not be empty");
    assert!(data.len() <= MAX_SIZE, "data exceeds MAX_SIZE");

    for val in data.iter_mut() {
        debug_assert!(*val >= 0, "expected non-negative value");
        *val *= 2;
    }
}

fn divide(a: i32, b: i32) -> i32 {
    assert!(b != 0, "division by zero");
    a / b
}
```

## When to Apply
- C code uses `assert()` from `<assert.h>` for invariant checking
- Precondition checks at function entry
- Loop invariant assertions
- Key insight: use `assert!` for conditions that must always hold (checked in release), `debug_assert!` for expensive checks only needed during development. Null pointer asserts often become unnecessary when using `&T` references
