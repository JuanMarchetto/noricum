# Pattern: goto to Structured Control Flow

## C Pattern
```c
int process(const char *input) {
    int result = -1;
    char *buf = NULL;

    buf = malloc(strlen(input) + 1);
    if (!buf) goto cleanup;

    strcpy(buf, input);
    if (strlen(buf) == 0) goto cleanup;

    result = (int)strlen(buf);

cleanup:
    free(buf);
    return result;
}
```

## Rust Equivalent
```rust
fn process(input: &str) -> Option<usize> {
    if input.is_empty() {
        return None;
    }
    Some(input.len())
}
```

## When to Apply
- C code uses `goto cleanup` for resource cleanup before return
- Error handling with `goto error` labels
- Multiple exit points that all jump to a common cleanup block
- Key insight: Rust's RAII (Drop) eliminates cleanup gotos; remaining gotos map to early return, `loop { break }`, or `?` operator
