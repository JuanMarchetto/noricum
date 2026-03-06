# I Built an LLM Agent That Migrates C to Safe Rust — With 0 Unsafe Blocks

**TL;DR:** Noricum is an open-source agent that takes C source code and produces idiomatic, safe Rust — verified by differential testing. It migrated a 520-LOC JSON parser (cJSON) to Rust with 0 unsafe blocks, passing byte-exact output comparison. Here's how it works and what I learned.

---

## The Problem

The US government is [pushing for memory-safe languages](https://www.whitehouse.gov/oncd/briefing-room/2024/02/26/press-release-technical-report/). DARPA's TRACTOR program is funding C-to-Rust migration. But migrating C to Rust is *hard*:

- **C2Rust** gives you Rust that compiles but is wrapped in `unsafe` blocks — defeating the purpose
- **Manual rewriting** is expensive and error-prone
- **LLMs alone** produce code that looks right but has subtle behavioral differences

What if we combined all three approaches — mechanical translation as a safety net, LLMs for idiomatic conversion, and automated differential testing to catch regressions?

## Meet Noricum

[Noricum](https://github.com/JuanMarchetto/noricum) is an autonomous migration agent written in Rust. It takes C source code through a 9-stage pipeline:

```
C Source → Extract → Classify Difficulty → C2Rust Baseline
    → LLM Analysis → LLM Translation → Validate → Repair Loop → Tests
```

The key insight: **the LLM doesn't work alone.** Every translation is verified by:

1. **Compiling** the generated Rust
2. **Running clippy** for lint warnings
3. **Differential testing** — compile both C and Rust, run with the same inputs, compare outputs byte-by-byte
4. **Idiomatic scoring** — a 0-100 score based on unsafe blocks, Rust patterns, and code quality

If any step fails, the output goes back to the LLM with the error message for repair — up to 5 iterations.

## The Results

I tested Noricum against 13 C files, from trivial (13 LOC) to complex (520 LOC):

| Source | Lines | Score | Unsafe Blocks | Diff Test |
|--------|-------|-------|---------------|-----------|
| `add.c` | 13 | 100/100 | 0 | PASS |
| `linked_list.c` | 50 | 92/100 | 0 | PASS |
| `hash_table.c` | 204 | 89/100 | 0 | PASS |
| `miniz_test.c` | 154 | 93/100 | 0 | PASS |
| **`cjson_combined.c`** | **520** | **100/100** | **0** | **PASS** |
| **`expr_eval.c`** | **1686** | **100/100** | **0** | **PASS** |

**14/14 files migrated successfully. 0 unsafe blocks across all outputs. 100% diff test pass rate.**

The expr_eval.c migration is the crown jewel — a full expression evaluator with:
- Recursive descent parser (lexer + parser + evaluator)
- 74 functions including 25+ built-in math/string operations
- Manual HashMap implementation with chaining
- DataSet with statistical computations (mean, variance, stddev, median)
- Dynamic variable store with global state
- String manipulation, type coercion, and control flow (if/else/while)

All 1686 lines translated to 1446 lines of safe Rust with zero unsafe blocks, zero repair iterations, and byte-exact output matching.

Noricum converted all of this to safe Rust using:
- `enum JsonValue` with `Vec<(String, JsonValue)>` for objects
- `String` instead of `char *`
- Pattern matching instead of pointer chasing
- Zero `unsafe` blocks

## How It's Different From C2Rust

C2Rust is a great tool, but it produces *mechanical* translations. Here's what C2Rust gives you for a hash table:

```rust
pub unsafe extern "C" fn hash_table_insert(
    mut table: *mut HashTable,
    mut key: *const libc::c_char,
    mut value: libc::c_int,
) { ... }
```

Here's what Noricum produces:

```rust
pub fn insert(&mut self, key: &str, value: i32) {
    let index = self.hash(key);
    // Update existing key
    for entry in &mut self.buckets[index] {
        if entry.key == key {
            entry.value = value;
            return;
        }
    }
    // Insert new entry
    self.buckets[index].push(Entry {
        key: key.to_string(),
        value,
    });
    self.size += 1;
}
```

No `unsafe`, no raw pointers, no `libc` — just idiomatic Rust.

## The Architecture

Noricum is a 7-crate Rust workspace (~13,200 LOC):

- **noricum-core** — Orchestrator with 9-stage state machine
- **noricum-agents** — LLM agents (analysis, translation, repair, test gen) via rig-rs
- **noricum-validation** — Differential testing + idiomatic scoring
- **noricum-tools** — Compilation, C2Rust integration, clippy
- **noricum-ir** — Semantic Code Map tracking migration state
- **noricum-mcp** — MCP server for IDE integration
- **noricum-cli** — CLI with migrate, analyze, doctor, bench commands

The agents use Claude via rig-rs, with Ollama as a local fallback. A RAG pattern store feeds successful migration patterns back to the translation agent.

## Repair Loop: The Secret Sauce

Most LLM-powered code generators stop at "it compiled." Noricum's repair loop is what makes it reliable:

1. LLM generates Rust code
2. Compile check — if it fails, feed errors back to LLM
3. Clippy check — warnings get fed back too
4. Diff test — run both C and Rust with identical inputs
5. If outputs differ, feed the diff back to the LLM
6. Repeat up to 5 times

For `cjson_combined.c`, the first translation had a minor string escaping issue. The repair loop caught it via diff test, fed the mismatch back, and the LLM fixed it in one iteration. Final score: 100/100.

## What I Learned

### LLMs Are Better Translators Than You'd Expect
For simple functions, Claude produces near-perfect idiomatic Rust on the first try. The difficulty is in complex pointer manipulation and manual memory management — exactly where the repair loop earns its keep.

### Differential Testing Is Non-Negotiable
Without diff testing, you'd get code that *looks* correct but behaves differently. String escaping, integer overflow, float precision — these are where silent regressions hide.

### RAG Context Helps a Lot
Feeding the LLM examples of similar successful migrations (ptr → slice, malloc → Vec, hash table → HashMap) dramatically improves first-try quality for medium-difficulty files.

### The 80/20 of C-to-Rust
80% of C functions are straightforward to migrate. 20% require deep understanding of ownership, lifetimes, and unsafe patterns. Noricum's difficulty classifier routes easy functions to cheaper/faster models and hard functions to more capable ones.

## Try It

```bash
git clone https://github.com/JuanMarchetto/noricum
cd noricum && cargo build --release

# Check your setup
./target/release/noricum doctor

# Migrate a C file
export ANTHROPIC_API_KEY=sk-ant-...
./target/release/noricum migrate tests/fixtures/medium/hash_table.c --diff-test --report report.html
```

Noricum also has a REST API, MCP server for IDE integration, and Docker support. See the [README](https://github.com/JuanMarchetto/noricum) for details.

## What's Next

- **Larger migrations** — targeting 1000-2000 LOC files (stb_image, sqlite3 shell)
- **CRUST-Bench evaluation** — running against the academic benchmark dataset
- **Incremental migration** — per-function migration instead of whole-file
- **Published benchmarks** — reproducible comparisons vs C2Rust

---

*Noricum is open source under MIT. Contributions welcome.*

*Built by [Juan Patricio Marchetto](https://github.com/JuanMarchetto) — creator of [SODA](https://github.com/JuanMarchetto/soda) (Solana code generator, 118+ stars).*

**[GitHub](https://github.com/JuanMarchetto/noricum) | [Try it now](https://github.com/JuanMarchetto/noricum#quick-start)**
