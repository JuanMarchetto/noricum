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

## Interactive Spike Mode (alternative to pipeline, validated on miniz_zip.c 2026-04-11)

**When to use:** C file where the autonomous pipeline has failed 3+ times, OR target is >2500 LOC with multi-layer wrappers/legacy API baggage. For <1000 LOC leaf functions, pipeline still wins on cost per attempt.

**What it is:** Claude Code as the interactive director, human as the operator, Noricum primitives available as MCP tools but not as a hardcoded sequence. Writes Rust bottom-up function-by-function with an oracle harness for immediate diff validation.

**Phase 0 is sacred** (pre-clock, ~2-3 hours, not counted in 8-hour spike budget):
1. `cargo new` outside the cargo workspace (or with explicit `[workspace]` in Cargo.toml to break inheritance)
2. Copy C sources + headers into the spike dir
3. `build.rs` with `cc-rs` compiling C into a static lib
4. Thin C wrapper (`wrapper.c` + `wrapper.h`) exposing opaque handles over the target functions
5. Pure-C smoke test compiled with `gcc` — must return 0 before any Rust is written
6. Deterministic fixture generator (Python `zipfile` or equivalent), script + output both committed
7. Rust-side differential test that extracts via the C wrapper and snapshots `(name, size, crc32, first_bytes)`
8. Phase 0 is green when `cargo test -- oracle_selftest` passes. Hour 0 starts here.

**Architectural seed strategy** (critical for avoiding C's accidental complexity):
- Before writing Rust, clone an existing idiomatic Rust crate for the problem domain to a tempdir
- Read its `src/types.rs` and `src/spec.rs` for 30 minutes
- Steal the type architecture wholesale (especially trait bound patterns — this is where the pipeline fails with `Box<dyn Read + Write + Seek>`)
- C source stays as byte-level ABI ground truth, NOT as architectural template

Validated seeds:
- ZIP containers → `zip-rs/zip2` (github.com/zip-rs/zip2)
- HTTP parsers → `hyper`
- SQLite adapters → `rusqlite`

**Algorithm delegation:** don't translate well-solved subsystems. Delegate to existing crates:
- Deflate/inflate → `flate2` (saves ~2200 LOC on miniz_zip alone)
- CRC32 → `crc32fast`
- AES → `aes` + `ctr` from RustCrypto
- SHA → `sha1` / `sha2`

The differential test asserts on extracted content + metadata, NOT on raw archive bytes. This scopes the migration to "structure and ABI" while keeping the algorithm work in battle-tested crates.

**Bottom-up function-by-function discipline:**
- Each function commits only after its diff test passes
- No batch commits, no "implement five then test"
- Failed function after 3 repairs: mark blocked, move on
- Hour-3 gate: if the simplest non-empty fixture fails, abort and reassess
- Hour-8 hard exit regardless of state

**Reference commit history:** `feat/interactive-spike` branch of this repo. 5 commits from `cf4a07f` (Phase 0 oracle) through `454e560` (writer path complete). miniz_zip.c → 1157 LOC Rust, 28 tests, 0 unsafe, 19 min elapsed.

## MCP Tool Limitations (learned 2026-04-11)

**DO NOT use `mcp__noricum__migrate_function` as a drop-in translator.** When called via the MCP server, it only runs the first stage of the pipeline state machine (`Pending → Extracted`) and returns `rust_source: ""` with `state: "Extracted"`. The full translation pipeline (Characterized → C2RustDone → Analyzed → Refined → Validated) does not execute from the MCP interface — only from the CLI or orchestrator.

**Workaround:** Interactive agents doing C-to-Rust work should write Rust directly using their own reasoning + architectural references (zip-rs, hyper, etc.), and use `mcp__noricum__check_compilation` and `mcp__noricum__diff_test` MCP tools only for verification. The "translate via MCP" pattern the design doc assumed does not work as-is.

**Unverified:** whether other pipeline stages (analyze_function, behavioral_review, etc.) have the same limitation or actually produce output via MCP. Check before relying on any of them.
