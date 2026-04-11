# olive

tsoding/olive.c 2D software graphics library: pixels, lines, triangles, circles, rectangles, text rendering from a 350-LOC embedded glyph atlas, and subcanvas/region operations. Raw pointer canvas became an owned `Canvas { pixels: Vec<u32> }`, macros like `OLIVEC_PIXEL` became `pixel()`/`set_pixel()` methods, subcanvas shared-mutable aliasing became region-based direct operations, type punning `*(uint32_t*)&z` became `f32::to_bits()`, and fallible helpers (barycentric coords, `normalize_rect`/`normalize_triangle`) return `Option`.

- **C source:** `tests/fixtures/olive/olive_combined.c`
- **C LOC:** 1443 (header + impl combined; "library" itself is ~1022 LOC)
- **Rust LOC:** 1176 (ratio 0.81x)
- **Idiomatic score:** high (see golden test); 0 unsafe per MEMORY.md
- **Unsafe blocks:** 0 (0 `unwrap()` too)
- **Tests:** 23 test functions covering all 24 library functions; golden test `golden_olive` in `tests/golden_outputs.rs` passes byte-exact via pixel-buffer checksums
- **Provenance:** `tests/fixtures/olive/olive_migrated.rs` (canonical; the `.noricum-artifacts/olive_combined-*` run ended in `FallbackUnsafe` and is NOT the file archived here)
- **Archived on:** 2026-04-11
