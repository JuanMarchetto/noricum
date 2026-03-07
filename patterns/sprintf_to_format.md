# Pattern: sprintf/snprintf to format!

## C Pattern
```c
void format_message(char *buf, size_t size, const char *name, int count) {
    snprintf(buf, size, "Hello %s, you have %d items", name, count);
}

void format_hex(char *buf, size_t size, unsigned int val) {
    snprintf(buf, size, "0x%08X", val);
}

void build_path(char *buf, size_t size, const char *dir, const char *file) {
    snprintf(buf, size, "%s/%s", dir, file);
}
```

## Rust Equivalent
```rust
fn format_message(name: &str, count: i32) -> String {
    format!("Hello {}, you have {} items", name, count)
}

fn format_hex(val: u32) -> String {
    format!("0x{:08X}", val)
}

fn build_path(dir: &str, file: &str) -> String {
    format!("{}/{}", dir, file)
    // Or better: std::path::Path::new(dir).join(file)
}
```

## When to Apply
- C code uses `sprintf`, `snprintf`, or `fprintf` for string formatting
- Buffer + size parameters for formatted output
- Printf format specifiers (`%d`, `%s`, `%f`, `%x`, `%02d`, etc.)
- Key insight: `format!` returns a `String` (no buffer overflow risk), format specifiers use `{}` with optional formatting like `{:08X}`
