---
name: noricum-dev
description: Noricum project conventions, architecture decisions, and coding standards. Use when working on Noricum crate code.
---

# Noricum Development Conventions

## Architecture
- Noricum is an **agent orchestrator**, not a compiler
- C2Rust is "step zero" (subprocess), not reinvented — but often skippable (P4: `--skip-c2rust`)
- LLM agents are central from v0
- Semantic Code Map (noricum-ir) tracks metadata, not compiler IR
- rig-rs 0.31 for LLM integration (Rust-native, rustls)

## Crate Dependencies (layered)
```
noricum-cli -> noricum-core -> noricum-agents -> noricum-tools -> noricum-ir
                            -> noricum-validation -> noricum-tools -> noricum-ir
noricum-mcp -> noricum-core
```

## Coding Standards
- Edition 2024, `thiserror` for lib errors, `anyhow` for CLI
- `tracing` for logging, never `println!` in library code
- No `unwrap()` in library code
- Async with tokio
- Tests in `#[cfg(test)] mod tests` within each file
- C compiler: `-std=gnu11` (not c11, needed for POSIX like strdup), `-lm` for math

## State Machine
```
Pending -> Extracted -> Characterized -> C2RustDone -> Analyzed -> Refined -> Validated
                                                                    |
                                                            Repairing (max iterations)
                                                                    |
                                                            FallbackUnsafe
```

## Model Router (Claude 4.6 era)
- Easy: `claude-haiku-4-5` (fast, cheap)
- Medium: `claude-sonnet-4-6` (also used for analysis)
- Hard: `claude-opus-4-6` (translation, complex repair)
- Fallback: Ollama `qwen2.5-coder:32b` when no API key

## Pipeline Improvements (P0-P5, learned from miniz + genann migrations)
- **P0: Quality floor** — repair rejected if unsafe count exceeds translation baseline
- **P1: Best-version tracking** — keeps highest-score version, uses it for fallback instead of c2rust
- **P2: Per-function C2Rust context** — extracts only matching c2rust functions per chunk
- **P3: Incremental per-module** — `split_into_modules()` groups C functions by prefix
- **P4: Skip C2Rust** — `--skip-c2rust` flag; LLM often translates better without c2rust noise
- **P5: Idiomatic improvement hints** — when code compiles + diff passes but score < threshold, generates actionable refactoring hints (reduce `as` casts, use iterators) so repair agent improves style instead of returning unchanged

## Key Thresholds
- Chunked translation: >800 LOC (MEDIUM_FILE_LOC)
- Reduced repair iterations: >1000 LOC
- Very large file handling: >2000 LOC
- Structural summary: >800 LOC
- Quality gate: re-translate if >5 unsafe blocks
- Stall detection: 2 consecutive unchanged error counts → re-translate at temp 0.7
- Chunk targets: 400 LOC (medium), 500 LOC (very large)

## Key Patterns
- Functions migrate independently, ordered by dependency graph (topological sort)
- Every migration must pass differential testing (byte-exact stdout match)
- Translation cache: `.noricum-cache/` keyed by SHA-256 of C source
- RAG patterns: successful migrations auto-added to PatternStore for future context
- Structural chunking: data model (structs/constructors) in chunk 0, logic functions in later chunks
