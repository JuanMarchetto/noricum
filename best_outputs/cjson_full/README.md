# cjson_full

Flagship migration of the full DaveGamble/cJSON library (12.5k stars on GitHub). Linked list to `Vec`, `malloc`/`free` to RAII, type tags to enum, UTF-16 surrogate pair handling, and a C-compatible `format_g()`. Phases A-F complete: convenience add, references (via `Clone`), case-sensitive ops, float/string arrays, parse variants, print variants, setters, version. Phase G (cJSON_Utils: JSON Pointer/Patch) deferred.

- **C source:** `tests/fixtures/cjson/cjson_full_combined.c`
- **C LOC:** 1696
- **Rust LOC:** 1439 (ratio 0.85x)
- **Idiomatic score:** 100/100
- **Unsafe blocks:** 0 (also 0 raw pointers)
- **Tests:** 97/97 C tests + 96/96 Rust tests pass; diff test byte-exact
- **Archived on:** 2026-04-11

Files in this directory:
- `cjson_full_migrated.rs` — the full 1439-LOC migration (canonical)
- `cjson_combined_520loc.rs` — an earlier simpler variant of the 520-LOC combined subset (preserved for historical reference; the canonical 520-LOC migration lives in `best_outputs/cjson_combined/`)
