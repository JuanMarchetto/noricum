# Contributing to Noricum

Thank you for your interest in contributing to Noricum!

## Getting Started

1. Fork the repository and clone your fork
2. Install Rust (edition 2024): https://rustup.rs/
3. Build: `cargo build --workspace`
4. Run tests: `cargo test --workspace`
5. Lint: `cargo clippy --workspace -- -D warnings`

## Development Setup

```bash
git clone https://github.com/YOUR_USERNAME/noricum
cd noricum
cargo build --workspace
cargo test --workspace
```

For LLM-powered features, set your API key:
```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

## Coding Conventions

See [CLAUDE.md](CLAUDE.md) for the full list. Key points:

- Rust edition 2024
- `thiserror` for library errors, `anyhow` for CLI
- `tracing` for logging (not `println!`)
- All public APIs need doc comments
- No `.unwrap()` in library code — use `?` or `.expect("reason")`
- Tests in `#[cfg(test)] mod tests` within each file

## Pull Request Process

1. Create a feature branch from `main`
2. Make your changes with conventional commit messages (`feat:`, `fix:`, `test:`, `docs:`, etc.)
3. Ensure all checks pass: `cargo check && cargo test && cargo clippy --workspace -- -D warnings && cargo fmt --all -- --check`
4. Open a PR against `main` with a clear description
5. Wait for CI and review

## Reporting Bugs

Use the [bug report template](.github/ISSUE_TEMPLATE/bug_report.md) and include:
- Steps to reproduce
- Expected vs actual behavior
- Environment info (OS, Rust version)

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
