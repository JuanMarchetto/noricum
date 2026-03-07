# Pattern: fopen/fread/fwrite to std::fs

## C Pattern
```c
int read_file(const char *path, char *buf, size_t max_len) {
    FILE *fp = fopen(path, "r");
    if (!fp) return -1;

    size_t n = fread(buf, 1, max_len - 1, fp);
    buf[n] = '\0';
    fclose(fp);
    return (int)n;
}

int write_file(const char *path, const char *data) {
    FILE *fp = fopen(path, "w");
    if (!fp) return -1;

    size_t len = strlen(data);
    size_t written = fwrite(data, 1, len, fp);
    fclose(fp);
    return (written == len) ? 0 : -1;
}
```

## Rust Equivalent
```rust
use std::fs;
use std::io;

fn read_file(path: &str) -> io::Result<String> {
    fs::read_to_string(path)
}

fn write_file(path: &str, data: &str) -> io::Result<()> {
    fs::write(path, data)
}
```

## When to Apply
- C code uses `fopen`/`fclose` for file handles
- `fread`/`fwrite` for binary I/O, `fgets`/`fputs` for text
- Error checking via `NULL` return from `fopen`
- Key insight: `std::fs::read_to_string` and `std::fs::write` handle open+read+close atomically; for streaming use `BufReader`/`BufWriter`
