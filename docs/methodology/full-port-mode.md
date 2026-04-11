# Full-Port Mode

A multi-session methodology for complete C-library→safe-Rust migrations that are too large for the 8-hour interactive spike. Derived from, and explicitly distinct from, [Interactive Spike Mode](interactive-spike-mode.md).

## When to use

Use Full-Port Mode when ALL of the following are true:

- The target is a self-contained C codebase >10,000 LOC, OR the target is the reference implementation of a language/runtime/protocol (Lua, Python, zlib, OpenSSL, SQLite).
- The operator has committed to a complete drop-in replacement, not a research port or a blog-post deliverable.
- There is no acceptable scope reduction — every function in the C surface must eventually exist in Rust with matching semantics.
- The project is expected to span weeks-to-months of calendar time.

If the target is smaller or the operator will accept a scope cut to fit 8 hours, use [Interactive Spike Mode](interactive-spike-mode.md) instead.

## What Full-Port Mode rejects from Spike Mode

- **The 8-hour budget.** Removed entirely. Sessions end at natural checkpoints (function complete, subsystem complete, test suite green), not at a clock.
- **"Commit per function" as an absolute.** Still the default, but some subsystems (GC, VM dispatch) need multi-function commits that land as one atomic change because partial landings break the build.
- **"One blog post at the end".** Instead: a running decision log (`docs/{target}-migration/plan.md`) that functions as the blog post source material throughout.

## What Full-Port Mode keeps from Spike Mode

- **Phase 0 oracle harness is mandatory.** Same recipe as [phase-0-oracle-harness.md](phase-0-oracle-harness.md). The C oracle is the ground truth for every differential test.
- **No scope creep without operator approval.** Director still pauses for architectural decisions.
- **C source is the authoritative reference.** On byte layout, ABI, and semantic edge cases, the C original wins every disagreement with the Rust seed crate.
- **Differential test is the oracle.** If the diff test says the migration is wrong, the migration is wrong. No other authority.

## The seven lock-in decisions (Hour 0 of the project, not the session)

Before any code is written, the operator + director lock in seven decisions. These CANNOT be changed mid-project without a declared re-assessment (see "Re-assessment protocol" below). Changing any of them retroactively invalidates all prior work.

1. **Dependency budget** — FREE / MATCHING / ZERO. See [dependency-budget.md](dependency-budget.md).
2. **Error model** — `Result<T, E>` threading vs `catch_unwind` vs mix. Affects every function signature in the core.
3. **Memory model** — owned-tree vs arena-indexed vs raw-pointer-GC. Affects every type definition.
4. **Concurrency model** — how coroutines / fibers / async are represented. Stackful crate vs CPS rewrite vs OS threads vs "not supported".
5. **ABI compatibility** — drop-in `extern "C"` vs Rust-native API vs both (layered).
6. **Binary format compatibility** — wire/bytecode/file-format byte-exact or diverged.
7. **Architectural seed** — which reference Rust crate (if any) provides the type vocabulary. Inspiration-only vs code-reuse.

These seven must be written to **persistent project memory** (not just session memory) before the first commit. The memory entry is the canonical spec; subsequent sessions read it as their first action.

## Session structure

Each session opens with a mandatory three-step boot:

1. **Read persistent memory** for the project's seven decisions and current module status.
2. **Read the migration plan doc** (`docs/{target}-migration/plan.md`) for the current checkpoint, the next module, and any open blockers.
3. **Run `cargo test`** to confirm the branch is in a known-green state before touching anything.

If any of these three surfaces a drift (memory contradicts plan, plan contradicts code, tests are red), the session's first job is to reconcile. **No new migration work until the known-state is consistent.**

Each session closes with a mandatory two-step shutdown:

1. **Update the plan doc** — mark completed modules, log any decisions taken, write any new blockers.
2. **Update persistent memory** — if a lock-in decision had to be revised, log it in memory with date and rationale.

Closing without these steps is treated as a partial session and the next session must re-do the delta.

## Module migration order (the plan doc template)

For any language runtime or complex C library, the order is almost always:

