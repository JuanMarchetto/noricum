# Interactive Spike Mode

A human-in-loop + LLM director methodology for C-to-Rust migrations that the autonomous pipeline cannot handle. Validated on miniz_zip.c (4895 LOC C library, 17 consecutive autonomous failures) in 66 minutes of session time.

## When to use

- A C file where the autonomous pipeline has failed 3+ times, OR
- Target >2500 LOC with multi-layer wrappers / legacy API baggage, OR
- Scope that the pipeline's hardcoded stages cannot restructure (e.g., needs different data-flow architecture, not just different translation).

For <1000 LOC leaf functions, the autonomous pipeline still wins on cost per attempt. Spike mode is for the frontier, not the baseline.

## Roles

- **Operator** — human engineer, typically the person who has most context on the target C library. Makes judgment calls, approves direction changes, owns timebox enforcement.
- **Director** — Claude Code (or equivalent agent) in an interactive session. Reads C source, writes Rust, runs cargo test, reports findings. Does not make scope decisions without operator approval.
- **Toolkit** — the MCP tools and primitives from the noricum stack (`check_compilation`, `diff_test`, `artifact_store`, etc.) plus any additional primitives created during the spike.

## Phases

### Phase 0 — Oracle harness (pre-clock, ~2-3 hours, NOT in budget)

This is setup, not experiment. The spike clock does not start until Phase 0 is green. See [Phase 0: Oracle Harness](phase-0-oracle-harness.md) for the full recipe.

Key insight: **the oracle is the truth source, not the existing Rust code**. Phase 0 produces a C wrapper compiled via `cc-rs` into a static lib, plus deterministic fixtures, plus a Rust differential test that walks every fixture via the C oracle and snapshots `(name, size, crc32, first_bytes)` tuples. When this test is green, Hour 0 of the spike begins.

### Hour 0 — Type contract

Open Claude Code in the spike directory. Load the relevant noricum-mcp tools. First prompt is verbatim (approved by operator), typically of the form:

> "Read `{target}.h` and `{reference_crate}/src/types.rs`. Produce a Rust type contract in `src/contract.rs` for the following types: [list]. Constraints: (a) use concrete sum types, not trait objects; (b) no raw pointers; (c) no C-style type aliases (use native Rust types); (d) no forward-declared empty structs; (e) no field names with `m_` prefix. Verify it compiles standalone with `cargo check` before returning."

Reference crate comes from the [Architectural Seed Registry](architectural-seeds.md). Duration: 30-60 min typically, shorter if the reference crate provides clean types.

### Hours 0-3 — Bottom-up function-by-function

Implement functions in call-graph order. For each function:
1. Director writes Rust using the type contract + C source for reference.
2. Director runs `cargo test` on the new function's differential test.
3. If green, operator reviews briefly and director commits.
4. If red, director debugs + retries up to 3 times.
5. If still red after 3 retries, mark blocked, continue with next function, return at end.

**Hour-3 gate:** the simplest non-empty fixture must pass differential test by hour 3. If not, abort and re-assess methodology before continuing.

### Hours 3-5 — Writer path (if the library has one)

Same discipline as reader path. For formats with writer, each writer function must round-trip through BOTH the Rust reader and the C oracle.

### Hours 5-7 — Iteration buffer

Reserved for functions that didn't pass cleanly. Use `repair` MCP tool or manual intervention. If a repair attempt destroys working code, roll back immediately.

### Hour 7-8 — Full corpus diff

Run the complete differential test across all fixtures. If green, commit the branch and start drafting the blog post during the final 30 minutes while it's fresh. If red, save the transcript + current state + failing fixtures list — that becomes the post-mortem artifact.

## Hard rules

- **8-hour budget is absolute.** Hour 8 is exit regardless of state. Exceeding the budget means the spike failed the methodology test even if the code eventually works.
- **Commit per function, not per feature.** Each function's diff test passing is one atomic commit. This gives a clean session log for the blog post.
- **No scope creep without operator approval.** If the director wants to add a feature, restructure a module, or bring in a new dependency, it pauses and asks.
- **C source is authoritative reference, not inspiration.** For binary formats with specific byte layouts, the spec lives in the C code. The director reads it.
- **The differential test is the oracle.** If `cargo test --test differential_zip` is green, the migration is correct. If it's red, the migration is wrong. No other authority.

## Counter-indications

- File <1000 LOC with no prior pipeline failures — pipeline is cheaper.
- Large project where the goal is CI integration, not a one-shot migration — build the pipeline instead.
- Target depends on non-existent Rust crates (no architectural seed available).
- Pure algorithm ports (FFT, regex engine, parser combinator) where the data flow IS the algorithm — no architectural elimination possible.

## Reference session

Branch `feat/interactive-spike` of `JuanMarchetto/noricum`. 12 commits, `cf4a07f` (Phase 0) through `64c4d3d` (skill learnings). Session duration: 66 minutes wall clock. Output: 1157 LOC Rust, 28 tests passing, 0 unsafe, byte-match with C oracle on 9 real-world archives / 21,331 entries.
