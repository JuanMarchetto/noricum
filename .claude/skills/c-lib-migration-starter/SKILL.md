---
name: c-lib-migration-starter
description: Starter checklist for an interactive spike migrating a C library to Rust. Chains the 8 Phase-0 setup steps plus the initial dependency-budget decision. Use when: start c-to-rust spike, new migration target, bootstrap spike, begin interactive spike, new c-lib migration.
---

# C Library Interactive Migration Starter

This skill walks you through the first 15 minutes of an interactive spike so you don't have to re-derive the setup every time. It chains the Phase 0 oracle harness recipe with the dependency budget decision and the architectural seed selection.

## When to use

The user says something like "I want to migrate X.c to Rust", "start an interactive spike on libfoo", "let's try the spike approach on this library". This skill takes you from zero to "Hour 0 is about to start" deterministically.

Do NOT use this skill if the user wants the autonomous pipeline (`noricum migrate`) — that's a different codepath. This skill is for interactive human-in-loop spikes.

## Pre-flight

Run the audits that protect against wasted Phase 0 time:

1. **MCP tool audit** — if the spike plans to use `migrate_function` or other noricum-mcp tools, run `tools/spike/mcp-audit.sh` and confirm the tools you need actually work. At least one known tool (`migrate_function`) returns empty output via MCP. See `docs/methodology/mcp-tool-limits.md`.

2. **Scaffold check** — verify `tools/spike/scaffold.sh` is executable. That's the single biggest time saver for Phase 0.

## Step 1 — Understand the target

Ask the user for:

- **Target:** path to the C source tree + the main `.c` and `.h` files
- **Size:** rough LOC count (`wc -l target/*.c`). If <1000 LOC, flag that the autonomous pipeline is probably cheaper than an interactive spike.
- **Prior attempts:** has the autonomous pipeline tried this before? How many times did it fail? At what step? If 0 prior attempts and <2500 LOC, recommend running the autonomous pipeline first.
- **Architectural seed availability:** is there an idiomatic Rust crate for this library category? Check `docs/methodology/architectural-seeds.md`. If no seed exists, the spike is harder because you'll derive the type contract from C alone.

## Step 2 — Dependency budget decision

This is the critical fork. Ask the user via `AskUserQuestion`:

> "What's the dependency budget for this spike? The choice affects binary size, reproducibility, and how much code you'll write.
>
> - **FREE** — delegate algorithms to best-in-class crates (flate2, aes, etc.). Best for research spikes, demos, reference implementations. 3-5x binary vs C, 0.15-0.25x source.
> - **MATCHING** — match the C original's dep count (usually 0-2). Vendor primitives as source. Best for drop-in replacements. 2-3x binary vs C, 0.3-0.5x source.
> - **ZERO** — no external runtime deps. Everything vendored. Best for embedded / WASM / defense. 2x binary vs C, 0.4-0.7x source."

Record the choice. It goes in `SPIKE.md` and is enforced by `tools/spike/dep-gate.sh` on each commit.

See `docs/methodology/dependency-budget.md` for the full recipe for each mode, including conversion paths.

## Step 3 — Architectural seed selection

Read `docs/methodology/architectural-seeds.md` and identify the best Rust crate to steal types from. For known categories:

- ZIP / archive formats → `zip-rs/zip2`
- HTTP parsers → `hyperium/http` + `hyper`
- JSON → `serde-rs/json` + `dtolnay/miniserde`
- Regex → `rust-lang/regex`
- Deflate → `Frommi/miniz_oxide` (use as dep in FREE mode, vendor source in MATCHING/ZERO)
- Crypto → `RustCrypto/*` (use as deps in FREE, vendor in MATCHING/ZERO)

Clone the reference crate to a tempdir. You'll read its `src/types.rs` during Hour 0.

## Step 4 — Scaffold

Run:

```sh
tools/spike/scaffold.sh <spike-name> <c-lib-dir>
```