1. **Foundation** (leaf dependencies, depend on nothing from the target)
2. **Core types** (the type contract — the data structures every other module uses)
3. **Memory/GC** (because every type eventually allocates)
4. **Primitive operations** (table access, string interning, arithmetic)
5. **Public API layer** (the `extern "C"` surface if ABI-compatible)
6. **Execution engine** (the VM, the main loop, the dispatch)
7. **Frontend / compiler** (parser, codegen — only after the execution engine can run their output)
8. **Serialization** (dump/undump, save/load, marshal)
9. **Standard library** (each stdlib module is a mini-port of its own)
10. **Dynamic loading / host integration** (loadlib, dlopen, plugins)
11. **Entry-point binary** (the CLI / REPL / host program)

Parser and compiler move LATE in the order because they emit bytecode that the execution engine consumes. If the execution engine can't run handcrafted bytecode yet, there's nothing to validate the parser against. Spike Mode's "bottom-up function-by-function" doesn't capture this — it assumes a much simpler dependency shape.

## Commit cadence

- **Per-function atomic commits** where the function has a meaningful standalone differential test. This is the default.
- **Per-module atomic commits** where a module's public functions are mutually recursive and must land together. Document in the commit message that this is an intentional batch and why.
- **Per-subsystem atomic commits** for GC, VM dispatch, and anything where partial landing breaks the build. Rare, should be the exception.

Never: "WIP" commits, "checkpoint" commits, or "will fix later" commits on the main working branch.

## Testing discipline

- Every function that passes a differential test stays passing. **Regressions block all other work** — no "I'll come back to this".
- The full test suite runs at the start AND end of every session, on the working branch, in a known environment.
- Differential test coverage is tracked in the plan doc as "X of Y C-functions have a Rust equivalent passing diff-test". The goal of every session is to raise X.
- Once the target's native test suite (e.g., Lua's `lua-tests`) can run, that becomes the coverage meter instead.

## Re-assessment protocol

If partway through the project the operator discovers that one of the seven lock-in decisions was wrong, they trigger re-assessment:

1. **Pause all migration work.** The working branch stays where it is; no new module translation.
2. **Write a re-assessment note** in `docs/{target}-migration/reassessment-{date}.md` explaining what was wrong, what the new decision is, and what code has to be re-done.
3. **Estimate the rework cost.** If it's > 30% of existing work, the operator should seriously consider aborting and starting fresh on a new branch with the corrected decision.
4. **Update persistent memory** with the new decision + date + link to the re-assessment note.
5. **Execute the rework as a first-class task** before any new module work.

Re-assessments are painful by design. The seven decisions are lock-in BECAUSE changing them retroactively is catastrophic; the protocol exists to make the cost visible so the operator thinks twice before triggering it.

## Binary size discipline

For ZERO-mode ports in particular, binary size can silently explode via `format!` / `core::fmt::Arguments`. Add a size check to the session shutdown routine:

```sh
cargo build --profile release-min --bin {target}
ls -la target/{arch}/release-min/{target}
```

Track the size in the plan doc's progress table. A 20% session-over-session increase is a warning. A 50% increase is a stop-and-investigate.

## Counter-indications

- **If the target is <5000 LOC,** use interactive-spike-mode instead. Full-port machinery is overhead for small targets.
- **If you don't have a running C oracle,** stop and build one. Full-Port Mode without a diff-test oracle is translation by faith.
- **If the operator isn't willing to commit weeks of calendar time,** either shrink the scope or pick a different methodology. Full-Port Mode half-done is worse than a spike done, because the branch accumulates un-landable state.
- **If the target depends on features Rust can't express,** stop and design a workaround before starting. Examples: "needs setjmp across C-Rust-C call boundary" (consider `catch_unwind` with `extern "C-unwind"`), "needs stackful coroutines without deps" (requires CPS rewrite — confirm budget first).

## Reference projects

- **Lua 5.4 full port** (branch `feat/interactive-spike-lua`, started 2026-04-11). 28K LOC, 7 lock-ins: ZERO / Result-threading / manual arena / CPS / drop-in ABI / bytecode-compat / piccolo-inspiration-only. See `docs/lua-migration/plan.md`.

When this doc gets a second reference entry, add it here with the same seven-decision summary.
