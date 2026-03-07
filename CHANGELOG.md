# Changelog

All notable changes to Noricum will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- expr_eval.c migration — 1686 LOC expression evaluator, score 100/100, 0 unsafe, 0 repairs, diff test PASS (largest file migrated: 3.2x previous record)
- Interface-aware CRUST-Bench with progress logging (`[N/M]` counter)
- Error severity classification in compiler diagnostics
- `--ollama-model` CLI flag for Ollama provider forcing
- `compare` subcommand for side-by-side migration comparison
- CI release workflow for automated GitHub releases
- Fuzz targets for core parsing functions
- 5th RAG seed pattern
- Behavioral review infrastructure and launch readiness assessment system
- Dynamic max_tokens scaling for translation/repair agents based on input size (fixes truncation on large files)
- cJSON library migration — 520 LOC, score 100/100, 0 unsafe, diff test PASS (basic + 12 extended edge-case tests)
- Context window mitigations for large file migrations (abbreviate_c_source, build_structural_summary, effective_repair_iterations)
- Multi-pass chunked translation for files >2000 LOC

### Fixed
- C compiler flags: `-std=c11` → `-std=gnu11` (fixes `strdup` implicit declaration causing segfaults)
- C linker flags: added `-lm` for math.h functions (sqrt, pow, fabs, floor, ceil, round)
- Combined single-file C fixtures for standalone diff testing
- Extended behavioral test harness with 12 edge cases (nested objects, arrays, escaping, unicode, nulls)
- RAG seed pattern `cjson_to_serde` updated to hand-rolled idioms (no external crate dependency)
- RAG pattern `cjson_to_serde.md` headers (`Rust Pattern`/`Rust Example` → `Rust Equivalent`) — was never loading into PatternStore
- Remove dead `interactive.rs` module from noricum-cli
- Ollama provider stubs now return proper `Result` error instead of silent no-op
- MCP `diff_test` tool: add input size validation (was missing unlike other tools)

### Changed
- Improved token estimation heuristic in orchestrator

## [0.1.0] - 2026-03-06

### Added
- Full 9-stage migration pipeline (Pending -> Validated)
- LLM-powered analysis, translation, repair, and test generation agents via rig-rs
- Rule-based C-to-Rust translation fallback for simple functions
- Differential testing with byte-exact output comparison
- Enhanced idiomatic scoring (0-100) with positive/negative signal detection
- Repair loop with up to 5 iterations and diff test feedback
- RAG pattern store with 4 seed patterns
- MCP server with 6 tools for IDE integration
- REST API server with 6 endpoints
- HTML migration reports (single-file and project-level)
- Dependency-aware multi-file migration with topological ordering
- CLI with migrate, analyze, doctor, bench, and serve commands
- Docker multi-stage build
- CI pipeline (check, test, clippy, fmt, audit, benchmark)
- Token usage estimation and temperature configurability
- Multi-stakeholder automated review system
- Economic evaluation system

### Security
- Path validation for LLM tool calls
- API key authentication for REST endpoints
- CORS restrictions (configurable allowlist)
- Input size limits (10 MB)
- Non-root Docker user
- cargo-audit in CI
