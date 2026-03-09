# Noricum

**The first open-source, production-ready C-to-Rust migration CLI — 0 unsafe blocks, verified by differential testing.**

[![CI](https://github.com/JuanMarchetto/noricum/actions/workflows/ci.yml/badge.svg)](https://github.com/JuanMarchetto/noricum/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-317%2B%20passing-brightgreen)](https://github.com/JuanMarchetto/noricum)
[![Rust](https://img.shields.io/badge/rust-edition%202024-orange)](https://www.rust-lang.org/)
[![LOC](https://img.shields.io/badge/LOC-~18%2C000-blue)](https://github.com/JuanMarchetto/noricum)

<!-- Demo GIF: replace with actual recording -->
<!-- ![Noricum Demo](demo.gif) -->

> Noricum takes your C source, translates it to idiomatic Rust using LLM agents, then verifies behavioral equivalence by compiling both and comparing outputs byte-by-byte. If they differ, the LLM fixes it automatically.

Noricum combines a deterministic pipeline with LLM-powered agents and differential
verification to migrate C code to safe, idiomatic Rust.

> **Note:** Noricum-generated code is verified by differential testing against specific inputs, not formally proven correct for all possible inputs. Always review migrated code before deploying to production.

**[Blog Post](blog/2026-03-06-c-to-rust-llm-agent.md)** | **[Quick Start](#quick-start)** | **[Benchmarks](#benchmark-results)**

## Why Noricum

Existing C-to-Rust migration tools fall into two camps: **mechanical transpilers** that wrap everything in `unsafe`, and **manual rewriting** that's slow and error-prone. Noricum takes a third approach — a **production-ready agent pipeline** that combines LLM intelligence with automated verification.

| | C2Rust | Manual Rewrite | Noricum |
|---|--------|---------------|---------|
| **Approach** | AST lowering | Human engineer | LLM agent + diff testing |
| **Output safety** | Everything in `unsafe` | Depends on engineer | **0 unsafe blocks** across all validated files |
| **Verification** | Compiles | Code review | Byte-exact differential testing |
| **Auto-repair** | None | N/A | Up to 5 LLM-driven iterations |
| **Speed** | Seconds | Days/weeks | Seconds per file |
| **IDE integration** | None | N/A | MCP server for Claude Code |

**Key differentiator:** Noricum doesn't just translate — it *verifies*. Every migration is validated by compiling both the original C and generated Rust, running them with identical inputs, and comparing outputs byte-by-byte. If they differ, the LLM automatically repairs the translation.

## Features

- **C2Rust mechanical translation** as optional step zero (graceful fallback if not installed)
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

Validated migrations on real-world C libraries (LLM-powered pipeline):

| Source | Lines | Functions | Score | Unsafe | Diff Test | Repairs |
|--------|-------|-----------|-------|--------|-----------|---------|
| **`cjson_full_combined.c`** | **1,441** | **~60** | **95+** | **0** | **PASS** | **0** |
| **`expr_eval.c`** | **1,686** | **74** | **100/100** | **0** | **PASS** | **0** |
| **`genann.c`** | **642** | **18** | **100/100** | **0** | **PASS** | **1** |
| **`cjson_combined.c`** | **520** | **12** | **100/100** | **0** | **PASS** | **1** |
| `hash_table.c` | 204 | 8 | 89/100 | 0 | PASS | 0 |
| `miniz_test.c` | 154 | 2 | 93/100 | 0 | PASS | 0 |

Plus 9 smaller fixtures (13-50 LOC each): all pass with 0 unsafe, scores 92-100.

**0 unsafe blocks across all validated files. All diff tests pass byte-exact.**

**Tested range:** Files up to ~1,700 LOC migrate reliably. A 4,429 LOC file (miniz compression library) exceeded the current pipeline's capacity — see [Known Limitations](#known-limitations).

### Flagship Migrations

#### cJSON (DaveGamble/cJSON — 12,510 stars)

The `cjson_full_combined.c` migration is the largest successful idiomatic migration:

| Metric | Value |
|--------|-------|
| **C LOC** | 1,441 |
| **Rust LOC** | 1,098 (0.76x — more compact than C) |
| **Unsafe blocks** | **0** |
| **Raw pointers** | **0** |
| **Tests** | **55/55 PASS** |
| **Diff test** | **PASS (byte-exact)** |

Key transformations:
- `struct cJSON` (linked list with next/prev/child pointers) → `enum JsonValue` with `Vec`
- `malloc/free` → RAII (automatic `Drop`)
- `char*` strings → `String`
- Type tag integers → enum variants
- `goto fail` error handling → `Option<T>` with `?`
- `cJSON_IsReference` flag → `Clone`
- UTF-16 surrogate pair decoding → `char::from_u32`
- C `sprintf("%1.15g")` number formatting → custom `format_g()` for byte-exact match

#### genann (codeplea/genann — 2,246 stars)

Neural network library migrated fully autonomously (no manual intervention):

| Metric | Value |
|--------|-------|
| **C LOC** | 642 |
| **Rust LOC** | 721 |
| **Unsafe blocks** | **0** |
| **Diff test** | **PASS (521,556 byte-exact assertions)** |
| **Repairs** | 1 (automatic) |

Key transformations:
- Function pointers → `ActivationFn` enum dispatch
- Single-malloc flat array → separate `Vec<f64>` per layer
- Global lookup table → struct field
- glibc RNG reimplemented (TYPE_3 degree-31 LFSR)
- `FILE*` I/O → `std::io::Read`/`Write` traits

### Known Limitations

- **Scale ceiling:** Reliable up to ~1,700 LOC single-file migrations. Files >2,000 LOC use chunked translation which may introduce cross-chunk inconsistencies. A 4,429 LOC file (miniz compression) resulted in `FallbackUnsafe` — documented in [docs/research/miniz-migration-attempt.md](docs/research/miniz-migration-attempt.md)
- **C only:** C++ is not currently supported. Target is C11 (`-std=gnu11` with POSIX extensions)
- **LLM dependency:** Full pipeline requires an Anthropic API key (Claude). Ollama local fallback is available but produces lower quality output. Cost per migration: ~$0.02 for small files, ~$5-15 for complex 1,000+ LOC files
- **Non-deterministic:** LLM outputs vary between runs. The same C file may produce different (but equivalent) Rust translations
- **Test coverage:** Differential testing verifies behavioral equivalence for tested inputs, not all possible inputs. Edge cases not covered by the test harness may differ
- **Not suitable for:** Safety-critical systems without thorough human review, files with inline assembly, heavily preprocessed code (complex `#ifdef` chains), or C code relying on undefined behavior

### Noricum vs C2Rust

| Metric | C2Rust Alone | Noricum |
|--------|-------------|---------|
| Translation | Mechanical AST lowering | LLM-powered idiomatic |
| Unsafe blocks | Wraps everything in `unsafe` | 0 across all validated files |
| Diff test verification | None | Byte-exact + exit code automated |
| Repair loop | None | Up to 5 iterations with diff feedback |
| Avg. idiomatic score | N/A | 89-100/100 |
| `hash_table.c` | ~250 LOC unsafe, raw ptrs | ~180 LOC safe, Vec/Box |
| `cjson_combined.c` | ~520 LOC unsafe, manual alloc | ~400 LOC safe, enum JsonValue |
| `cjson_full_combined.c` | ~1441 LOC unsafe, linked lists | ~1098 LOC safe, Vec/enum (0.76x) |
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
