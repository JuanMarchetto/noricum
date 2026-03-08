---
name: migration-validator
description: Safety verification workflow and checklists for migrated Rust code. Use when validating that a C-to-Rust migration is correct and safe.
---

# Migration Validation Checklist

## Compilation Check
- [ ] Rust code compiles with `rustc --edition 2024`
- [ ] No errors, only warnings acceptable if minor
- [ ] Clippy passes with 0 warnings (`cargo clippy`)

## Safety Check
- [ ] Count `unsafe` blocks (target: 0 for easy/medium, minimize for hard)
- [ ] Every `unsafe` block has a `// SAFETY:` comment
- [ ] No raw pointer dereferences without bounds checking
- [ ] No transmute unless absolutely necessary
- [ ] No `unwrap()` on user-controlled data

## Semantic Equivalence
- [ ] Same function signature (equivalent types)
- [ ] Same behavior for all valid inputs
- [ ] Same error conditions / edge cases
- [ ] Overflow behavior matches (wrapping vs. checked)
- [ ] Printf format matches exactly (especially `%g` → custom format_g())
- [ ] Bool/int distinction preserved (C `int` 0/1 ≠ Rust `bool`)

## Differential Testing
- [ ] Compile C with `cc -std=gnu11 -lm`
- [ ] Compile Rust with `rustc --edition 2024`
- [ ] Run both with identical test inputs
- [ ] Compare outputs **byte-by-byte** (not just "looks similar")
- [ ] Test edge cases: empty input, max values, null/None, negative numbers
- [ ] Float tolerance: exact match preferred, tolerance only for known precision differences

## Idiomatic Score
Enhanced formula with positive AND negative signals:
- Base: 100
- Negative: `- (unsafe_blocks * 10) - (clippy_warnings * 2) - (unwrap_count * 3) - (as_casts * 1)`
- Positive: `+ min(10, result_count * 2) + min(5, option_count) + min(5, iter_usage)`
- Ranges: 90-100 Excellent, 70-89 Good, 60-69 Acceptable, <60 Needs repair

## Quality Floor (P0)
- Track baseline unsafe count from initial translation
- **NEVER** allow repair to increase unsafe above baseline
- If repair adds unsafe → reject that iteration, try again
- This prevents the "oscillation" problem where repair trades quality for compilation

## Best-Version Tracking (P1)
- Keep the version with highest score throughout repair loop
- If nothing passes validation, use best version as fallback (not raw c2rust)
- Prefer: compiling version > high-score non-compiling > c2rust fallback

## Repair Loop Rules
- Effective iterations scale with file size:
  - <1000 LOC: full configured max (default 5)
  - 1000-2000 LOC: min(configured, 3)
  - >2000 LOC: min(configured, 2)
  - Chunked translations: max(configured, 8) — chunks are small, repair is affordable
- Stall detection: 2 unchanged error counts → re-translate at temp 0.7
- Temperature ramp: base 0.2 + 0.1 per iteration (capped at 1.0)
- Quality gate: if initial translation has >5 unsafe → re-translate at temp 0.5

## Validated Production Migrations
These fixtures are regression-tested and represent known-good migrations:
| File | LOC C→Rust | Score | Unsafe | Notes |
|------|-----------|-------|--------|-------|
| add, power, gcd, etc. | <50 | 100 | 0 | Simple arithmetic |
| hash_table.c | 204→~200 | 81 | 0 | malloc/free → Box/Vec |
| cjson_combined.c | 520→319 | 100 | 0 | JSON parser, recursive data |
| expr_eval.c | 1686→1446 | 100 | 0 | 74 functions, recursive descent |
| cjson_full_combined.c | 1696→1439 | 100* | 0 | Full cJSON library (49/78 fns) |
| picohttpparser.c | 1102→1551 | 100 | 0 | HTTP parser, state machine |
| miniz_core_standalone.c | 4429 | N/A | N/A | FallbackUnsafe (ceiling case) |
