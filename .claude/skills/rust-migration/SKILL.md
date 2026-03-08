---
name: rust-migration
description: Rust conversion patterns and idioms reference. Use when converting C code to idiomatic Rust.
---

# C to Rust Migration Patterns

## Pipeline Process
1. **Analysis** — classify difficulty, detect patterns, plan strategy
2. **Translation** — C → safe Rust (LLM, optionally with C2Rust context)
3. **Quality gate** — re-translate if >5 unsafe blocks (temp 0.5)
4. **Validation** — compile + diff test + idiomatic score
5. **Repair loop** — fix errors preserving quality floor (P0)
6. **Best-version fallback** — keep highest-quality version even if doesn't compile (P1)

## Memory Management
```rust
// malloc/free -> Vec (arrays)
let arr: Vec<i32> = vec![0; n];

// Single heap alloc -> Box
let val = Box::new(42);

// Linked list with malloc -> Vec (flatten)
// DO NOT use Box<Node> chains — use Vec<T> for cache friendliness
let items: Vec<Item> = Vec::new();

// realloc -> Vec::resize or push
v.resize(new_size, 0);
```

## Data Model Migration (learned from cJSON)
```rust
// C type tags + unions -> Rust enum with variants
// typedef struct { int type; union { double num; char *str; } } Value;
enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}
// Key: linked list (next/prev pointers) -> Vec
// Key: type tags (cJSON_Number, cJSON_String) -> enum variants
// Key: malloc/free -> RAII (automatic Drop)
// Key: IsReference flag (shared ownership) -> Clone
```

## Error Handling
```rust
// C error codes -> Result<T, E>
#[derive(Debug, thiserror::Error)]
enum ParseError {
    #[error("unexpected token at position {0}")]
    UnexpectedToken(usize),
    #[error("unterminated string")]
    UnterminatedString,
}

// C NULL returns -> Option<T> or Result
fn find(key: &str) -> Option<&JsonValue> { ... }
```

## String Handling
```rust
// const char* -> &str (borrowed, zero-copy)
fn process(s: &str) -> usize { s.len() }

// char* that gets modified -> String
fn build() -> String { format!("hello {}", "world") }

// String escaping (learned from cJSON):
// Must handle \", \\, \n, \t, \r, \b, \f, and \uXXXX (UTF-16 surrogate pairs)
fn escape_string(s: &str) -> String { ... }
```

## Function Pointer Migration (learned from genann)
```rust
// C function pointer typedef -> Rust enum dispatch
// typedef double (*genann_actfun)(const struct genann *ann, double a);
#[derive(Clone, Copy, PartialEq)]
enum ActivationFn { Sigmoid, SigmoidCached, Threshold, Linear }

impl ActivationFn {
    fn apply(&self, ann: &Genann, a: f64) -> f64 {
        match self {
            ActivationFn::Sigmoid => sigmoid(a),
            // ...
        }
    }
}
// Key: enum dispatch avoids Fn trait objects and self-referential closures
// Key: use usize (not i32) for fields used as array indices — avoids `as usize` casts
// Key: single-malloc with internal pointers → separate Vec<f64> fields
```

## Pointer Patterns
```rust
// const T* + len -> &[T]
fn sum(data: &[i32]) -> i32 { data.iter().sum() }

// T* + len (mutable) -> &mut [T]
fn fill(data: &mut [i32], val: i32) { data.fill(val); }

// T* nullable -> Option<&T>
fn maybe_read(p: Option<&i32>) -> i32 { p.copied().unwrap_or(0) }

// void* -> generics or enum (NEVER transmute)
fn process<T: AsRef<[u8]>>(data: T) { ... }
```

## Struct Migration
```rust
// C struct with constructor/destructor -> Rust struct + impl
// Drop is automatic for Vec/String/Box - no manual free needed
struct Buffer {
    data: Vec<u8>,
}

impl Buffer {
    fn new(capacity: usize) -> Self {
        Self { data: Vec::with_capacity(capacity) }
    }
}
```

## Control Flow
```rust
// goto cleanup -> ? operator or Drop
// goto error -> Result + ? propagation
// goto retry -> loop { ... break; }
// switch/case -> match (exhaustive)
// Duff's device / coroutine via switch -> explicit state machine with match loop
//   (this is the HARDEST pattern — requires full algorithm understanding)
```

## Numeric Formatting (learned from cJSON)
```rust
// C printf("%.17g", num) doesn't map directly to Rust format!("{}", num)
// Rust adds ".0" for whole numbers (C: "1" vs Rust: "1.0")
// Solution: custom format_g() function that strips trailing zeros
fn format_g(val: f64) -> String {
    if val.fract() == 0.0 && val.abs() < 1e15 {
        return (val as i64).to_string();
    }
    let s = format!("{:.17e}", val);
    // ... strip trailing zeros, convert from Rust sci notation to C-style
}
```

## Common Pitfalls (from production migrations)
- `bool` vs `int`: C returns 0/1 as int; Rust `bool` prints `true/false` → keep as `i32`
- Integer overflow: C wraps silently; Rust panics in debug → use `wrapping_add` etc.
- Printf format: `%d` → `{}`, `%s` → `{}`, `%f` → `{:.6}`, `%g` → custom format_g()
- Signed/unsigned: C implicit conversion; Rust requires explicit `as` casts
- Recursive data structures: need `Box<T>` for indirection in Rust
- Bit manipulation: same operators but explicit types needed (`u32`, `i32`)
- **Use `usize` for array dimensions**: C uses `int` for sizes, but Rust indexing requires `usize`. Using `i32` forces `as usize` on every array access, which penalizes idiomatic score heavily. Prefer `usize` from the start.
- **glibc `srand`/`rand` determinism**: C's `rand()` uses glibc TYPE_3 (degree-31) PRNG. For diff-test to pass, must reimplement the exact PRNG algorithm, not use Rust's `rand` crate.
- **`a > 0` returns double in C**: This is an implicit bool-to-double cast. In Rust: `if a > 0.0 { 1.0 } else { 0.0 }`
