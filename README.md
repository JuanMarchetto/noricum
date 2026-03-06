# Noricum

**Autonomous C/C++ to Rust migration agent — 0 unsafe blocks, verified by differential testing.**

[![CI](https://github.com/JuanMarchetto/noricum/actions/workflows/ci.yml/badge.svg)](https://github.com/JuanMarchetto/noricum/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-296%20passing-brightgreen)](https://github.com/JuanMarchetto/noricum)
[![Rust](https://img.shields.io/badge/rust-edition%202024-orange)](https://www.rust-lang.org/)
[![LOC](https://img.shields.io/badge/LOC-~13%2C200-blue)](https://github.com/JuanMarchetto/noricum)

<!-- Demo GIF: replace with actual recording -->
<!-- ![Noricum Demo](demo.gif) -->

> **Migrate C to safe Rust in seconds.** Noricum takes your C source, translates it to idiomatic Rust using LLM agents, then *proves* behavioral equivalence by compiling both and comparing outputs byte-by-byte. If they differ, the LLM fixes it automatically.

Noricum combines a deterministic pipeline (C2Rust as step zero) with LLM-powered
agents and differential verification to migrate C/C++ code to safe, idiomatic Rust.

**[Blog Post](blog/2026-03-06-c-to-rust-llm-agent.md)** | **[Quick Start](#quick-start)** | **[Benchmarks](#benchmark-results)**

## Features

- **C2Rust mechanical translation** as step zero — guaranteed baseline output
- **LLM-powered analysis, translation, and repair** via Claude API
- **Automatic difficulty classification** and model routing (easy/medium/hard)
- **Differential testing** — compile both C and Rust, compare outputs byte-by-byte
- **Enhanced idiomatic scoring** based on unsafe count, clippy, positive/negative Rust patterns (0-100)
- **Repair loop** with up to 5 iterations, diff test feedback drives fixes
- **RAG pattern store** for learning from past successful migrations
- **HTML migration reports** with side-by-side code, metrics, and score gauges
- **MCP server** for IDE integration (Claude Code, VS Code)
- **Dependency-aware multi-file migration** with topological ordering
- **`--docs` flag** — automatically generate Rust doc comments from C source comments
- **Security hardened** — path validation for LLM tools, API auth, CORS restrictions, input size limits

### Planned Features

The following features are on the roadmap but not yet implemented:

- **Incremental migration** — Per-function state tracking across runs
- **Selective function migration** — Migrate specific functions by name
- **Interactive review** — Terminal-based human-in-the-loop review mode

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
| **`cjson_combined.c`** | **520** | **12** | **100/100** | **0** | **PASS** | **1** |

**13/13 files validated, 0 unsafe blocks, 100% diff test pass rate.**

### Noricum vs C2Rust

| Metric | C2Rust Alone | Noricum |
|--------|-------------|---------|
| Translation | Mechanical AST lowering | LLM-powered idiomatic |
| Unsafe blocks | Wraps everything in `unsafe` | 0 across 13+ files |
| Diff test verification | None | Byte-exact + exit code automated |
| Repair loop | None | Up to 5 iterations with diff feedback |
| Avg. idiomatic score | N/A | 89-100/100 |
| `hash_table.c` | ~250 LOC unsafe, raw ptrs | ~180 LOC safe, Vec/Box |
| `cjson_combined.c` | ~520 LOC unsafe, manual alloc | ~400 LOC safe, enum JsonValue |
| Float tolerance | N/A | Configurable epsilon comparison |
| Multi-input testing | N/A | Multiple stdin/args per test |
| REST API | None | 6 endpoints (`/api/health`, `/api/migrate`, ...) |

## Quick Start

Build from source:

```bash
git clone https://github.com/JuanMarchetto/noricum
cd noricum && cargo build --release
# Binary available at target/release/noricum
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

# Generate doc comments on migrated Rust functions
noricum migrate path/to/file.c --docs
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

Configuration is driven by CLI flags and environment variables. Key defaults:

| Setting | Default | CLI Flag / Env Var |
|---------|---------|-------------------|
| Max repair iterations | 5 | — |
| Min idiomatic score | 60 | — |
| Differential testing | enabled | `--diff-test` |
| Clippy check | enabled | — |
| Max token budget | unlimited | `--max-tokens` |

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `ANTHROPIC_API_KEY` | Yes (for LLM mode) | Anthropic API key for Claude models |
| `NORICUM_API_KEY` | No | API key for REST server authentication |
| `NORICUM_CORS_ORIGINS` | No | Comma-separated CORS origins (default: localhost only) |

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

## REST API

Start the API server:

```bash
noricum serve --host 127.0.0.1 --port 3000
```

### Example Requests

```bash
# Health check
curl http://localhost:3000/api/health

# Analyze C source difficulty
curl -X POST http://localhost:3000/api/analyze \
  -H "Content-Type: application/json" \
  -d '{"source": "int add(int a, int b) { return a + b; }"}'

# Check Rust compilation
curl -X POST http://localhost:3000/api/check \
  -H "Content-Type: application/json" \
  -d '{"source": "pub fn add(a: i32, b: i32) -> i32 { a + b }"}'

# Get idiomatic score
curl -X POST http://localhost:3000/api/score \
  -H "Content-Type: application/json" \
  -d '{"source": "pub fn add(a: i32, b: i32) -> i32 { a + b }", "c_source": "int add(int a, int b) { return a + b; }"}'

# Run differential test
curl -X POST http://localhost:3000/api/diff-test \
  -H "Content-Type: application/json" \
  -d '{"c_source": "#include <stdio.h>\nint main(void) { printf(\"5\\n\"); return 0; }", "rust_source": "fn main() { println!(\"5\"); }"}'

# Migrate C to Rust
curl -X POST http://localhost:3000/api/migrate \
  -H "Content-Type: application/json" \
  -d '{"source": "int add(int a, int b) { return a + b; }", "name": "add"}'
```

When `NORICUM_API_KEY` is set, include `-H "X-Api-Key: YOUR_KEY"` in requests.

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
