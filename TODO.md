# Noricum — Pending Work

Tracked items for near-term implementation. Ordered by priority within each category.

---

## Context Window Mitigations (remaining from P3)

### P3.7 — Multi-pass Translation for Very Large Files

**Problem:** Files >2000 LOC risk exceeding the LLM context window when sent as a single prompt. The current structural summary (P2.6) helps but doesn't solve the fundamental input size limit.

**Design:**
1. Parse C source and split into chunks of ~500 LOC by function boundaries
2. Translate shared declarations first (structs, typedefs, globals, enums)
3. Translate function chunks in dependency order, injecting already-translated signatures as context
4. Recombine chunks into a single `.rs` file with deduplicated imports
5. Run validation on the combined output

**Files to create/modify:**
- `crates/noricum-tools/src/c_chunker.rs` — Lightweight C function boundary parser
- `crates/noricum-agents/src/translation.rs` — New `translate_chunked()` function
- `crates/noricum-core/src/orchestrator.rs` — Route to chunked translation when LOC > 2000

**Estimated effort:** ~200-300 LOC new code

### P3.8 — Streaming LLM Responses

**Problem:** For large files, LLM responses can take 30-60 seconds. Streaming would enable progress feedback and early error detection (e.g., truncation).

**Action required:**
- Investigate `rig-core 0.31` streaming API support
- If supported: add `run_prompt_streaming()` to `LlmClient` enum
- If not supported: open issue upstream or use raw `reqwest` streaming as fallback
- Wire streaming into translation and repair agents for files >1000 LOC

**Files to modify:**
- `crates/noricum-agents/src/providers.rs` — Add streaming variant to `LlmClient`
- `crates/noricum-agents/src/translation.rs` — Use streaming for large files
- `crates/noricum-agents/src/repair.rs` — Use streaming for large files

---

## Test Coverage Gaps

### Unit tests for LLM agents

**repair.rs** and **translation.rs** in `noricum-agents` have zero unit tests. They are partially covered by integration tests (`mock_llm_pipeline.rs`), but lack:
- Tests for `abbreviate_c_source()` edge cases (repair.rs)
- Tests for `build_structural_summary()` output (translation.rs)
- Tests for dynamic `max_tokens` calculation in both agents
- Tests for temperature escalation logic in repair

**Files:** `crates/noricum-agents/src/repair.rs`, `crates/noricum-agents/src/translation.rs`

### expr_eval.c fixture not in any test

The 1686 LOC fixture exists at `tests/fixtures/large/expr_eval.c` but no integration test exercises it. Add a golden output test or at least a compilation check.

**File:** `tests/golden_outputs.rs` or `tests/integration_test.rs`

---

## Documentation & Metadata

### Cargo.toml metadata for crates.io

All 7 crates are missing `description`, `categories`, and `keywords` fields. Required before any crates.io publish.

**Files:** All `crates/*/Cargo.toml`

### CHANGELOG.md cleanup

- Duplicate `### Fixed` section in Unreleased
- Missing entry for context window mitigations (PR #7)

**File:** `CHANGELOG.md`

### Module-level doc comments

Several modules lack `//!` module-level documentation:
- `noricum-mcp/src/lib.rs`
- `noricum-agents/src/repair.rs`, `translation.rs` (have function-level but no module-level)
- Various tool modules in `noricum-tools/src/`

---

## Code Quality

### Large file refactoring

Files over 500 LOC that could benefit from splitting:
- `crates/noricum-tools/src/rule_translate.rs` (1203 lines) — Split by translation pattern category
- `crates/noricum-cli/src/main.rs` (915 lines) — Extract subcommand handlers into modules

### Wire incremental migration to CLI

Infrastructure exists in `noricum-core/src/incremental.rs` (100+ LOC, full state tracking) but is not exposed via CLI. Add `--incremental-state <dir>` flag.

**Files:** `crates/noricum-cli/src/main.rs`, `crates/noricum-core/src/orchestrator.rs`

---

## CI Improvements

### MSRV testing

CI doesn't test minimum supported Rust version. Add a matrix entry for the oldest supported toolchain.

**File:** `.github/workflows/ci.yml`

### Cross-platform testing

Currently only runs on Ubuntu. Consider adding macOS to the matrix.

**File:** `.github/workflows/ci.yml`
