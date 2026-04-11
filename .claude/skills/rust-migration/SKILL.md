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

## Dependency Budget — decide at the start of every migration

Before writing any translation, pick one of three modes:

| Mode | Policy | Binary vs C | Source vs C | Use case |
|---|---|---:|---:|---|
| **FREE** | delegate algorithms to best-in-class crates | 3-5x | 0.15-0.25x | research spikes, demos, reference implementations |
| **MATCHING** | match the C original's dep count (vendor primitives as source) | 2-3x | 0.3-0.5x | drop-in replacements |
| **ZERO** | no external runtime deps at all (everything in-tree) | ~2x | 0.4-0.7x | embedded, WASM, defense, regulated |

**Initial-instructions template** for an interactive spike:

> "Migrate {target}.c to Rust. Target: {N}-hour budget, byte-match with C oracle on {K} fixtures. **Dependency budget: {FREE|MATCHING|ZERO}.** [If MATCHING/ZERO:] Do not pull in `{example_crate}` or any wrapping crate; vendor the primitives as source under `src/{subdir}/`. Run `cargo tree` at each commit and confirm only the spike crate is listed."

**Full recipe for each mode + conversion paths:** `docs/methodology/dependency-budget.md`.

**Enforcement:** `tools/spike/dep-gate.sh` reads the mode from `SPIKE.md` (or `SPIKE_DEP_MODE` env var) and fails the commit if runtime deps exceed the allowed count. `tools/spike/binary-size-gate.sh <rust_bin> <c_bin> [max_ratio]` measures against a C baseline and fails if the ratio exceeds the threshold (default 3.5x).

## Binary Format Container Migration (learned from miniz_zip.c interactive spike)

**Pattern:** For C libraries that wrap a well-specified binary format (ZIP, PNG, ELF, gzip, tar) around an algorithm-heavy core (deflate, DCT, LZW), the migration splits into three parts:

1. **Container layer** — parse/emit the format's headers, tables, and records. Translate this by hand using the format spec as ground truth.
2. **Algorithm layer** — compression, hashing, encryption. In FREE mode, delegate to an existing Rust crate (`flate2`, `crc32fast`, `aes`, `sha1/2`). In MATCHING/ZERO mode, vendor the primitives as source.
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

## Architectural Elimination — free features via Rust design choices

When porting a C container format, audit every complexity source in the C code and classify it:

- **Data-flow driven** (compression, CRC, actual algorithms) — must be ported or delegated.
- **Architecture-driven** (branches that exist because of a particular data flow choice the C code made) — candidate for elimination. Choose a different Rust architecture and the feature disappears.
- **Legacy baggage** (5 wrapper variants, 32-bit fallbacks, deprecated aliases) — don't port unless consumers need them.

**Two validated eliminations from the miniz_zip spike:**

1. **Data descriptors (flag bit 3, sizes-after-data):** miniz_zip has ~60 LOC to parse data descriptor blocks. The Rust reader uses the central directory as the authoritative source for all sizes/CRC/offsets, so the local file header's placeholder zeros are simply ignored. Zero new code was written. The feature became architectural.

2. **`Box<dyn Read + Write + Seek>` trait object:** miniz_zip uses function-pointer callbacks for source abstraction (~200 LOC of indirection). Run 14 of the autonomous pipeline tried to port this to a trait object with multiple non-auto traits — which is INVALID Rust and killed the run. The interactive spike replaced it with a concrete `ZipSource` enum that implements the three traits by delegation. ~160 LOC collapsed, failure mode became impossible by construction.

**Other candidates that collapsed on the same port:** 32-bit fallback branches (use u64 everywhere), 8 writer init variants reduced to `ZipWriter::create(path)`, reader/writer mode flag inside one struct → two distinct types where misuse is a compile error.

**Recognition rule:** any C feature that exists because of constrained-system micro-optimizations (64KB stack, no malloc, 32-bit fields) or pre-zip64 backward compatibility is a prime elimination candidate. Modern Rust targets don't care about those constraints.

**Document eliminations in commit messages** as `ELIMINATED via [architecture choice]` — preserves the audit trail and explains to future readers why the Rust port is smaller.

## Fixture Generation via Dev-Dependencies

For binary formats with an existing idiomatic Rust crate, use the crate as a **dev-dependency** to generate test fixtures at test time. Do NOT commit large binary fixtures to the repo, and do NOT hand-craft format bytes unless you're testing a specific edge case.

```toml
[dev-dependencies]
zip = { version = "2", default-features = false,
        features = ["deflate-flate2", "flate2", "aes-crypto", "time"] }
```

