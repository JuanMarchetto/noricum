---
name: rust-migration
description: Rust conversion patterns and idioms reference. Use when converting C code to idiomatic Rust.
---

# C to Rust Migration Patterns

## 2-Step Process (SACTOR approach)
1. C -> unsafe Rust (C2Rust mechanical translation)
2. unsafe Rust -> safe idiomatic Rust (LLM refinement)

## Memory Management
```rust
// malloc/free -> Vec
let arr: Vec<i32> = vec![0; n];  // replaces malloc(n * sizeof(int)) + memset

// Single heap alloc -> Box
let val = Box::new(42);

// realloc -> Vec::resize or push
let mut v = Vec::with_capacity(initial);
v.resize(new_size, 0);
```

## Error Handling
```rust
// C error codes -> Result
#[derive(Debug, thiserror::Error)]
enum MyError {
    #[error("null pointer")]
    NullPtr,
    #[error("out of range")]
    OutOfRange,
}

fn safe_divide(a: i32, b: i32) -> Result<i32, MyError> {
    if b == 0 { return Err(MyError::OutOfRange); }
    Ok(a / b)
}
```

## String Handling
```rust
// const char* -> &str (borrowed, no allocation)
fn process(s: &str) -> usize { s.len() }

// char* that gets modified -> String
fn build() -> String { format!("hello {}", "world") }

// char[] fixed buffer -> String or ArrayString
```

## Pointer Patterns
```rust
// const T* + len -> &[T]
fn sum(data: &[i32]) -> i32 { data.iter().sum() }

// T* + len (mutable) -> &mut [T]
fn fill(data: &mut [i32], val: i32) { data.fill(val); }

// T* nullable -> Option<&T>
fn maybe_read(p: Option<&i32>) -> i32 { p.copied().unwrap_or(0) }
```

## Struct Migration
```rust
// C struct with constructor/destructor -> Rust struct + impl + Drop
struct Buffer {
    data: Vec<u8>,
}

impl Buffer {
    fn new(capacity: usize) -> Self {
        Self { data: Vec::with_capacity(capacity) }
    }
    fn append(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }
}
// Drop is automatic for Vec - no manual free needed
```

## Control Flow
```rust
// goto cleanup -> ? operator or Drop
// goto error -> Result + ? propagation
// goto retry -> loop { ... break; }
```