This creates `noricum-spike-<name>/` with:
- Cargo.toml (with [workspace] to break inheritance, and the release-min profile)
- build.rs (cc-rs compiling the C sources)
- wrapper.h / wrapper.c (stubs)
- wrapper_smoke.c (template)
- src/lib.rs (extern block stub)
- fixtures/gen_fixtures.py (template)
- tests/differential_<name>.rs (template)
- SPIKE.md (Phase 0 checklist)

## Step 5 — Fill in the wrapper

The scaffold produces stubs. You (or the user with guidance) must:

1. Edit `wrapper.h` to define the opaque handle + 5-10 thin wrapper functions around the target library's public API.
2. Edit `wrapper.c` to implement each wrapper function as a delegation to the target library, heap-allocating internal state.
3. Edit `src/lib.rs` extern block to match `wrapper.h`.

For zip-like formats, use the wrapper shape from the miniz_zip spike as a reference:
```c
typedef void* spike_handle_t;
spike_handle_t wr_reader_open(const char* filename);
int wr_reader_get_num_files(spike_handle_t h);
int wr_reader_locate(spike_handle_t h, const char* name);
int wr_reader_extract(spike_handle_t h, int idx, void* buf, size_t cap);
int wr_reader_stat(spike_handle_t h, int idx, char* name_out, size_t name_cap, size_t* size_out, unsigned int* crc32_out);
void wr_reader_close(spike_handle_t h);
```

For other format categories, look at the corresponding architectural seed and replicate its minimal API surface.

## Step 6 — Fixture corpus

Edit `fixtures/gen_fixtures.py` to deterministically produce 10-15 fixtures covering the format's edge cases (empty, small, large, unicode, nested, compressed, various encodings). For ZIP, use Python's `zipfile`. For other formats, find the equivalent.

Run the generator and commit both the script and the generated fixtures.

## Step 7 — Smoke test

Compile and run `wrapper_smoke.c` with `gcc`:

```sh
gcc -o wrapper_smoke wrapper_smoke.c wrapper.c <target>.c -I.
./wrapper_smoke
```

Must exit 0 before Rust is touched. If not, the wrapper has a bug.

## Step 8 — Oracle self-test

Run `cargo test --test differential_<name> -- oracle_selftest`. Must pass. When it does, Phase 0 is green and **Hour 0 of the spike begins**.

## Step 9 — Start the clock

```sh
tools/spike/time-tracker.sh start
tools/spike/time-tracker.sh install-hook  # optional, once per repo
```

The 8-hour budget starts NOW. Every commit from this point forward is logged with a `Spike-Elapsed:` trailer.

## Step 10 — Open the main spike session

Hand off to the `noricum-dev` skill's "Interactive Spike Mode" section for the actual hour-by-hour spike flow. The first prompt you (Claude Code) issue should match this template:

> "Read `{target}.h` and `{reference_crate}/src/types.rs` (clone from {url} to a tempdir if needed). Produce a Rust type contract in `src/contract.rs` for the following types: [ask user which types]. Constraints: (a) use concrete sum types, not `Box<dyn>` trait objects with multiple non-auto traits; (b) no raw pointers; (c) no C-style type aliases (native Rust types only); (d) no forward-declared empty structs; (e) no `m_` field prefix. Dependency budget: {FREE|MATCHING|ZERO}. Verify it compiles standalone with `cargo check` before returning."

## Success criteria for this skill

You've succeeded if:

- Phase 0 is green in under 15 minutes of interactive time (scaffold + fill + smoke + oracle selftest)
- The user has consciously chosen a dependency budget mode
- An architectural seed has been identified
- The 8-hour clock has started
- The user is ready to issue the first translation prompt

If any of those is missing, you haven't completed the starter and shouldn't proceed to the migration itself.

## Reference

- `docs/methodology/README.md` — index of all methodology docs
- `docs/methodology/phase-0-oracle-harness.md` — full recipe
- `docs/methodology/dependency-budget.md` — mode choice
- `docs/methodology/architectural-seeds.md` — crate registry
- `docs/methodology/interactive-spike-mode.md` — what comes after this skill finishes
- `.claude/skills/noricum-dev/SKILL.md` — general noricum conventions
- `.claude/skills/rust-migration/SKILL.md` — Rust translation patterns
