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

## Pipeline Improvements (P30-P33, learned from miniz_zip.c 9 runs)
- **P30: Hybrid Repair Engine** — 3-phase: rule engine (free) → surgical per-function (cheap) → legacy whole-file (expensive)
- **P31: Assembly Cleanup** — fence stripping, syntax error parsing, use import merging
- **P32: Brace-Balance Validation** — detect/fix truncated module outputs before assembly
- **P32b: Smart Truncate + Re-translate** — truncate at last balanced brace, re-translate truncated modules
- **P33: Type Contract** — types-first modular migration:
  - `generate_type_contract()` in `type_contract.rs` — single LLM call before module translation
  - `ModuleSplit` struct returns `shared_context` from `split_into_modules()`
  - `resolve_header_types()` reads `#include`'d .h files for complete type definitions
  - Assembly seeds P27 dedup from contract type names
  - Key learning: shared_context only has .c file content; .h headers must be resolved separately
- **P34: Graceful Budget Degradation** — adaptive budget `modules*10+25`, soft limits at 80/95/100%
- **P34b: Skip Phase 3 for Assembly** — Phase 3 legacy repair destroys assembled output; skip it

## Type Contract Prompt Rules (learned from Runs 10-12 manual fix)
- Function pointer fields → `Option<fn(args) -> ret>`, NOT `Box<dyn Any>`
- Size/offset/length constants → `pub const NAME: usize`, NOT `u32`
- Use native `bool` everywhere, never define `type MzBool = i32`
- Struct fields use CamelCase types but snake_case field names (Rust convention)
- Every struct must have ALL fields — never empty forward declarations

## Key Patterns
- Functions migrate independently, ordered by dependency graph (topological sort)
- Every migration must pass differential testing (byte-exact stdout match)
- Translation cache: `.noricum-cache/` keyed by SHA-256 of C source
- RAG patterns: successful migrations auto-added to PatternStore for future context
- Structural chunking: data model (structs/constructors) in chunk 0, logic functions in later chunks
- For multi-file C projects: type definitions live in .h headers, not .c files — must resolve includes
- Type contract prompt must enforce idiomatic Rust (no raw pointers, no C-style aliases, complete structs)
