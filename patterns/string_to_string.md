# Pattern: C Strings to Rust String

## C Pattern
```c
#include <string.h>
#include <stdlib.h>

char *concat(const char *a, const char *b) {
    size_t la = strlen(a), lb = strlen(b);
    char *result = (char *)malloc(la + lb + 1);
    if (!result) return NULL;
    strcpy(result, a);
    strcat(result, b);
    return result;
}

int compare(const char *a, const char *b) {
    return strcmp(a, b);
}
```

## Rust Equivalent
```rust
fn concat(a: &str, b: &str) -> String {
    format!("{}{}", a, b)
}

fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    a.cmp(b)
}
```

## When to Apply
- C function uses `char *` for string manipulation
- Uses `strlen`, `strcpy`, `strcat`, `strcmp`, `strncpy`, `strdup`
- Allocates strings with `malloc` and manually null-terminates
- Key insight: `const char *` maps to `&str`, owned `char *` maps to `String`
