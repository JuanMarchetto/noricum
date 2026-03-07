# Pattern: atoi/strtol to str::parse

## C Pattern
```c
#include <stdlib.h>

int parse_int(const char *s) {
    return atoi(s);
}

long parse_long(const char *s) {
    char *endptr;
    long val = strtol(s, &endptr, 10);
    if (*endptr != '\0') {
        fprintf(stderr, "Invalid number: %s\n", s);
        return 0;
    }
    return val;
}

double parse_double(const char *s) {
    return atof(s);
}

unsigned long parse_hex(const char *s) {
    return strtoul(s, NULL, 16);
}
```

## Rust Equivalent
```rust
fn parse_int(s: &str) -> i32 {
    s.parse().unwrap_or(0)
}

fn parse_long(s: &str) -> Result<i64, std::num::ParseIntError> {
    s.parse()
}

fn parse_double(s: &str) -> f64 {
    s.parse().unwrap_or(0.0)
}

fn parse_hex(s: &str) -> Result<u64, std::num::ParseIntError> {
    u64::from_str_radix(s, 16)
}
```

## When to Apply
- C code uses `atoi`, `atol`, `atof` for string-to-number conversion
- `strtol`, `strtoul`, `strtod` with endptr for validated parsing
- Key insight: Rust's `.parse::<T>()` returns `Result`, so invalid input is handled safely. Use `from_str_radix` for non-decimal bases
