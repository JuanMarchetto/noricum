# Noricum

**Autonomous C/C++ to Rust migration agent.**

[![CI](https://github.com/JuanMarchetto/noricum/actions/workflows/ci.yml/badge.svg)](https://github.com/JuanMarchetto/noricum/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![crates.io](https://img.shields.io/crates/v/noricum.svg)](https://crates.io/crates/noricum)

Noricum combines a deterministic pipeline (C2Rust as step zero) with LLM-powered
agents and differential verification to migrate C/C++ code to safe, idiomatic Rust.

## Features

- **C2Rust mechanical translation** as step zero -- guaranteed baseline output
- **LLM-powered analysis, translation, and repair** via Claude API and Ollama (local)
- **Automatic difficulty classification** and model routing (easy/medium/hard)
- **Differential testing** -- compile both C and Rust, compare outputs
- **Idiomatic scoring** based on unsafe block count and clippy warnings (0-100)
- **Repair loop** with up to 5 iterations before fallback to unsafe
- **MCP server** for IDE integration (Claude Code, editors)
- **RAG pattern store** for learning from past successful migrations

## Quick Start

```bash
cargo install noricum
```

Or build from source:

```bash
git clone https://github.com/JuanMarchetto/noricum
cd noricum && cargo build --release
```

## Usage

```bash
noricum doctor                          # Check tool availability
noricum analyze path/to/file.c          # Analyze difficulty
noricum migrate path/to/file.c          # Migrate with LLM agents
noricum migrate path/to/file.c --no-llm # Migrate without LLM (C2Rust only)
noricum migrate path/to/directory/      # Migrate all C files in a directory
```

### Verbosity

```bash
noricum -v migrate file.c    # Debug logging
noricum -vv migrate file.c   # Trace logging
```

## Configuration

Noricum reads configuration from `noricum.toml` in the project root. Key settings:

```toml
[llm]
primary_provider = "anthropic"
fallback_provider = "ollama"

[llm.anthropic]
analysis_model = "claude-opus-4-6"
translation_model = "claude-sonnet-4-6"
repair_model = "claude-sonnet-4-6"

[migration]
max_repair_iterations = 5
min_idiomatic_score = 60
allow_unsafe_fallback = true

[validation]
differential_testing = true
clippy_check = true
```

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `ANTHROPIC_API_KEY` | Yes (for LLM mode) | Anthropic API key for Claude models |

Run `noricum doctor` to verify that all required tools and credentials are configured.

## Architecture

Noricum is organized as a Cargo workspace with focused crates:

| Crate | Type | Purpose |
|-------|------|---------|
| `noricum-cli` | Binary | clap CLI entry point |
| `noricum-core` | Library | Orchestrator, state machine, model router |
| `noricum-ir` | Library | Semantic Code Map (migration metadata) |
| `noricum-agents` | Library | LLM agents via rig-rs |
| `noricum-tools` | Library | Tool implementations (c2rust, compile, test, clippy) |
| `noricum-validation` | Library | Verification pipeline (diff tests, scoring) |
| `noricum-mcp` | Library + Binary | MCP server for IDE integration |

### Migration Pipeline

```
Pending -> Extracted -> Characterized -> C2RustDone -> Analyzed -> Refined -> Validated
                                                                     |
                                                                     v
                                                              Repairing (max 5)
                                                                     |
                                                                     v
                                                              FallbackUnsafe
```

## MCP Server

Noricum includes an MCP (Model Context Protocol) server for integration with
Claude Code and other MCP-compatible tools.

### Running Standalone

```bash
cargo run --release -p noricum-mcp
```

### Claude Code Integration

Add to your project's `.mcp.json`:

```json
{
  "mcpServers": {
    "noricum": {
      "command": "cargo",
      "args": ["run", "--release", "-p", "noricum-mcp"],
      "env": {}
    }
  }
}
```

### Available MCP Tools

| Tool | Description |
|------|-------------|
| `migrate_function` | Migrate C source to idiomatic Rust |
| `analyze_function` | Classify difficulty and report characteristics |
| `check_compilation` | Check if Rust source compiles |
| `get_idiomatic_score` | Score Rust source for idiomatic quality (0-100) |

## Docker

```bash
docker build -t noricum .
docker run --rm -e ANTHROPIC_API_KEY noricum doctor
docker run --rm -e ANTHROPIC_API_KEY -v $(pwd):/work noricum migrate /work/file.c
```

## License

MIT

## Author

Juan Patricio Marchetto
