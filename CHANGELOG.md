# Changelog

All notable changes to Noricum will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- cJSON library migration — 520 LOC, score 100/100, 0 unsafe, diff test PASS (basic + 12 extended edge-case tests)
- Combined single-file C fixtures for standalone diff testing
- Extended behavioral test harness with 12 edge cases (nested objects, arrays, escaping, unicode, nulls)
- RAG seed pattern `cjson_to_serde` updated to hand-rolled idioms (no external crate dependency)

### Fixed
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
