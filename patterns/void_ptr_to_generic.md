# Pattern: void* to Generics or Trait Objects

## C Pattern
```c
typedef struct {
    void *data;
    size_t elem_size;
    size_t len;
    size_t capacity;
} GenericArray;

void array_push(GenericArray *arr, const void *elem) {
    if (arr->len >= arr->capacity) {
        arr->capacity *= 2;
        arr->data = realloc(arr->data, arr->capacity * arr->elem_size);
    }
    memcpy((char *)arr->data + arr->len * arr->elem_size, elem, arr->elem_size);
    arr->len++;
}

void *array_get(const GenericArray *arr, size_t index) {
    return (char *)arr->data + index * arr->elem_size;
}
```

## Rust Equivalent
```rust
struct GenericArray<T> {
    data: Vec<T>,
}

impl<T> GenericArray<T> {
    fn new() -> Self {
        Self { data: Vec::new() }
    }

    fn push(&mut self, elem: T) {
        self.data.push(elem);
    }

    fn get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }
}
```

## When to Apply
- C code uses `void *` for generic/polymorphic data structures
- Manual `memcpy` with `elem_size` for type-erased operations
- Casting `void *` back to concrete types at usage sites
- Key insight: Rust generics (`<T>`) replace `void *` with full type safety at zero runtime cost. For heterogeneous collections, use trait objects (`Box<dyn Trait>`)
