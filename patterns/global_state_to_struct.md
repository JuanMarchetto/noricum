# Pattern: Global State to Struct

## C Pattern
```c
static int counter = 0;
static char buffer[256];
static int initialized = 0;

void init(void) {
    counter = 0;
    memset(buffer, 0, sizeof(buffer));
    initialized = 1;
}

void increment(void) {
    if (!initialized) return;
    counter++;
    snprintf(buffer, sizeof(buffer), "count: %d", counter);
}

int get_counter(void) {
    return counter;
}

const char *get_buffer(void) {
    return buffer;
}
```

## Rust Equivalent
```rust
struct Counter {
    count: i32,
    buffer: String,
}

impl Counter {
    fn new() -> Self {
        Self {
            count: 0,
            buffer: String::new(),
        }
    }

    fn increment(&mut self) {
        self.count += 1;
        self.buffer = format!("count: {}", self.count);
    }

    fn count(&self) -> i32 {
        self.count
    }

    fn buffer(&self) -> &str {
        &self.buffer
    }
}
```

## When to Apply
- C code uses `static` global variables as module-level state
- Multiple functions read/write the same globals
- An `init()` function sets up the global state
- Key insight: group related globals into a struct with methods
