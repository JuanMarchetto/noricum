# Pattern: Function Pointer Callback to Closure/Trait

## C Pattern
```c
typedef int (*compare_fn)(const void *, const void *);

void sort(int *arr, int n, compare_fn cmp) {
    for (int i = 0; i < n - 1; i++) {
        for (int j = 0; j < n - i - 1; j++) {
            if (cmp(&arr[j], &arr[j+1]) > 0) {
                int tmp = arr[j];
                arr[j] = arr[j+1];
                arr[j+1] = tmp;
            }
        }
    }
}

int ascending(const void *a, const void *b) {
    return *(const int *)a - *(const int *)b;
}

void apply_each(int *arr, int n, void (*fn)(int *)) {
    for (int i = 0; i < n; i++) {
        fn(&arr[i]);
    }
}
```

## Rust Equivalent
```rust
fn sort<F: Fn(&i32, &i32) -> std::cmp::Ordering>(arr: &mut [i32], cmp: F) {
    arr.sort_by(cmp);
}

fn ascending(a: &i32, b: &i32) -> std::cmp::Ordering {
    a.cmp(b)
}

fn apply_each<F: FnMut(&mut i32)>(arr: &mut [i32], mut f: F) {
    for item in arr.iter_mut() {
        f(item);
    }
}
```

## When to Apply
- C code uses function pointers (`int (*fn)(...)`) as callbacks
- `typedef` for function pointer types
- `void *` used to pass arbitrary data alongside callback (context)
- Key insight: function pointers map to generics with `Fn`/`FnMut`/`FnOnce` traits
