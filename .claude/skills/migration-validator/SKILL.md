---
name: migration-validator
description: Safety verification workflow and checklists for migrated Rust code. Use when validating that a C-to-Rust migration is correct and safe.
---

# Migration Validation Checklist

## Compilation Check
- [ ] Rust code compiles with `rustc --edition 2024`
- [ ] No errors, only warnings acceptable if minor

## Safety Check
- [ ] Count `unsafe` blocks (target: 0 for easy/medium functions)
- [ ] Every `unsafe` block has a `// SAFETY:` comment explaining why
- [ ] No raw pointer dereferences without bounds checking
- [ ] No transmute unless absolutely necessary

## Semantic Equivalence
- [ ] Same function signature (equivalent types)
- [ ] Same behavior for all valid inputs
- [ ] Same error conditions / edge cases
- [ ] Overflow behavior matches (wrapping vs. checked)

## Differential Testing
- [ ] Compile C with `cc -std=c11`
- [ ] Compile Rust with `rustc --edition 2024`
- [ ] Run both with identical test inputs
- [ ] Compare outputs byte-by-byte
- [ ] Test edge cases: empty input, max values, null/None

## Idiomatic Score
Formula: `100 - (unsafe_blocks * 10) - (clippy_warnings * 2)`
- 90-100: Excellent
- 70-89: Good
- 60-69: Acceptable (minimum threshold)
- Below 60: Needs repair iteration

## Clippy Check
- [ ] Run `cargo clippy` on generated code
- [ ] Address all warnings if practical
- [ ] Document any intentional clippy allowances

## Repair Loop Rules
- Max 5 iterations
- Each iteration: fix compiler errors -> revalidate
- If still failing after 5: fallback to unsafe (FallbackUnsafe state)
- Log all repair attempts for learning
