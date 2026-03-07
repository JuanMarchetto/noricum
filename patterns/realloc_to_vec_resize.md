# Pattern: realloc to Vec Growth

## C Pattern
```c
typedef struct {
    int *data;
    size_t len;
    size_t capacity;
} DynArray;

void dynarray_init(DynArray *arr) {
    arr->capacity = 4;
    arr->data = malloc(arr->capacity * sizeof(int));
    arr->len = 0;
}

void dynarray_push(DynArray *arr, int val) {
    if (arr->len >= arr->capacity) {
        arr->capacity *= 2;
        arr->data = realloc(arr->data, arr->capacity * sizeof(int));
    }
    arr->data[arr->len++] = val;
}

void dynarray_free(DynArray *arr) {
    free(arr->data);
    arr->data = NULL;
    arr->len = 0;
    arr->capacity = 0;
}
```

## Rust Equivalent
```rust
fn example() {
    let mut arr: Vec<i32> = Vec::with_capacity(4);
    arr.push(42);
    arr.push(99);
    // Vec handles reallocation automatically
    // Drop handles deallocation automatically
}
```

## When to Apply
- C code manually tracks `data`, `len`, `capacity` triple
- Uses `realloc` to grow a buffer when capacity is exceeded
- Doubling strategy (`capacity *= 2`) for amortized O(1) push
- Key insight: `Vec<T>` is exactly this pattern built into the language. Use `Vec::with_capacity` to pre-allocate, `push` to grow, no manual free needed
