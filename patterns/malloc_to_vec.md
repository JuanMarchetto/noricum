# Pattern: malloc/free to Vec or Box

## C Pattern
```c
int *create_array(int n) {
    int *arr = (int *)malloc(n * sizeof(int));
    if (!arr) return NULL;
    for (int i = 0; i < n; i++) arr[i] = i;
    return arr;
}
// caller must free(arr)
```

## Rust Equivalent (Vec)
```rust
fn create_array(n: usize) -> Vec<i32> {
    (0..n as i32).collect()
}
```

## Rust Equivalent (Box for single values)
```rust
fn create_value(x: i32) -> Box<i32> {
    Box::new(x)
}
```

## When to Apply
- malloc + free pairs for dynamic arrays -> Vec
- malloc + free for single heap values -> Box
- realloc patterns -> Vec with push/resize
- Ownership is clear and transferred to caller
