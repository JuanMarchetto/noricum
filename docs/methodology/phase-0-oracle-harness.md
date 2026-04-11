# Phase 0: Oracle Harness Recipe

Pre-clock setup for an interactive spike. Builds the C oracle infrastructure so the Rust side has a truth source before any translation begins. Expected duration: 2-3 hours of focused work. Does NOT count against the 8-hour spike budget.

## What exists when Phase 0 is green

- A new cargo package outside the main workspace (or with its own `[workspace]` table)
- C sources + headers copied from the target library
- `build.rs` compiling the C sources into a static lib via `cc-rs`
- `wrapper.h` + `wrapper.c` — opaque-handle C wrapper around the target API
- `wrapper_smoke.c` — pure-C smoke test that opens a sanity fixture and exits 0
- `fixtures/gen_fixtures.py` (or equivalent) — deterministic fixture generator
- `fixtures/{format}_corpus/` — committed fixtures
- `tests/differential_{format}.rs` — Rust-side harness calling the C wrapper via `extern "C"` and snapshotting each entry
- `cargo test -- oracle_selftest` is GREEN

When all of that is true, **Hour 0 starts**.

## Step by step

### 1. Branch and directory

```sh
git checkout -b feat/interactive-spike
mkdir noricum-spike-{name} && cd noricum-spike-{name}
cargo init --name spike
```

Inside the new `Cargo.toml`, add `[workspace]` as a top-level key to break inheritance from any parent workspace:

```toml
[workspace]

[package]
name = "noricum-spike-{name}"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
name = "spike"
path = "src/lib.rs"

[build-dependencies]
cc = "1"

[dependencies]
# Runtime deps come later. See dependency-budget.md for modes.
```

### 2. Copy C sources

```sh
cp {path-to-target}/*.c {path-to-target}/*.h .
```

Include every `.c` and `.h` that the target depends on. For miniz_zip this was 4 `.c` files + 5 `.h` files. Check `#include` directives in the main file to find required headers.

### 3. Write `build.rs`

```rust
fn main() {
    for f in ["source1.c", "source1.h", "wrapper.c", "wrapper.h", /* ... */] {
        println!("cargo:rerun-if-changed={f}");
    }

    cc::Build::new()
        .file("source1.c")
        .file("source2.c")
        .file("wrapper.c")
        .include(".")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-function")
        .compile("{name}_wrapper");
}
```

### 4. Write `wrapper.h` + `wrapper.c`

Use opaque handles. The Rust side should not know the layout of the C library's internal types. Example for a reader/writer format:

```c
/* wrapper.h */
typedef void* {name}_handle_t;
{name}_handle_t wr_reader_open(const char* filename);
int             wr_reader_get_num_files({name}_handle_t h);
int             wr_reader_extract({name}_handle_t h, int idx, void* buf, size_t cap);
int             wr_reader_stat({name}_handle_t h, int idx, char* name_out, size_t name_cap, size_t* size_out, unsigned int* crc32_out);
void            wr_reader_close({name}_handle_t h);
```

Implement each function in `wrapper.c` as a thin delegation to the C library, allocating any internal state on the heap and returning an opaque pointer.

### 5. Write `wrapper_smoke.c`

A pure-C `main()` that opens one sanity fixture (e.g., `hello.{ext}`), extracts a known entry, verifies `strcmp == 0` on the content, returns 0 on success or non-zero on any failure. This is your first green light — if this returns 0, the C wrapper is correctly linked and the target library's core API works through it.

Compile and run outside of cargo:
```sh
gcc -o wrapper_smoke wrapper_smoke.c wrapper.c {c_sources}.c -I.
./wrapper_smoke
```

**Must exit 0 before Rust is touched.** This catches wrapper bugs before Rust confuses the picture.

### 6. Fixture generator

Write a `fixtures/gen_fixtures.py` (or Bash, or whatever) that DETERMINISTICALLY produces a small corpus of fixtures covering edge cases. Use fixed timestamps, fixed RNG seeds. Commit both the script AND the generated files.

The corpus should cover: empty, single-entry, multiple-entries, large payload, edge cases of the format (unicode, comments, long filenames, nested paths, zip64 or equivalent), format-specific compression variants.

### 7. Rust FFI layer

In `src/lib.rs`, declare the extern block:

```rust
use std::os::raw::{c_char, c_int, c_uint, c_void};

pub type SpikeHandle = *mut c_void;

#[link(name = "{name}_wrapper", kind = "static")]
unsafe extern "C" {
    pub fn wr_reader_open(filename: *const c_char) -> SpikeHandle;
    pub fn wr_reader_get_num_files(h: SpikeHandle) -> c_int;
    /* ... */
}
```

This is the ONLY unsafe in the spike (outside of unsafe third-party crates). Everything else in `src/` is safe Rust.

### 8. Differential test harness

In `tests/differential_{format}.rs`, write a Rust test that:
1. Walks every fixture in the corpus directory
2. Opens each via the C wrapper (the oracle)
3. Extracts every entry
4. Snapshots `(name, size, crc32, first_bytes)` to a Vec
5. Asserts on well-formedness (non-empty, consistent sizes)

The critical test is `oracle_selftest` — sanity check that the oracle works. When this passes, Phase 0 is green.

```rust
#[test]
fn oracle_selftest() {
    let fixtures = list_fixtures();
    assert!(!fixtures.is_empty());
    for fx in &fixtures {
        let snap = snapshot_via_oracle(fx).expect("oracle");
        /* assertions */
    }
}
```

### 9. Run it

```sh
cargo test --test differential_{format} -- oracle_selftest
```

Must be green. If it fails, debug the C wrapper first (go back to step 5 smoke test), then the `build.rs`, then the extern block. Do NOT touch Rust translation until Phase 0 is green.

## Gotchas encountered in real spikes

- **miniz_export.h** — miniz is split across multiple headers and wants a generated `miniz_export.h` for its `MINIZ_EXPORT` macro. Copy it from the source tree explicitly.
- **Multi-file C libraries** — check `#include` recursively. Don't assume "the big .c file has everything".
- **Workspace inheritance** — if your spike lives inside a cargo workspace, add an explicit `[workspace]` table to the spike's `Cargo.toml` or parent's `Cargo.toml` will try to auto-include it.
- **Fixture determinism** — Python's `zipfile` adds timestamps by default. Set `ZipInfo.date_time = (2026, 1, 1, 0, 0, 0)` or equivalent.
- **Compressed binary fixtures in git** — commit both the generator script AND the generated files. Script ensures reproducibility; files ensure offline test runs.

## When Phase 0 takes longer than 3 hours

Finish it anyway. A skimped Phase 0 produces a broken oracle that silently accepts wrong answers. The 8-hour spike budget is protected by deferring the clock start, not by skimping on setup.