```rust
#[test]
fn extension_test_zip64_via_zip_rs() {
    use zip::CompressionMethod;
    use zip::write::{SimpleFileOptions, ZipWriter};

    let path = tmp_archive("zip64");
    let file = std::fs::File::create(&path).unwrap();
    let mut zw = ZipWriter::new(file);
    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true);               // forces zip64 extras
    for i in 0..3 {
        zw.start_file(format!("entry{i}.txt"), opts).unwrap();
        zw.write_all(format!("content {i}").as_bytes()).unwrap();
    }
    zw.finish().unwrap();

    // Test the archive against the Rust reader under development.
    let mut rust = RustZipReader::open(&path).unwrap();
    // ...
}
```

**Tradeoff:** the dev-dep pulls transitive crates (flate2, miniz_oxide, aes, sha1, hmac, pbkdf2) into the dev build tree. Keep `default-features = false` and enumerate the exact features you need. The runtime binary is unaffected.

**When to hand-craft instead:** when you need to test an exact byte-level edge case (e.g., "flag bit 3 set with the optional descriptor magic, followed by sentinel sizes in the central dir") that the dev-dep crate doesn't produce by default. The spike's `extension_test_data_descriptor_reader_handles_bit3` handcrafts the archive byte-by-byte for this reason.

## Real-World Fuzzing via `SPIKE_REAL_ARCHIVES` Pattern

Add an `#[ignore]`d integration test that runs only when an env var points to a directory of real-world format artifacts. Default `cargo test` stays hermetic; the fuzzing pass is opt-in.

```rust
#[test]
#[ignore = "requires SPIKE_REAL_ARCHIVES env var pointing to a directory of .zip files"]
fn real_world_diff_test() {
    let dir = match std::env::var("SPIKE_REAL_ARCHIVES") {
        Ok(d) => PathBuf::from(d),
        Err(_) => return,
    };
    // Walk every file with a format-matching extension, open with both
    // oracle and Rust reader, byte-compare.
    const ZIP_EXTS: &[&str] = &["zip", "jar", "war", "ear", "apk", "docx",
                                 "xlsx", "pptx", "epub", "kmz"];
    // ...
}
```

**How to run:**
```sh
SPIKE_REAL_ARCHIVES=/path/to/dir cargo test --test differential_zip \
    real_world -- --ignored --nocapture
```

**What to download:** a mix of producers to cover format variability. For ZIP specifically, the miniz_zip spike used:
- 3 Windows binary releases (sharkdp tooling: bat, fd, hyperfine)
- 3 Rust source tag archives (ripgrep, serde, tokio)
- 1 Java archive from Maven Central (commons-lang3.jar)
- 1 Java archive, Office document, or APK for XML-heavy binary content
- 2 large source archives (Go 14k entries, Python 5k entries) for scale

Result on that corpus: **9/9 archives byte-matched, 21,331 total entries extracted** against the C oracle. Zero divergences.

**Failure modes to expect:** a mismatch here is a genuine bug in the reader. An oracle-only failure (C reader cannot open but Rust can) might mean the fixture isn't actually the claimed format (happened once with a docx that was actually HTML due to a URL redirect).

## WinZip AES Counter Gotcha

WinZip's AES extension uses AES-CTR mode with a NON-STANDARD counter layout that differs from NIST AES-CTR. The counter:
- Starts at 1, not 0
- Is LE-encoded in the LOW 4 bytes of the 16-byte counter block
- Upper 12 bytes are zero and never change

This means you **cannot** use the `ctr` crate directly — it defaults to a different layout. Roll your own:

```rust
fn decrypt_ctr<C>(key: &[u8], data: &mut [u8])
where
    C: BlockEncrypt + KeyInit,
{
    let cipher = C::new_from_slice(key).expect("key length");
    const BLOCK: usize = 16;
    let mut counter: u32 = 1;  // ZIP-specific: starts at 1
    let mut i = 0;
    while i < data.len() {
        let mut block = [0u8; BLOCK];
        block[..4].copy_from_slice(&counter.to_le_bytes());  // LE in low 4 bytes
        // upper 12 bytes stay zero
        let mut keystream = GenericArray::clone_from_slice(&block);
        cipher.encrypt_block(&mut keystream);
        let take = core::cmp::min(BLOCK, data.len() - i);
        for j in 0..take { data[i + j] ^= keystream[j]; }
        counter = counter.wrapping_add(1);
        i += BLOCK;
    }
}
```

**Related AES constants:** salt length = key length / 2, password verification = 2 bytes, HMAC-SHA1 truncated to 10 bytes, PBKDF2 iterations = 1000, AE-2 variant sets central dir CRC32 to zero (skip CRC check when 0).

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
