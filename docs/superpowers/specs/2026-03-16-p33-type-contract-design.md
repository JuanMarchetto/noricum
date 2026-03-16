# P33: Type Contract — Design Spec

## Problem

In modular migration of large C files (e.g., miniz_zip.c, 4895 LOC), each module is translated independently by the LLM. Each module invents its own Rust representation of shared C types (structs, enums, typedefs). When assembled, these incompatible definitions cause hundreds of compilation errors.

Evidence from 9 runs of miniz_zip.c:
- `ZipArchive` defined differently across 11/17 modules (Run 3) or 8/10 modules (Runs 5-9)
- Field types diverge: `*mut ()` vs `Box<dyn ReadWrite>` vs `Rc<RefCell<dyn Any>>` for the same C field
- P27 dedup fixes duplicate *definitions* but cannot fix *usage* conflicts (code assumes wrong field shapes)
- Run 6 achieved 1 assembly error by accident (compatible warm-start seeds); Runs 8-9 had 500+ errors

## Solution: Types-First Translation

Generate a single, validated Rust type contract before translating any module body. Inject this contract into every module translation prompt so all modules share identical type definitions.

## Architecture

### New Phase in Pipeline

```
split_into_modules()
        │
        ▼
 ┌──────────────────────────────┐
 │ P33: generate_type_contract() │  ← NEW
 │ 1. Extract shared_context     │
 │ 2. LLM: translate types only  │
 │ 3. cargo check validation     │
 │ 4. Return type_contract String │
 └──────────────┬───────────────┘
                ▼
  migrate_single_module() per module
   - type_contract prepended to output
   - prompt: "types defined, only write functions"
                │
                ▼
  assemble_module_outputs()
   - type_contract as first block (once)
   - P27 dedup strips accidental redefinitions
```

### Components

#### 1. `generate_type_contract()` — new in `orchestrator.rs`

```rust
async fn generate_type_contract(
    llm: &LlmClient,
    shared_context: &str,
    c_source: &str,
    artifacts: Option<&ArtifactStore>,
) -> Result<String>
```

- **Input**: Full C source + shared_context (returned from `split_into_modules()` as `ModuleSplit`)
- **Step 1**: Receive shared_context from caller (extracted by `split_into_modules()`)
- **Step 2**: Call LLM with focused prompt (see Prompt Design below)
- **Step 3**: Post-process: strip any `impl` blocks from output (keep only struct/enum/const/type — see Issue 4)
- **Step 4**: Validate with `check_rust_compiles()` from `noricum-tools/src/compiler.rs` (edition 2024)
- **Step 5**: If compilation fails, apply `apply_all_rules()` first (free mechanical fixes), then retry LLM with compiler errors (up to 3 retries total)
- **Step 6**: Save to artifacts as `02-type-contract.rs`
- **Output**: Compiled Rust type definitions as String

#### 2. Modifications to `migrate_single_module()`

New parameter: `type_contract: &str`

Changes to translation prompt:
- Separate section: "The following Rust types are already defined and available. Do NOT redefine any struct, enum, const, or type alias below. Only write functions and impl blocks."
- The type contract is NOT embedded as a C comment — it's a distinct Rust section in the prompt
- When a type contract is active, `accumulated_rust_context` accumulates ONLY function signatures (not type definitions), since types are already covered by the contract. This prevents duplicate/conflicting type info in later module prompts (Issue 2 fix)

#### 3. Modifications to `assemble_module_outputs()`

- Type contract inserted as first block before any module code
- P27 dedup continues as safety net (strips any accidental redefinitions from modules)
- No changes to dedup logic itself

#### 4. Artifact persistence

- Saved as `02-type-contract.rs` in artifact directory
- Warm-start can load a pre-existing type contract. Skip LLM call if contract exists and `shared_context` SHA-256 hash matches (`contract_source_hash` field in manifest). Hash is of `shared_context` only (not full C source), so function body changes don't invalidate the contract

### Prompt Design

