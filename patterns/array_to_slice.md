# Pattern: C Array + Length to Slice

## C Pattern
```c
int sum(const int *arr, size_t len) {
    int total = 0;
    for (size_t i = 0; i < len; i++) {
        total += arr[i];
    }
    return total;
}

void fill(int *arr, size_t len, int value) {
    for (size_t i = 0; i < len; i++) {
        arr[i] = value;
    }
}

int find(const int *arr, size_t len, int target) {
    for (size_t i = 0; i < len; i++) {
        if (arr[i] == target) return (int)i;
    }
    return -1;
}
```

## Rust Equivalent
```rust
fn sum(arr: &[i32]) -> i32 {
    arr.iter().sum()
}

fn fill(arr: &mut [i32], value: i32) {
    arr.fill(value);
}

fn find(arr: &[i32], target: i32) -> Option<usize> {
    arr.iter().position(|&x| x == target)
}
```

## When to Apply
- C functions take `(const T *arr, size_t len)` or `(T *arr, int n)` parameter pairs
- Array traversal with index variable `for (i = 0; i < len; i++)`
- Read-only access -> `&[T]`, mutable access -> `&mut [T]`
- Key insight: Rust slices encode pointer + length as a single fat pointer; use iterator methods (`.iter()`, `.sum()`, `.position()`) instead of manual indexing
