# miniz_test

Adler-32 and CRC-32 checksum routines extracted from miniz, plus a small driver that exercises both one-shot and incremental variants.

- **C source:** `tests/fixtures/miniz/miniz_test.c`
- **C LOC:** 153
- **Rust LOC:** 154
- **Idiomatic score:** 84/100
- **Unsafe blocks:** 0
- **Tests:** golden diff test passes byte-exact (`golden_miniz_test` in `tests/golden_outputs.rs`)
- **Provenance:** `output/miniz/miniz_test.rs` (best single-file migration output saved from the pipeline)
- **Archived on:** 2026-04-11