```
Translate ALL type definitions from this C code to idiomatic Rust.

Rules:
- Convert typedef struct → pub struct with pub fields
- Convert enum → pub enum (use #[repr(i32)] if values have explicit integer assignments)
- Convert #define constants → pub const
- Convert typedef aliases → pub type
- Use idiomatic Rust types:
  - void* → Box<dyn std::any::Any> (or concrete type if determinable)
  - char* → String (owned) or &str (borrowed)
  - T* + size_t len → Vec<T>
  - Nullable pointers → Option<T>
  - Function pointers → fn(...) -> ... or Option<fn(...) -> ...> for nullable
- Add #[derive(Debug, Clone)] where appropriate
- NO function bodies — only struct/enum/const/type definitions
- Include brief doc comments for each type explaining its purpose

Output ONLY valid Rust code. No markdown fences. No explanations.

C type definitions:
{shared_context}

Full C source (for context on how types are used):
{c_source_abbreviated}
```

Note: `c_source_abbreviated` is a truncated version of the full C source (~2000 LOC max) to give the LLM usage context without token overflow.

### Validation

The type contract is validated before use:

1. Write contract to temp `.rs` file
2. Validate using `check_rust_compiles()` from `noricum-tools/src/compiler.rs` (uses `--edition=2024`, consistent with project)
3. If fails: apply `apply_all_rules()` for free mechanical fixes first
4. If still fails: retry LLM with compiler errors in prompt (up to 3 total retries)
5. If all retries fail: fall back to current behavior (no contract, each module defines own types)
6. Log validation result via tracing

### Integration Points

- **`migrate_file_modular()`**: Call `generate_type_contract()` after `split_into_modules()`, before module loop
- **`migrate_single_module()`**: Accept `type_contract` parameter, inject into prompt
- **`assemble_module_outputs()`**: Accept optional `type_contract` parameter, prepend as first block
- **Warm-start**: Save/load contract from artifacts; skip regeneration if hash matches
- **Artifact store**: New `save_type_contract()` method

### `split_into_modules()` Return Type Change

Currently `shared_context` is a local variable inside `split_into_modules()`. Modify the return type to expose it:

```rust
pub struct ModuleSplit {
    pub modules: Vec<CModule>,
    pub shared_context: String,
}

pub fn split_into_modules(c_source: &str, max_module_loc: usize) -> ModuleSplit
```

This avoids duplicating the extraction logic and gives `generate_type_contract()` direct access to the shared context.

### Post-Processing: Strip `impl` Blocks

After LLM generates the type contract, strip ALL `impl` blocks before validation. Rationale:
- `impl` blocks with method bodies will conflict with module code that implements the same methods
- Trait impls (`impl Default`, `impl Display`) are nice-to-have but not essential for type consistency
- Modules can define their own `impl` blocks freely without conflict
- Simpler is safer — the contract's job is only to define **data shapes**

Implementation: reuse `skip_braced_block()` from `orchestrator.rs` to detect and remove `impl ... { }` blocks.

### `c_source_abbreviated` Truncation Strategy

To give the LLM usage context without token overflow, build the abbreviated source as:
1. Full `shared_context` (all type definitions — already in prompt separately)
2. All function signatures (one line each, extracted via regex `^\w[\w\s\*]+\w+\s*\(`)
3. Bodies of first N functions that fit within ~2000 LOC budget

This gives the LLM both types and usage patterns across the entire file.

### Edge Cases

- **Empty shared_context**: Skip P33, fall back to current behavior
- **Contract too large** (>1000 LOC): Truncate to structs/enums only (skip const arrays)
- **LLM generates function bodies**: Post-processing strips `impl` blocks and standalone `fn` definitions
- **Validation failure after 3 retries**: Log warning, proceed without contract (graceful degradation)

### Provider

- DeepSeek (`deepseek-chat`) for all calls including contract generation
- Contract generation is a focused task (~300 LOC input) — DeepSeek handles this well
- Total estimated cost: ~$1-2 per run (same as before + ~$0.10 for contract)

### Success Criteria

- miniz_zip.c assembly has <10 compilation errors (vs 500+ in Runs 8-9)
- All modules use identical type definitions for shared structs (ZipArchive, ZipError, etc.)
- Type contract compiles independently before module translation begins
- No regression on existing test suite (435+ tests)

### Non-Goals

- Does not change module splitting logic
- Does not change P30 hybrid repair (still available as fallback)
- Does not change provider routing or difficulty classification
- Does not attempt to generate function signatures (P11 already handles that)
