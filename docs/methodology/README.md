# Noricum Methodology Library

Patterns, heuristics, and operational knowledge from Noricum's real-world C-to-Rust migrations. Each doc is a standalone extract — reusable by any interactive-agent workflow, not specific to Noricum itself.

## Contents

- **[Interactive Spike Mode](interactive-spike-mode.md)** — 8-hour human-in-loop + LLM director methodology for frontier migrations (>2500 LOC, 3+ pipeline failures).
- **[Phase 0: Oracle Harness](phase-0-oracle-harness.md)** — pre-clock setup recipe with `cc-rs` + `wrapper.c` + deterministic fixtures + differential test.
- **[Architectural Elimination](architectural-elimination.md)** — the pattern of FREE features via Rust data-flow design choices. Validated on miniz_zip.c (4 eliminations, ~700 LOC of C complexity → 0 LOC Rust).
- **[Dependency Budget Modes](dependency-budget.md)** — "free" / "matching" / "zero" dep policies set in initial spike instructions. Answers "what if we want a drop-in C replacement?".
- **[Architectural Seed Registry](architectural-seeds.md)** — proven C-library → idiomatic Rust crate mappings for stealing type architecture.
- **[MCP Tool Limitations](mcp-tool-limits.md)** — honest disclosure of what works and what doesn't via the MCP interface.

## Source

These docs are distilled from private agent-memory files under `~/.claude/projects/-home-marche-noricum/memory/`. The memory folder is per-machine; **this directory is the public cross-machine form**. New memories from future sessions should be synced here via `tools/sync-memories-to-docs.sh` (or manually, following the same frontmatter + topic split pattern).

## Persistence

The memory folder itself can be backed up with `tools/backup-agent-memory.sh`. That produces a tarball you can store in a private location (private GitHub repo, cloud storage, second machine) to survive hardware failure. The public docs in this directory are git-tracked and survive as long as the repo does.

## Provenance

Initial content derived from the **miniz_zip.c interactive spike** (branch `feat/interactive-spike`, 2026-04-11), which produced a 1157-LOC Rust reader+writer that byte-matched the C oracle on **21,331 real-world entries across 9 archives in 66 minutes** of interactive session time. That spike's commit log is the canonical case study for this methodology.
