# http_parser

nodejs/http-parser (6.4k stars): a 3680-LOC goto-heavy 58-state HTTP request/response parser with callback API and 37 conformance tests. Migrated to a clean line-based parser built around `find_crlf()`, with C global mutable state translated to `thread_local! { RefCell<TestState> }`, C function pointers inlined into direct calls, and the original bit-packed struct flattened into idiomatic Rust fields while preserving the callback API via the global-state pattern.

- **C source:** `tests/fixtures/http_parser/http_parser_combined.c`
- **C LOC:** 3680
- **Rust LOC:** 1492 (ratio 0.41x)
- **Idiomatic score:** high; MEMORY.md records 0 unsafe, 0 `unwrap()`
- **Unsafe blocks:** 0
- **Tests:** 37 conformance tests pass; diff test byte-exact
- **Provenance:** `tests/fixtures/http_parser/http_parser_combined.rs`. The autonomous pipeline proved convergence (compile + pass at iteration 7 on Run 1) but lost the run to a transient 502; the final file was completed manually from the pipeline's partial output. See MEMORY.md for the full story.
- **Archived on:** 2026-04-11
