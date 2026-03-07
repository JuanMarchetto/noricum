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
cargo clippy --workspace -- -D warnings
```

### Prerequisites

- **Rust 1.86+** (edition 2024): install via [rustup](https://rustup.rs/)
- **GCC** (for compiling C test fixtures): `apt install gcc` / `brew install gcc`
- **tree-sitter** is vendored — no system install needed

### LLM-powered features

Create a `.env` file in the project root (it is gitignored):
```bash
echo "ANTHROPIC_API_KEY=sk-ant-..." > .env
```

Then export before running:
```bash
export $(cat .env | xargs)
cargo run -p noricum-cli -- migrate tests/fixtures/simple/add.c
```

Without an API key, use `--no-llm` to run the rule-based pipeline:
```bash
cargo run -p noricum-cli -- migrate tests/fixtures/simple/add.c --no-llm
```

### Useful commands

```bash
cargo test --workspace              # Run all tests
cargo clippy --workspace -- -D warnings  # Lint
cargo fmt --all -- --check          # Check formatting
cargo doc --no-deps --workspace     # Build docs
cargo run -p noricum-cli -- doctor  # Check tool availability
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
