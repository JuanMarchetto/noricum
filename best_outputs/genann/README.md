# genann

First autonomous "wow" migration: codeplea/genann (2,246 stars), a tiny feed-forward neural network library. Function pointers became an `ActivationFn` enum dispatch, a single-malloc struct split into separate `Vec<f64>` fields, a global lookup moved into a struct field, glibc's TYPE_3 degree-31 RNG was reimplemented in Rust, and `FILE` I/O became `std::io::{Read, Write}` trait objects.

- **C source:** `tests/fixtures/genann/genann_combined.c`
- **C LOC:** 642
- **Rust LOC:** 623 in this archive (MEMORY.md records the migration-time size as 721 LOC — the archived file is the currently-canonical copy shared with `tests/fixtures/genann/genann_migrated.rs`)
- **Idiomatic score:** 47/100 (lower due to `as` casts for numeric conversions)
- **Unsafe blocks:** 0 (0 `unwrap()` too)
- **Tests:** diff test byte-exact on 521,556 assertions; golden test `golden_genann` in `tests/golden_outputs.rs`
- **Repair iterations:** 1
- **Archived on:** 2026-04-11
