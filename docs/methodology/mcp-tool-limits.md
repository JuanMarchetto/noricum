# MCP Tool Limitations — honest disclosure

What works and what doesn't when calling Noricum's MCP tools from an external Claude Code session. Based on findings from the `feat/interactive-spike` session (2026-04-11).

## TL;DR

The Noricum MCP server exposes 6 tools, but **not all of them are usable end-to-end from outside the CLI**. Specifically, `migrate_function` only runs the first stage of the pipeline state machine and returns an empty `rust_source`. The other tools were not audited in the same session but should be assumed suspect until verified.

Before building anything that depends on Noricum's MCP tools being fully functional, run a manual audit against each one with a sample input and confirm it produces non-empty output.

## Confirmed broken (or partial)

### `mcp__noricum__migrate_function`

**Expected behavior:** takes a C function source, returns the migrated Rust source.

**Observed behavior:** returns

```json
{
  "diff_test_passed": null,
  "difficulty": "Medium",
  "idiomatic_score": null,
  "rust_source": "",
  "state": "Extracted",
  "unsafe_count": null
}
```

The `rust_source` is empty. The `state` is `Extracted`, which is the FIRST stage of the pipeline state machine:

```
Pending -> Extracted -> Characterized -> C2RustDone -> Analyzed -> Refined -> Validated
```

The tool dispatches only the extract phase and returns without running the translate → analyze → refine → validate sequence that the full CLI runs internally. The empty `rust_source` is the tell.

**Workaround:** interactive agents doing C-to-Rust migrations should write the Rust directly using their own reasoning + reference material (see [Architectural Seed Registry](architectural-seeds.md)), and use `check_compilation` / `diff_test` tools only for verification.

## Not yet audited (assume suspect)

### `mcp__noricum__analyze_function`

Claimed behavior: analyze a function for migration difficulty. Whether it produces useful output via MCP is unverified.

### `mcp__noricum__check_compilation`

Claimed behavior: check if Rust source compiles. Should work since it's just a wrapper around `rustc`. Not audited in the interactive spike session because Rust compilation was verified locally via `cargo test`.

### `mcp__noricum__get_idiomatic_score`

Claimed behavior: score Rust source for idiomatic-ness. Unverified whether it returns non-null.

### `mcp__noricum__diff_test`

Claimed behavior: compile C and Rust source with `main()`, run both, compare outputs byte-by-byte. Unverified whether this works for non-main binaries or libraries.

### `mcp__noricum__repair`

Claimed behavior: repair Rust code. Not audited; assume it has similar pipeline-state-machine limitations to `migrate_function`.

### `mcp__noricum__behavioral_review`

Not audited.

## Recommended MCP audit procedure

Before any new session that depends on the MCP tools, run this 5-minute sanity check from inside a Claude Code session with noricum-mcp loaded:

1. **`migrate_function`** with a trivial C function (`int add(int a, int b) { return a + b; }`). Expect: `rust_source` is non-empty. Record actual: ???
2. **`analyze_function`** with the same input. Expect: non-null analysis fields. Record actual: ???
3. **`check_compilation`** with a trivial Rust snippet (`fn main() {}`). Expect: success. Record actual: ???
4. **`get_idiomatic_score`** with the same snippet. Expect: non-null score. Record actual: ???
5. **`diff_test`** with matching trivial C + Rust `main()` printing "hello". Expect: pass. Record actual: ???
6. **`repair`** with a Rust snippet that has a known error. Expect: repaired source. Record actual: ???

If any of these fail or return empty fields, note it here and update this doc. The audit should be re-run after any change to the MCP server's stage routing.

## Implication for the product story

The MCP tools are a **good product story**. They're marketed as reusable primitives that any agent can drive. The reality right now is that at least one of them (`migrate_function`) is incomplete when driven via MCP. Before shipping Noricum as "an MCP toolkit for C-to-Rust migrations", every tool needs to be audited end-to-end and either fixed or removed from the advertised surface.

Two paths forward:

1. **Fix the MCP server's stage routing.** The CLI runs the full pipeline; the MCP server should too. This is probably a missing `async` chain in the MCP tool handlers that stops at the first stage instead of running all of them.
2. **Rename the tools to match what they actually do.** If `migrate_function` only extracts, rename it to `extract_function_for_migration` and add new tools that expose the downstream stages. This makes the API honest about its granularity.

The interactive spike methodology in this directory was designed assuming the MCP tools work; it still works when they don't (the director writes Rust directly), but the promise of "reusable toolkit" is weaker than it looks.

## Historical note

This limit was discovered by accident during the miniz_zip.c spike — I called `migrate_function` on a trivial wrapper function and got back empty `rust_source`. If I had tried to build the whole spike on top of `migrate_function`, that empty output would have killed the methodology. Catching it early was pure luck; a systematic audit would have caught it deterministically.

**Moral:** run the 5-minute audit above before any spike that plans to lean on MCP tools.
