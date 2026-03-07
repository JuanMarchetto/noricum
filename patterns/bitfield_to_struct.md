# Pattern: Bitfield/Bitmask to Struct with Methods

## C Pattern
```c
#define FLAG_READ    0x01
#define FLAG_WRITE   0x02
#define FLAG_EXEC    0x04
#define FLAG_HIDDEN  0x08

typedef unsigned int Permissions;

Permissions make_perms(int read, int write, int exec) {
    Permissions p = 0;
    if (read)  p |= FLAG_READ;
    if (write) p |= FLAG_WRITE;
    if (exec)  p |= FLAG_EXEC;
    return p;
}

int has_read(Permissions p) { return (p & FLAG_READ) != 0; }
int has_write(Permissions p) { return (p & FLAG_WRITE) != 0; }
int can_execute(Permissions p) { return (p & FLAG_EXEC) != 0; }
```

## Rust Equivalent
```rust
#[derive(Debug, Clone, Copy, Default)]
struct Permissions {
    read: bool,
    write: bool,
    exec: bool,
    hidden: bool,
}

impl Permissions {
    fn new(read: bool, write: bool, exec: bool) -> Self {
        Self { read, write, exec, hidden: false }
    }

    fn has_read(self) -> bool { self.read }
    fn has_write(self) -> bool { self.write }
    fn can_execute(self) -> bool { self.exec }
}
```

## When to Apply
- C code uses `#define` constants for bit flags
- Bitwise OR (`|`) to combine flags, AND (`&`) to test
- `typedef unsigned int` for flag sets
- Key insight: for small flag sets, a struct with `bool` fields is clearer. For larger sets or when binary compatibility matters, use the `bitflags` crate
