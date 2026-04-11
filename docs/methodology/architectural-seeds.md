# Architectural Seed Registry

Proven mappings from "C library type" to "idiomatic Rust crate whose types you should steal". Use this during the type contract phase (Hour 0) of an interactive spike to avoid re-deriving the Rust-idiomatic shape from scratch.

The pattern: clone the reference crate into a tempdir, read its `src/types.rs` and related files for 30 minutes, and copy the type architecture wholesale into your contract. The C source remains ground truth for ABI and byte layouts, but the type shape comes from the Rust-idiomatic reference.

## Entries

### ZIP containers

- **Reference:** [`zip-rs/zip2`](https://github.com/zip-rs/zip2) (the `zip` crate, v2.x)
- **Files to read:** `src/types.rs` (top-level types + file entry model), `src/spec.rs` (byte layouts of central directory, EOCD, local file header), `src/read.rs` (reader flow), `src/write.rs` (writer flow), `src/aes.rs` if AES support is needed
- **Key patterns to steal:**
  - `FixedSizeBlock` trait for POD byte-layout records
  - `Zip32CDEBlock` / `Zip32CentralDirectoryEnd` split (on-wire struct vs parsed record)
  - `Zip64CentralDirectoryEndLocator` + `Zip64CentralDirectoryEnd` when zip64 is needed
  - Entries keyed by `Box<str>` filename
  - `OnceLock<u64>` for lazy `data_start` discovery
  - Generic-over-reader via trait bounds (not `Box<dyn>`) — `<R: Read + Seek + ?Sized>`
- **Validated in:** miniz_zip.c spike (branch `feat/interactive-spike`, 2026-04-11). 7741 LOC C → 1157 LOC Rust, byte-match on 21,331 real-world entries.

### HTTP parsers

- **Reference:** [`hyperium/http`](https://github.com/hyperium/http) + [`hyperium/hyper`](https://github.com/hyperium/hyper)
- **Files to read:** `http/src/method.rs`, `http/src/status.rs`, `http/src/header/` for typed headers, `hyper/src/proto/h1/` for the line-based parser
- **Key patterns to steal:** typed HeaderName / HeaderValue rather than raw strings, Method as enum with Extension variant, HeaderMap implementation
- **Validated in:** http-parser migration (commit log in noricum git history). 3680 LOC C → 1492 LOC Rust, byte-exact on 37 tests.
- **Gotcha:** C goto state machines do NOT translate line-by-line. Rewrite at a higher abstraction (line-by-line parsing with `find_crlf`).

### SQLite adapters

- **Reference:** [`rusqlite/rusqlite`](https://github.com/rusqlite/rusqlite)
- **Files to read:** `src/types/` (the type conversion layer), `src/inner_connection.rs` (connection wrapper pattern)
- **Key patterns to steal:** `ToSql` / `FromSql` traits, `Connection::prepare` returning a Statement with lifetime tied to the connection, `Rows` iterator pattern
- **Validated in:** not validated in Noricum yet.

### JSON parsers

- **Reference:** [`serde-rs/json`](https://github.com/serde-rs/json) + [`dtolnay/miniserde`](https://github.com/dtolnay/miniserde) for the minimal case
- **Files to read:** `src/value/mod.rs` (the `Value` enum), `src/ser/`, `src/de/`
- **Key patterns to steal:** `Value::{Null, Bool, Number, String, Array, Object}` enum, recursive via `Box`, Number as its own wrapper (not bare `f64`)
- **Validated in:** cjson_combined.c migration. 520 LOC C → clean Rust with type tags → enum variants pattern.

### Regex engines

- **Reference:** [`rust-lang/regex`](https://github.com/rust-lang/regex)
- **Files to read:** `regex-syntax/src/ast/` (AST shape), `regex-automata/src/` (the compiled automaton layer)
- **Key patterns to steal:** separation between AST and compiled automaton, `Hir` intermediate representation, `Input` / `Captures` types
- **Validated in:** not validated in Noricum yet.

### Deflate / compression

- **Reference:** [`rust-lang/flate2-rs`](https://github.com/rust-lang/flate2-rs) (the wrapper) + [`Frommi/miniz_oxide`](https://github.com/Frommi/miniz_oxide) (the pure-Rust deflate impl)
- **Files to read:** `miniz_oxide/src/deflate/` and `miniz_oxide/src/inflate/` for the pure-Rust implementation when you need Mode-2/3 (zero-dep, vendored)
- **Key patterns to steal:** the split between "state machine" and "bitstream I/O", hand-rolled bit reader with a u64 accumulator, `StreamResult` / `TINFLStatus` enum for parser state
- **Validated in:** miniz_zip.c spike used `flate2` as runtime dep (Mode-1 FREE). Vendoring miniz_oxide for Mode-2 MATCHING has not been validated yet.

### C string / buffer handling

- **Reference:** [`rust-lang/rust` std](https://github.com/rust-lang/rust/tree/master/library/std/src/ffi) — `CString`, `CStr`, `OsString`, `OsStr`
- **Pattern:** for nul-terminated C strings, `CString::new(bytes)?` / `CStr::from_ptr(ptr)`. For filesystem paths that may not be UTF-8, `OsString`.
- **Gotcha:** C code that uses `char*` for binary data (not strings) should map to `Vec<u8>` / `&[u8]`, NOT `String`. Check the C code: does it call `strlen()`? If yes, it's a string. If no, it's bytes.

### Linear algebra / math kernels

- **Reference:** [`rust-ndarray/ndarray`](https://github.com/rust-ndarray/ndarray) or [`nalgebra`](https://github.com/dimforge/nalgebra)
- **Key pattern:** owned vs borrowed array types (`Array` vs `ArrayView`), trait-driven element access
- **Gotcha:** C math kernels often use raw `double*` + length + stride. Rust equivalent is `&[f64]` for contiguous or a slice-of-slices for 2D. Don't port the stride parameter unless the kernel actually uses non-unit stride.

### Neural network primitives

- **Reference:** [`huggingface/candle`](https://github.com/huggingface/candle) or [`sonos/tract`](https://github.com/sonos/tract)
- **Validated in:** genann.c migration (642 LOC C → 721 LOC Rust). Used enum dispatch for activation functions instead of function pointers.
- **Key pattern to steal:** activation functions as an enum with `apply(x)` method, not function pointers. Avoids unsafe + trait objects.

### Bit manipulation / encoding

- **Reference:** [`bitvec/bitvec`](https://github.com/ferrilab/bitvec) for bit-level access
- **Gotcha:** C code that does `uint32_t x; x |= (1 << n)` translates to Rust `x |= 1u32 << n` (same operator, explicit types). Don't use bitvec unless the access pattern is genuinely non-aligned or requires bit-by-bit iteration.

## How to add an entry

When you discover a new architectural seed during a spike, add an entry to this file with:

- **Reference:** crate name + GitHub URL
- **Files to read:** specific paths that contain the type shape
- **Key patterns to steal:** 3-5 bullets naming the concrete patterns
- **Validated in:** which Noricum spike/migration exercised it
- **Gotcha:** anything that bit the first migration that would bite future ones

One entry per C library category. If the same Rust crate works for multiple C categories, link from each.

## How NOT to use this registry

- **Don't blindly copy code.** The reference crate's implementation may be wrong for your target's byte layout, licensing, or error model. Read the patterns, write your own.
- **Don't treat the reference as infallible.** zip-rs had its own bugs fixed over years. Your port may encounter edge cases the reference handles incorrectly. When there's a disagreement between the reference and the C original, the **C original wins** on byte layout and the **C original wins** on ABI compatibility.
- **Don't pull the reference in as a runtime dep.** Keep it as a dev-dep at most (for fixture generation). The point is to learn the type architecture, not inherit the crate's dependency tree.
