# cjson_combined

520-LOC hand-combined cJSON (parser, printer, object/array builders) used as the first mid-size JSON fixture. Recursive data structures, manual memory, and string escaping all map onto an enum + `Vec` + RAII model in the Rust output.

- **C source:** `tests/fixtures/cjson/cjson_combined.c`
- **C LOC:** 520
- **Rust LOC:** 354
- **Idiomatic score:** 100/100
- **Unsafe blocks:** 0
- **Tests:** 1 basic + 12 extended tests byte-exact (see `golden_cjson_combined` and `golden_cjson_combined_extended` in `tests/golden_outputs.rs`)
- **Repair iterations:** 3
- **Provenance:** `.noricum-artifacts/cjson_combined-20260318-160932/06-final.rs` (`final_state: Validated`)
- **Archived on:** 2026-04-11

Note: not to be confused with the flagship `cjson_full` migration (1696-LOC full DaveGamble/cJSON), which lives in `best_outputs/cjson_full/`.
