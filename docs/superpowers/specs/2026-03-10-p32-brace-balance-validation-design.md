# P32: Brace-Balance Validation for Module Outputs

## Problem

LLM translation can truncate function bodies, producing Rust code with unmatched opening braces. In Run 6, module mz_p2's `mz_zip_reader_read_central_dir` opened a brace at line 982 but never closed it. This caused a cascading "unclosed delimiter" error that propagated through all subsequent modules in the assembly (brace depth reached 5 by EOF).

The current pipeline has no structural validation of module outputs before assembly. Brace imbalance in one module breaks the entire assembled file.

## Solution

Approach 1: Validate → Re-translate (1 retry) → Auto-close fallback.

### Components

**1. `check_brace_balance()` in `repair_rules.rs`**

Counts `{` and `}` in Rust source, skipping string literals and line comments. Returns the final depth (0 = balanced, >0 = unclosed braces, <0 = extra closing braces).

**2. `auto_close_braces()` in `repair_rules.rs`**

If depth > 0, appends that many `}` lines to the end (with `// auto-closed: truncated output` marker). If depth <= 0, returns source unchanged.

**3. Integration in `migrate_single_module()` in `orchestrator.rs`**

After a module completes its translate→validate→repair cycle, before pushing to `module_outputs`:

1. Call `check_brace_balance(&rust_output)`
2. If depth != 0:
   - Log warning: "Module {name} has unbalanced braces (depth={depth}), re-translating..."
   - Re-translate once with prompt hint about truncation
   - If retry is still unbalanced → call `auto_close_braces()` + set `compiles = false`
3. Push module to `module_outputs` (always include, never exclude)

**4. Rule R4 in `apply_all_rules()` in `repair_rules.rs`**

Add `auto_close_braces()` as final rule (R4) in the mechanical rule pipeline. Acts as safety net: if any unbalanced braces survive to assembly, Phase 1 of hybrid repair closes them before Phase 2 surgical repair attempts per-function fixes.

### Files Modified

- `crates/noricum-tools/src/repair_rules.rs` — Add `check_brace_balance()`, `auto_close_braces()`, wire R4
- `crates/noricum-core/src/orchestrator.rs` — Add brace validation in `migrate_single_module()` post-repair

### Testing

- Unit tests for `check_brace_balance()`: balanced, unclosed, extra closes, braces in strings/comments
- Unit test for `auto_close_braces()`: appends correct number of braces
- Integration: verify R4 fires in `apply_all_rules()` pipeline

### Success Criteria

- Run 7 of miniz_zip.c produces an assembly with 0 "unclosed delimiter" errors
- No module is excluded from assembly due to truncation
- Brace-balance check adds < 1ms overhead per module (pure string scan)
