# Pattern: Error Codes to Result

## C Pattern
```c
typedef enum { OK = 0, ERR_INVALID = -1, ERR_OVERFLOW = -2 } Error;

Error compute(int input, int *output) {
    if (!output) return ERR_INVALID;
    *output = input * 2;
    return OK;
}
```

## Rust Equivalent
```rust
#[derive(Debug, thiserror::Error)]
enum ComputeError {
    #[error("invalid input")]
    Invalid,
    #[error("overflow")]
    Overflow,
}

fn compute(input: i32) -> Result<i32, ComputeError> {
    Ok(input * 2)
}
```

## When to Apply
- C function returns an int/enum error code
- Success value passed through pointer parameter
- Multiple error conditions with distinct codes
