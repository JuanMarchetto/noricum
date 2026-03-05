---
name: noricum-dev
description: Noricum project conventions, architecture decisions, and coding standards. Use when working on Noricum crate code.
---

# Noricum Development Conventions

## Architecture
- Noricum is an **agent orchestrator**, not a compiler
- C2Rust is "step zero" (subprocess), not reinvented
- LLM agents are central from v0
- Semantic Code Map (noricum-ir) tracks metadata, not compiler IR
- rig-rs for LLM integration (Rust-native)

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

## State Machine
Pending -> Extracted -> Characterized -> C2RustDone -> Analyzed -> Refined -> Validated
With repair loop (max 5) and FallbackUnsafe

## Model Router
- Easy: Sonnet (or Ollama for trivial)
- Medium: Sonnet
- Hard: Opus

## Key Patterns
- Functions migrate independently, ordered by dependency graph
- Every migration must pass differential testing
- Idiomatic score: 100 - (unsafe * 10) - (clippy_warnings * 2)
