# Changelog

All notable changes to Noricum will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
