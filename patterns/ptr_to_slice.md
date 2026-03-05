# Pattern: Pointer + Length to Slice

## C Pattern
```c
void process(const char *data, size_t len) {
    for (size_t i = 0; i < len; i++) {
        // use data[i]
    }
}
```

## Rust Equivalent
```rust
fn process(data: &[u8]) {
    for &byte in data {
        // use byte
    }
}
```

## When to Apply
- C function takes a pointer + length pair
- Pointer is not modified (const)
- Access is within bounds (0..len)
