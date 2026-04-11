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

## State Machine Migration (learned from http-parser)
```rust
// C goto-based state machine (58 states, 1515 lines) -> clean line-based parsing
// Key: DON'T port the goto mess line-by-line. Rewrite at a higher abstraction level.

// C pattern:
//   switch(parser->state) {
//     case s_req_method: ... goto reexecute;
//     case s_req_url: ... goto reexecute;
//   }
// Rust pattern: parse complete lines with find_crlf(), process phases sequentially
fn find_crlf(data: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 1 < data.len() {
        if data[i] == b'\r' && data[i + 1] == b'\n' { return Some(i); }
        i += 1;
    }
    None
}
// Then: parse_request_line() -> parse_headers() -> parse_body()
// Each returns the position after processing, maintaining byte-exact count.

// C global mutable callback state -> thread_local! { RefCell<T> }
use std::cell::RefCell;
thread_local! {
    static G_STATE: RefCell<TestState> = RefCell::new(TestState::new());
}
// Callbacks are regular functions accessing the thread_local (no unsafe needed)
// This mirrors C's static globals but is safe in Rust
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

## Binary Format Container Migration (learned from miniz_zip.c interactive spike)

**Pattern:** For C libraries that wrap a well-specified binary format (ZIP, PNG, ELF, gzip, tar) around an algorithm-heavy core (deflate, DCT, LZW), the migration splits into three parts:

1. **Container layer** — parse/emit the format's headers, tables, and records. Translate this by hand using the format spec as ground truth.
2. **Algorithm layer** — compression, hashing, encryption. Delegate to an existing Rust crate (`flate2`, `crc32fast`, `aes`, `sha1/2`).
3. **Oracle differential test** — compile the C library, expose a thin wrapper, compare Rust output against it on a fixture corpus.

```rust
// ZIP central directory entry — byte layout from APPNOTE.TXT 4.3.12
// Use u64 throughout for sizes/offsets to get zip64 compatibility for free
#[derive(Clone, Debug)]
pub struct CentralDirHeader {
    pub version_made_by: u16,
    pub version_needed: u16,
    pub flags: u16,
    pub compression_method: CompressionMethod,
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub local_header_offset: u64,
    pub file_name: String,
    pub extra_field: Vec<u8>,
    pub comment: String,
    // ... other fields per spec
}

// Parse little-endian bytes with helper functions, not derive-based serde
fn read_u32_le(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]])
}
```

**Rule:** Never derive the Rust contract from C struct layouts alone. C structs have accidental complexity (padding, prefix conventions, legacy fields) that pollutes the contract. Instead, clone an existing idiomatic Rust crate for the domain and steal its types.

```rust
// Source abstraction: concrete enum, not trait object
// This replaces Box<dyn Read + Write + Seek> which is INVALID Rust
// (trait objects can have at most one non-auto trait)
pub enum ZipSource {
    File(std::fs::File),
    Mem(std::io::Cursor<Vec<u8>>),
}

impl Read for ZipSource { /* delegate to inner */ }
impl Write for ZipSource { /* delegate to inner */ }
impl Seek for ZipSource { /* delegate to inner */ }
```

**Algorithm delegation example (miniz_zip, 2200+ LOC saved):**

```rust
// Reader side
use flate2::read::DeflateDecoder;
let taken = (&mut source).take(compressed_size);
let mut decoder = DeflateDecoder::new(taken);
let mut out = Vec::with_capacity(uncompressed_size as usize);
decoder.read_to_end(&mut out).map_err(|_| ZipError::DecompressionFailed)?;

// Writer side
use flate2::write::DeflateEncoder;
use flate2::Compression;
let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
encoder.write_all(data)?;
let compressed = encoder.finish()?;
```

**The differential test scope trade-off:** delegating compression means the Rust writer won't produce byte-identical archives to the C library (flate2 and miniz-oxide make different tie-breaking choices in deflate). The diff test asserts on extracted content + metadata (filename, size, crc32, extracted bytes), NOT on raw archive bytes. This is explicitly by design and should be documented in the test and commit messages.

**C oracle harness pattern (mandatory for this approach):**

```rust
// build.rs
fn main() {
    cc::Build::new()
        .file("miniz.c").file("miniz_tdef.c").file("miniz_tinfl.c")
        .file("miniz_zip.c").file("wrapper.c")
        .include(".")
        .compile("miniz_wrapper");
}

// wrapper.h — thin opaque-handle wrapper
typedef void* zip_handle_t;
zip_handle_t wr_reader_open(const char* filename);
int          wr_reader_extract(zip_handle_t h, int idx, void* buf, size_t cap);
// ... etc

// lib.rs — Rust FFI declarations
#[link(name = "miniz_wrapper", kind = "static")]
unsafe extern "C" {
    pub fn wr_reader_open(filename: *const c_char) -> ZipHandle;
    // ...
}
```

Result: C implementation available as ground truth via `unsafe extern` block, Rust implementation written separately against the same spec, both extract the same fixtures, byte-for-byte equality checked in tests.

**Key invariants from the miniz_zip spike** (LOC savings tell the story):
- C source: 4895 LOC (miniz_zip.c) + 2200 LOC (miniz_tdef.c + miniz_tinfl.c) + 646 LOC (miniz.c) = 7741 LOC total
- Rust spike: 337 LOC contract + 453 LOC reader + 319 LOC writer + 48 LOC FFI = **1157 LOC (0.15x of the C)**
- Savings breakdown: deflate/inflate → flate2 (saves ~2200 LOC), CRC32 → crc32fast (saves ~80 LOC), zlib-compat wrappers → not needed (saves ~400 LOC), heap/cfile/mem variants → single ZipSource enum (saves ~300 LOC), legacy 32-bit fallback paths → u64 throughout (saves ~200 LOC)
- Test count: 28 tests, 0 unsafe in translated code (only FFI block is unsafe), 0 clippy warnings

**Reference implementation:** `feat/interactive-spike` branch of noricum, commits `cf4a07f` through `454e560`. Read those 5 commits for the full pattern in action.

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
- **goto state machines**: Don't port byte-by-byte. Rewrite at higher abstraction (line-by-line parsing). 1515 LOC goto → ~200 LOC clean Rust. The `parsed` byte count stays correct via position tracking.
- **C callback APIs with global state**: Use `thread_local! { RefCell<T> }` instead of `unsafe static mut`. Callbacks are plain functions that access the thread_local.
- **Frontier target stochasticity**: For >2500 LOC with goto/switch state machines, the LLM pipeline converges stochastically. Manual completion is a valid strategy when LLM provides good types+structure but truncates on complex functions.
