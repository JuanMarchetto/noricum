# Noricum

**Autonomous C/C++ to Rust migration agent.**

[![CI](https://github.com/JuanMarchetto/noricum/actions/workflows/ci.yml/badge.svg)](https://github.com/JuanMarchetto/noricum/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![crates.io](https://img.shields.io/crates/v/noricum.svg)](https://crates.io/crates/noricum)

Noricum combines a deterministic pipeline (C2Rust as step zero) with LLM-powered
agents and differential verification to migrate C/C++ code to safe, idiomatic Rust.

## Features

- **C2Rust mechanical translation** as step zero — guaranteed baseline output
- **LLM-powered analysis, translation, and repair** via Claude API and Ollama (local)
- **Automatic difficulty classification** and model routing (easy/medium/hard)
- **Differential testing** — compile both C and Rust, compare outputs byte-by-byte
- **Enhanced idiomatic scoring** based on unsafe count, clippy, positive/negative Rust patterns (0-100)
- **Repair loop** with up to 5 iterations, diff test feedback drives fixes
- **RAG pattern store** for learning from past successful migrations
- **HTML migration reports** with side-by-side code, metrics, and score gauges
- **MCP server** for IDE integration (Claude Code, VS Code)
- **Dependency-aware multi-file migration** with topological ordering

## Benchmark Results

Real migration results on test fixtures (LLM-powered pipeline):

| Source | Lines | Functions | Score | Unsafe | Diff Test | Repairs |
|--------|-------|-----------|-------|--------|-----------|---------|
| `add.c` | 13 | 1 | 100/100 | 0 | PASS | 0 |
| `power.c` | 33 | 3 | 100/100 | 0 | PASS | 0 |
| `gcd.c` | 24 | 2 | 100/100 | 0 | PASS | 0 |
| `factorial.c` | 18 | 1 | 100/100 | 0 | PASS | 0 |
| `fibonacci.c` | 30 | 1 | 100/100 | 0 | PASS | 0 |
| `max_min.c` | 31 | 3 | 100/100 | 0 | PASS | 0 |
| `strlen.c` | 18 | 1 | 100/100 | 0 | PASS | 0 |
| `linked_list.c` | 50 | 4 | 92/100 | 0 | PASS | 0 |
| `error_codes.c` | 43 | 2 | 96/100 | 0 | PASS | 0 |
| `buffer.c` | 36 | 2 | 94/100 | 0 | PASS | 0 |
| **`hash_table.c`** | **204** | **8** | **89/100** | **0** | **PASS** | **0** |
| **`miniz_test.c`** | **154** | **2** | **93/100** | **0** | **PASS** | **0** |

**12/12 files validated, 0 unsafe blocks, 100% diff test pass rate.**

### Noricum vs C2Rust

| Metric | C2Rust Alone | Noricum |
|--------|-------------|---------|
| Translation | Mechanical AST lowering | LLM-powered idiomatic |
| Unsafe blocks | Wraps everything in `unsafe` | 0 across 12+ files |
| Diff test verification | None | Byte-exact + exit code automated |
| Repair loop | None | Up to 5 iterations with diff feedback |
| Avg. idiomatic score | N/A | 89-100/100 |
| `hash_table.c` | ~250 LOC unsafe, raw ptrs | ~180 LOC safe, Vec/Box |
| Float tolerance | N/A | Configurable epsilon comparison |
| Multi-input testing | N/A | Multiple stdin/args per test |
| REST API | None | 6 endpoints (`/api/health`, `/api/migrate`, ...) |

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
# Check tool availability
noricum doctor

# Analyze difficulty of a C file
noricum analyze path/to/file.c

# Migrate with LLM agents (requires ANTHROPIC_API_KEY)
noricum migrate path/to/file.c

# Migrate without LLM (rule-based translation only)
noricum migrate path/to/file.c --no-llm

# Migrate all C files in a directory
noricum migrate path/to/directory/

# Migrate with differential testing verification
noricum migrate path/to/file.c --diff-test

# Generate HTML migration report
noricum migrate path/to/file.c --report output/report.html

# JSON output (for scripting)
noricum migrate path/to/file.c --json

# Specify output directory for generated .rs files
noricum migrate path/to/file.c --output output/
```

### End-to-End Example

```bash
# 1. Set your API key
export ANTHROPIC_API_KEY=sk-ant-...

# 2. Check everything is ready
noricum doctor

# 3. Analyze the file first
noricum analyze tests/fixtures/medium/hash_table.c
# Output: Difficulty: Hard, Functions: 8, Patterns: malloc/free, linked-list chaining

# 4. Migrate with full verification
noricum -v migrate tests/fixtures/medium/hash_table.c \
  --diff-test \
  --report output/hash_table_report.html \
  --output output/medium/

# 5. View results
cat output/medium/hash_table.rs     # Generated Rust code
open output/hash_table_report.html  # Visual report with metrics
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

**Pipeline stages:**

1. **Extraction** — Read C source, create `FunctionUnit`
2. **Difficulty Classification** — Router classifies as Easy/Medium/Hard
3. **C2Rust Transpilation** — Mechanical baseline translation (if available)
4. **Analysis Agent** — LLM analyzes patterns, unsafe constructs, strategy
5. **Translation Agent** — LLM generates idiomatic Rust (with RAG pattern context)
6. **Validation** — Compile check + clippy + diff test + idiomatic scoring
7. **Repair Loop** — LLM fixes errors (compiler errors + diff test mismatches, max 5 iterations)
8. **Fallback** — If repair fails, keep C2Rust/unsafe output
9. **Test Generation** — LLM generates Rust unit tests for validated code

### Idiomatic Scoring

The scoring system evaluates migrated Rust code on a 0-100 scale:

- **Base**: 100 - (unsafe_blocks × 10) - (clippy_warnings × 2)
- **Positive signals** (+2 each, capped at 3): `Result<`, `Option<`, `.iter()`, `impl`, `From<`, `HashMap<`, `&[u8]`, etc.
- **Negative signals**: `.unwrap()` (−3), raw `as` casts (−1), manual `[i]` indexing (−2)
- **LOC ratio bonus** (+5): Rust shorter than C source

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
| `migrate_function` | Migrate C source to idiomatic Rust (full pipeline) |
| `analyze_function` | Classify difficulty and report characteristics |
| `check_compilation` | Check if Rust source compiles |
| `get_idiomatic_score` | Score Rust source for idiomatic quality (0-100) |
| `diff_test` | Run differential test between C and Rust sources |
| `repair` | Re-check compilation and return structured diagnostics |

## Docker

```bash
docker build -t noricum .
docker run --rm -e ANTHROPIC_API_KEY noricum doctor
docker run --rm -e ANTHROPIC_API_KEY -v $(pwd):/work noricum migrate /work/file.c
```

## Testing

```bash
cargo test                           # All unit + integration tests
cargo test --test golden_outputs     # Golden output regression tests
cargo test --test integration_test   # CLI integration tests
cargo clippy --workspace             # Lint check (0 warnings required)
```

## License

MIT

## Author

Juan Patricio Marchetto
