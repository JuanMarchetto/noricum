# expr_eval

Expression evaluator with a recursive-descent parser, lexer, 25+ builtins, variable store, and a small `DataSet` stats helper. Largest single-file migration produced by the pipeline to date at byte-exact quality.

- **C source:** `tests/fixtures/large/expr_eval.c`
- **C LOC:** 1686
- **Rust LOC:** 1446
- **Idiomatic score:** 100/100
- **Unsafe blocks:** 0
- **Tests:** golden test asserts key output sections (`golden_expr_eval_fixture` in `tests/golden_outputs.rs`); full diff test was byte-exact at migration time
- **Repair iterations:** 0
- **Provenance:** `output/large/expr_eval.rs` (best single-file migration output saved from the pipeline)
- **Archived on:** 2026-04-11
