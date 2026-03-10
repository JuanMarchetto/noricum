# P30: Hybrid Repair Engine — Design Spec

## Problem

The current assembly repair loop sends the entire assembled output (~3000-6000 LOC) to the LLM and asks it to fix compilation errors. This causes:

1. **Truncation** — LLM regenerates ~1000 LOC, losing 60-90% of functions
2. **Context overload** — LLM gets confused by volume, introduces new errors
3. **Token waste** — ~30K input + ~15K output per iteration, 8 iterations = ~360K tokens
4. **Stall on ownership patterns** — The same 5 mechanical errors persist across all iterations

Evidence from 4 runs of miniz_zip.c (4895 LOC C):
- Run 3 (Claude): 95→4 errors in 6 iters, stalled on ownership patterns
- Run 4 (DeepSeek): 10→4 errors in 4 iters, retranslation destroyed code (59→4 functions)
- Manual fix of Run 3 iter-05: 5 mechanical fixes → compiles (2829 LOC, 96 functions)

## Solution

Three-phase hybrid repair, applied only to assembled outputs (>MODULAR_FILE_LOC lines):

```
Assembly output
    │
    ▼
Phase 1: Rule Engine (deterministic, 0 LLM calls)
    │ rustc → 0 errors? → Done
    ▼
Phase 2: Surgical Repair (focused LLM, ~100 LOC per fix)
    │ rustc → 0 errors? → Done
    ▼
Phase 3: Full Repair (legacy whole-file, max 3 iterations)
```

For files <MODULAR_FILE_LOC, the existing repair loop continues unchanged.

## Phase 1: Rule Engine

Module: `crates/noricum-tools/src/repair_rules.rs`

Five deterministic rules, each a function `fn apply(source: &str, errors: &[CompilerError]) -> String`:

| Rule | Error Code | Detection | Fix |
|------|-----------|-----------|-----|
| R1: Clone bounds | E0599 | Generic fn missing Clone, body calls `.clone()` | Add `+ Clone` to type bound |
| R2: downcast_mut | E0599 | Method call on `dyn Any` | Insert `.downcast_mut::<ConcreteType>()` |
| R3: Split borrow | E0499 | Double `&mut` borrow in same scope | Inline helper or extract field before loop |
| R4: mut binding | E0596 | Mutate through `Option<&mut T>` binding | Add `mut` to binding |
| R5: ref mut pattern | E0596 | `if let Some(v) = opt_mut_ref` | Change to `Some(ref mut v)` |

Rules are conservative — if not confident in the match, they skip (no-op). Re-compile after each applied rule.

Input: Rust source + parsed compiler errors (error code, line number, message).
Output: Modified Rust source (or unchanged if no rules matched).

### CompilerError struct

```rust
pub struct CompilerError {
    pub code: String,      // "E0499"
    pub line: usize,       // 1-indexed
    pub column: usize,
    pub message: String,   // full error message
    pub snippet: String,   // the source line(s) referenced
}
```

Parser: `fn parse_rustc_errors(rustc_output: &str) -> Vec<CompilerError>` — regex-based extraction of error code, line, message from rustc stderr.

## Phase 2: Surgical Repair

Module: `crates/noricum-core/src/surgical_repair.rs`

For each remaining error after Phase 1:

1. **Parse error** — Extract error code, line number, message
2. **Extract failing function** — Find the function containing the error line. Search upward for `fn `, downward for balanced `}`. Result: ~50-150 LOC.
3. **Gather context** — For the failing function:
   - Struct/enum definitions it references (grep types from signature + body)
   - The exact rustc error message
   - Relevant pattern from rust-idioms.md (matched by error code)
   - Signatures of sibling functions it calls (not bodies)
4. **Focused LLM call** — Send ~100 LOC context + function + error. Prompt instructs: "Fix ONLY this function. DO NOT redefine types. Return ONLY the fixed function."
5. **Splice back** — Replace the original function in the full source with the fixed version
6. **Re-compile** — Check if error is resolved

Max 5 surgical repair cycles. Each cycle targets one error.

Token estimate per fix: ~2K input + ~1K output (vs ~45K for whole-file repair). ~15x more efficient.

## Phase 3: Full Repair (Fallback)

The existing repair loop, limited to max 3 iterations. Only reached if Phases 1-2 leave unresolved errors. Uses `select_repair_model()` (fast model). No retranslation (P28 already prevents this for assembly).

## Integration

```rust
// In orchestrator.rs, Stage 7:
if !validation.passed {
    if c_lines > MODULAR_FILE_LOC {
        // P30: Hybrid repair for assembled outputs
        hybrid_repair(&mut unit, &client, &provider_config, difficulty, &config, &artifacts).await?;
    } else {
        // Existing repair loop for small/medium files (unchanged)
    }
}
```

## Files

| File | Action | Purpose |
|------|--------|---------|
| `crates/noricum-tools/src/repair_rules.rs` | Create | Rule engine: 5 mechanical rules + error parser |
| `crates/noricum-tools/src/lib.rs` | Modify | Add `pub mod repair_rules` |
| `crates/noricum-core/src/surgical_repair.rs` | Create | Function extraction, context gathering, splice |
| `crates/noricum-core/src/lib.rs` | Modify | Add `pub mod surgical_repair` |
| `crates/noricum-core/src/orchestrator.rs` | Modify | `hybrid_repair()` function, Stage 7 integration |

## Testing

- **Unit tests for rule engine**: Use Run 4 iter-04-rejected.rs (4 errors, 59 functions) as fixture. Rule engine should resolve at least 3 of 4 errors.
- **Unit tests for error parser**: Known rustc output → parsed CompilerError structs.
- **Unit tests for function extraction**: Given line number → extract correct function boundaries.
- **Unit tests for splice**: Replace function → verify surrounding code unchanged.
- **Integration test**: Full hybrid_repair on iter-04 fixture → should compile.

## Success Criteria

- iter-04-rejected.rs (4 errors) → compiles after Phase 1 alone (0 LLM calls)
- Total token usage for assembly repair reduced by 10x+
- No regression on existing small/medium file migrations
