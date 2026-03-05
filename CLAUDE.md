# Noricum - Autonomous C/C++ to Rust Migration Agent

## Project Overview
Noricum is an autonomous agent for migrating C/C++ codebases to safe, idiomatic Rust.
It combines a deterministic pipeline (C2Rust as step zero) with LLM agents (via rig-rs)
and differential testing for verification.

## Architecture
- **Agent orchestrator**, not a compiler. C2Rust handles mechanical translation.
- LLM agents are central from v0 (not deferred).
- Semantic Code Map (noricum-ir) tracks migration state metadata, not compiler IR.
- rig-rs for LLM integration. Primary: Claude API. Fallback: Ollama (local).

## Crate Structure
| Crate | Type | Purpose |
|-------|------|---------|
| noricum-cli | Binary | clap CLI entry point |
| noricum-core | Library | Orchestrator, state machine, model router |
| noricum-ir | Library | Semantic Code Map (migration metadata) |
| noricum-agents | Library | LLM agents via rig-rs |
| noricum-tools | Library | Tool implementations (c2rust, compile, test, clippy) |
| noricum-validation | Library | Verification pipeline (diff tests, scoring) |
| noricum-mcp | Library | MCP server (future) |

## Coding Conventions
- Rust edition 2024
- Use `thiserror` for library errors, `anyhow` for CLI/binary errors
- Use `tracing` for logging (not `log` or `println!`)
- All public APIs must have doc comments
- Async with tokio runtime
- Tests go in `#[cfg(test)] mod tests` within each file, plus integration tests in `tests/`
- No `unwrap()` in library code; use proper error propagation with `?`
- Prefer returning `Result` over panicking

## Error Handling Pattern
```rust
// In library crates, define typed errors:
#[derive(Debug, thiserror::Error)]
pub enum MyError {
    #[error("description: {0}")]
    Variant(String),
}

// In CLI, use anyhow for top-level:
fn main() -> anyhow::Result<()> { ... }
```

## State Machine States
```
Pending -> Extracted -> Characterized -> C2RustDone -> Analyzed -> Refined -> Validated
                                                                     |
                                                                     v
                                                              Repairing (max 5 iterations)
                                                                     |
                                                                     v
                                                              FallbackUnsafe
```

## Key Commands
```bash
cargo check                          # Verify workspace compiles
cargo test                           # Run all tests
cargo run -p noricum-cli -- migrate <file>  # Run migration
cargo clippy --workspace             # Lint check
```

## Testing Strategy
- Unit tests in each crate
- Integration tests in `tests/`
- Test fixtures in `tests/fixtures/simple/` (small C files)
- Differential testing: compile both C and Rust, compare outputs

## Git Conventions
- Branch from `main`
- Conventional commit messages (feat:, fix:, refactor:, test:, docs:, chore:)
- No co-author attribution for AI in commits
