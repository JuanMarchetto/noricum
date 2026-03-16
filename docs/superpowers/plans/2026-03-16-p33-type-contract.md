# P33: Type Contract Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate a single validated Rust type contract before translating module bodies, eliminating cross-module type inconsistency in modular migration.

**Architecture:** New `generate_type_contract()` phase between `split_into_modules()` and module translation loop. Type contract is LLM-translated from C shared_context, validated via `check_rust_compiles()`, then injected as prefix in every module prompt. Assembly prepends it once; P27 dedup strips accidental redefinitions.

**Tech Stack:** Rust edition 2024, rig-rs 0.31 (LlmClient), noricum-tools compiler, noricum-agents translation, DeepSeek deepseek-chat.

**Spec:** `docs/superpowers/specs/2026-03-16-p33-type-contract-design.md`

---

## Chunk 1: Foundation — ModuleSplit return type + shared_context extraction

### Task 1: Change `split_into_modules()` return type to `ModuleSplit`

**Files:**
- Modify: `crates/noricum-tools/src/ast.rs:804-821` (CModule area + split_into_modules signature)
- Modify: `crates/noricum-core/src/orchestrator.rs:1953` (call site)
- Test: `crates/noricum-tools/src/ast.rs` (existing tests)

- [ ] **Step 1: Write test for ModuleSplit return**

In `crates/noricum-tools/src/ast.rs`, add test at end of `#[cfg(test)] mod tests`:

```rust
#[test]
fn test_split_into_modules_returns_shared_context() {
    let c_source = r#"
#include <stdio.h>
typedef struct { int x; int y; } Point;
enum Color { RED, GREEN, BLUE };
#define MAX_SIZE 100

void point_create(Point* p) { p->x = 0; p->y = 0; }
void point_move(Point* p, int dx, int dy) { p->x += dx; p->y += dy; }
void color_print(enum Color c) { printf("%d\n", c); }
void color_name(enum Color c) { printf("color\n"); }
int misc_helper() { return 42; }
"#;
    let split = split_into_modules(c_source, None);
    assert!(!split.shared_context.is_empty());
    assert!(split.shared_context.contains("Point"));
    assert!(split.shared_context.contains("Color"));
    assert!(split.shared_context.contains("MAX_SIZE"));
    assert!(!split.modules.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p noricum-tools test_split_into_modules_returns_shared_context`
Expected: FAIL — `split_into_modules` returns `Vec<CModule>`, not `ModuleSplit`

- [ ] **Step 3: Define `ModuleSplit` struct and change return type**

In `crates/noricum-tools/src/ast.rs`, add after `CModule` struct (after line 813):

```rust
/// Result of splitting C source into modules.
pub struct ModuleSplit {
    /// The individual modules.
    pub modules: Vec<CModule>,
    /// Shared context extracted from the C source (includes, typedefs, structs, enums, globals).
    pub shared_context: String,
}
```

Change `split_into_modules` signature at line 820-821 from:
```rust
pub fn split_into_modules(c_source: &str, target_module_loc: Option<usize>) -> Vec<CModule>
```
to:
```rust
pub fn split_into_modules(c_source: &str, target_module_loc: Option<usize>) -> ModuleSplit
```

Change the early return at ~line 824 (when `functions.len() < 4`) from:
```rust
return vec![CModule { ... }];
```
to:
```rust
return ModuleSplit {
    modules: vec![CModule { ... }],
    shared_context: String::new(),
};
```

Change the final return at end of function from:
```rust
modules
```
to:
```rust
ModuleSplit {
    modules,
    shared_context,
}
```

- [ ] **Step 4: Fix call site in orchestrator.rs**

In `crates/noricum-core/src/orchestrator.rs`, at line ~1953 where `split_into_modules` is called, change from:
```rust
let modules = noricum_tools::ast::split_into_modules(c_source, ...);
```
to:
```rust
let module_split = noricum_tools::ast::split_into_modules(c_source, ...);
let modules = module_split.modules;
let shared_context = module_split.shared_context;
```

Fix all other call sites of `split_into_modules` in the codebase (search with grep). Each should destructure `ModuleSplit`. Key call sites to update:
- ~7 existing tests in `ast.rs` that bind to `Vec<CModule>` — change to `.modules` (e.g., `split_into_modules(source, None).modules`)
- Any call sites in `orchestrator.rs` beyond the main one
- MCP server if it calls `split_into_modules` directly

- [ ] **Step 5: Run tests**

Run: `cargo test --workspace`
Expected: ALL pass (including the new test and all existing tests)

- [ ] **Step 6: Commit**

```bash
git add crates/noricum-tools/src/ast.rs crates/noricum-core/src/orchestrator.rs
git commit -m "refactor: split_into_modules returns ModuleSplit with shared_context"
```

---

## Chunk 2: Type contract generation + validation

### Task 2: Add `generate_type_contract()` function

**Files:**
- Create: `crates/noricum-core/src/type_contract.rs`
- Modify: `crates/noricum-core/src/lib.rs` (add module)
- Test: `crates/noricum-core/src/type_contract.rs` (inline tests)

- [ ] **Step 1: Write test for contract generation prompt building**

Create `crates/noricum-core/src/type_contract.rs`:

```rust
use crate::CoreError;
use noricum_agents::providers::{LlmClient, ProviderConfig, select_model};

/// P33: Type contract prompt template.
const TYPE_CONTRACT_PREAMBLE: &str = "You are an expert C-to-Rust type translator. \
You translate C type definitions to idiomatic Rust. You output ONLY valid Rust code.";

/// Build the user prompt for type contract generation.
fn build_type_contract_prompt(shared_context: &str, c_source_abbreviated: &str) -> String {
    format!(
        "Translate ALL type definitions from this C code to idiomatic Rust.\n\n\
         Rules:\n\
         - Convert typedef struct → pub struct with pub fields\n\
         - Convert enum → pub enum (use #[repr(i32)] if values have explicit integer assignments)\n\
         - Convert #define constants → pub const\n\
         - Convert typedef aliases → pub type\n\
         - Use idiomatic Rust types:\n\
           - void* → Box<dyn std::any::Any> (or concrete type if determinable)\n\
           - char* → String (owned) or &str (borrowed)\n\
           - T* + size_t len → Vec<T>\n\
           - Nullable pointers → Option<T>\n\
           - Function pointers → fn(...) -> ... or Option<fn(...) -> ...> for nullable\n\
         - Add #[derive(Debug, Clone)] where appropriate\n\
         - NO function bodies — only struct/enum/const/type definitions\n\
         - NO impl blocks\n\
         - NO standalone fn definitions\n\
         - Include brief doc comments for each type explaining its purpose\n\n\
         Output ONLY valid Rust code. No markdown fences. No explanations.\n\n\
         C type definitions:\n```c\n{shared_context}\n```\n\n\
         Full C source (for context on how types are used):\n```c\n{c_source_abbreviated}\n```",
        shared_context = shared_context,
        c_source_abbreviated = c_source_abbreviated,
    )
}

/// Build abbreviated C source: all function signatures + first N bodies that fit in budget.
fn abbreviate_c_for_context(c_source: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = c_source.lines().collect();
    if lines.len() <= max_lines {
        return c_source.to_string();
    }
    // Take first max_lines lines (includes shared_context + early functions)
    lines[..max_lines].join("\n")
}

/// Strip impl blocks from LLM output (keep only struct/enum/const/type definitions).
fn strip_impl_blocks(rust_source: &str) -> String {
    let lines: Vec<&str> = rust_source.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("impl ") {
            // Skip the entire impl block
            i = skip_braced_block_lines(&lines, i);
            continue;
        }
        // Also strip standalone fn definitions
        if (trimmed.starts_with("pub fn ") || trimmed.starts_with("fn "))
            && trimmed.contains('(')
        {
            if trimmed.contains('{') {
                i = skip_braced_block_lines(&lines, i);
            } else {
                i += 1;
            }
            continue;
        }
        result.push(lines[i]);
        i += 1;
    }
    result.join("\n")
}

/// Skip a braced block starting at `start`, returns index after closing brace.
fn skip_braced_block_lines(lines: &[&str], start: usize) -> usize {
    let mut depth = 0;
    let mut i = start;
    while i < lines.len() {
        for ch in lines[i].chars() {
            if ch == '{' { depth += 1; }
            if ch == '}' { depth -= 1; }
        }
        i += 1;
        if depth <= 0 && i > start + 1 {
            break;
        }
    }
    i
}

/// Strip markdown fences from LLM output.
fn strip_markdown_fences(source: &str) -> String {
    source
        .lines()
        .filter(|line| {
            let t = line.trim();
            !t.starts_with("```")
        })
        .collect::<Vec<&str>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_type_contract_prompt_contains_shared_context() {
        let prompt = build_type_contract_prompt("typedef struct { int x; } Foo;", "void bar() {}");
        assert!(prompt.contains("typedef struct { int x; } Foo;"));
        assert!(prompt.contains("void bar() {}"));
        assert!(prompt.contains("NO impl blocks"));
    }

    #[test]
    fn test_strip_impl_blocks() {
        let input = r#"pub struct Foo {
    pub x: i32,
}

impl Foo {
    pub fn new() -> Self {
        Foo { x: 0 }
    }
}

pub enum Bar {
    A,
    B,
}

impl Default for Bar {
    fn default() -> Self {
        Bar::A
    }
}

pub const MAX: i32 = 100;
"#;
        let result = strip_impl_blocks(input);
        assert!(result.contains("pub struct Foo"));
        assert!(result.contains("pub enum Bar"));
        assert!(result.contains("pub const MAX"));
        assert!(!result.contains("impl Foo"));
        assert!(!result.contains("impl Default"));
        assert!(!result.contains("fn new"));
    }

    #[test]
    fn test_strip_impl_blocks_preserves_empty() {
        assert_eq!(strip_impl_blocks(""), "");
    }

    #[test]
    fn test_strip_standalone_fn() {
        let input = "pub struct X { pub a: i32 }\npub fn helper() -> i32 {\n    42\n}\n";
        let result = strip_impl_blocks(input);
        assert!(result.contains("pub struct X"));
        assert!(!result.contains("pub fn helper"));
    }

    #[test]
    fn test_abbreviate_under_budget() {
        let source = "line1\nline2\nline3";
        assert_eq!(abbreviate_c_for_context(source, 100), source);
    }

    #[test]
    fn test_abbreviate_over_budget() {
        let source = "line1\nline2\nline3\nline4\nline5";
        let result = abbreviate_c_for_context(source, 3);
        assert_eq!(result, "line1\nline2\nline3");
    }

    #[test]
    fn test_strip_markdown_fences() {
        let input = "```rust\npub struct Foo {}\n```\n";
        let result = strip_markdown_fences(input);
        assert!(!result.contains("```"));
        assert!(result.contains("pub struct Foo"));
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

In `crates/noricum-core/src/lib.rs`, add:
```rust
pub mod type_contract;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p noricum-core test_build_type_contract_prompt test_strip_impl test_strip_standalone test_abbreviate test_strip_markdown`
Expected: ALL 7 tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/noricum-core/src/type_contract.rs crates/noricum-core/src/lib.rs
git commit -m "feat: P33 type_contract module with prompt builder, impl stripper, tests"
```

### Task 3: Add async `generate_type_contract()` with LLM + validation

**Files:**
- Modify: `crates/noricum-core/src/type_contract.rs`

- [ ] **Step 1: Write integration test**

Add to tests in `type_contract.rs`:

```rust
#[test]
fn test_validate_type_contract_compiles() {
    // Simulate a valid type contract
    let contract = "pub struct Foo { pub x: i32 }\npub enum Bar { A, B }\npub const MAX: i32 = 100;\n";
    let result = noricum_tools::compiler::check_rust_compiles(contract).unwrap();
    assert!(result.success, "Valid type contract should compile: {}", result.stderr);
}

#[test]
fn test_validate_type_contract_rejects_invalid() {
    let contract = "pub struct Foo { pub x: UnknownType }\n";
    let result = noricum_tools::compiler::check_rust_compiles(contract).unwrap();
    assert!(!result.success);
}
```

- [ ] **Step 2: Run tests to verify they pass (validation logic uses existing compiler)**

Run: `cargo test -p noricum-core test_validate_type_contract`
Expected: PASS

- [ ] **Step 3: Implement `generate_type_contract()`**

Add to `type_contract.rs`:

```rust
/// Maximum retries for type contract compilation.
const MAX_CONTRACT_RETRIES: usize = 3;

/// Maximum lines for abbreviated C source context.
const CONTRACT_C_CONTEXT_MAX_LINES: usize = 2000;

/// P33: Generate a validated Rust type contract from C shared context.
///
/// 1. Build prompt from shared_context + abbreviated C source
/// 2. Call LLM to translate types
/// 3. Post-process: strip fences, strip impl blocks
/// 4. Validate with check_rust_compiles()
/// 5. Retry up to MAX_CONTRACT_RETRIES times on failure
/// 6. Return None if all retries fail (graceful degradation)
pub async fn generate_type_contract(
    client: &LlmClient,
    provider_config: &ProviderConfig,
    shared_context: &str,
    c_source: &str,
    artifacts: Option<&crate::artifacts::ArtifactStore>,
) -> Result<Option<String>, CoreError> {
    if shared_context.trim().is_empty() {
        tracing::info!("P33: empty shared_context, skipping type contract");
        return Ok(None);
    }

    let c_abbreviated = abbreviate_c_for_context(c_source, CONTRACT_C_CONTEXT_MAX_LINES);
    let user_prompt = build_type_contract_prompt(shared_context, &c_abbreviated);

    // Use fast model (Easy difficulty) — type translation is a focused task
    let model_sel = select_model(provider_config, noricum_ir::Difficulty::Easy, "type_contract")
        .map_err(|e| CoreError::Orchestration(format!("P33 model selection: {e}")))?;
    let model = &model_sel.model;
    let mut last_errors = String::new();

    for attempt in 0..MAX_CONTRACT_RETRIES {
        let prompt = if attempt == 0 {
            user_prompt.clone()
        } else {
            format!(
                "{}\n\nPREVIOUS ATTEMPT FAILED TO COMPILE. Fix these errors:\n{}\n\nOutput ONLY corrected Rust code.",
                user_prompt, last_errors
            )
        };

        let raw = match client
            .run_prompt(model, TYPE_CONTRACT_PREAMBLE, 0.2, 8192, &prompt)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("P33: LLM call failed attempt {}: {}", attempt + 1, e);
                continue;
            }
        };

        // Post-process
        let cleaned = strip_markdown_fences(&raw);
        let contract = strip_impl_blocks(&cleaned);

        if contract.trim().is_empty() {
            tracing::warn!("P33: empty contract after post-processing, attempt {}", attempt + 1);
            continue;
        }

        // Validate compilation
        match noricum_tools::compiler::check_rust_compiles(&contract) {
            Ok(result) if result.success => {
                tracing::info!(
                    "P33: type contract compiled (attempt {}, {} lines)",
                    attempt + 1,
                    contract.lines().count()
                );
                // Save artifact
                if let Some(store) = artifacts {
                    let _ = store.save_type_contract(&contract);
                }
                return Ok(Some(contract));
            }
            Ok(result) => {
                tracing::warn!(
                    "P33: contract failed to compile (attempt {}): {}",
                    attempt + 1,
                    &result.stderr[..result.stderr.len().min(500)]
                );
                // Try mechanical fixes via rule engine before LLM retry
                let parsed_errors = noricum_tools::repair_rules::parse_rustc_errors(&result.stderr);
                let fixed = noricum_tools::repair_rules::apply_all_rules(&contract, &parsed_errors);
                if fixed != contract {
                    match noricum_tools::compiler::check_rust_compiles(&fixed) {
                        Ok(r2) if r2.success => {
                            tracing::info!("P33: rule engine fixed contract (attempt {})", attempt + 1);
                            if let Some(store) = artifacts {
                                let _ = store.save_type_contract(&fixed);
                            }
                            return Ok(Some(fixed));
                        }
                        _ => {}
                    }
                }
                last_errors = result.stderr;
            }
            Err(e) => {
                tracing::warn!("P33: compiler error attempt {}: {}", attempt + 1, e);
                continue;
            }
        }
    }

    tracing::warn!("P33: type contract failed after {} retries, proceeding without contract", MAX_CONTRACT_RETRIES);
    Ok(None)
}
```

Add `save_type_contract()` to `ArtifactStore` in `crates/noricum-core/src/artifacts.rs` (following the existing `save_c2rust` pattern at line 117):

```rust
/// Save the P33 type contract.
pub fn save_type_contract(&self, rust_source: &str) -> io::Result<()> {
    std::fs::write(self.run_dir.join("02-type-contract.rs"), rust_source)
}
```

- [ ] **Step 4: Run all workspace tests**

Run: `cargo test --workspace`
Expected: ALL pass

- [ ] **Step 5: Commit**

```bash
git add crates/noricum-core/src/type_contract.rs crates/noricum-core/src/artifacts.rs
git commit -m "feat: P33 generate_type_contract with LLM translation + compilation validation"
```

---

## Chunk 3: Pipeline integration

### Task 4: Wire type contract into `migrate_file_modular()`

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs:1943-2204`

- [ ] **Step 1: Call `generate_type_contract()` after module split**

In `migrate_file_modular()`, after `split_into_modules()` call (~line 1953) and before the module loop, add:

```rust
// P33: Generate type contract
let type_contract = crate::type_contract::generate_type_contract(
    client,
    provider_config,
    &shared_context,
    c_source,
    artifacts.as_ref(),
).await?;

if let Some(ref tc) = type_contract {
    tracing::info!("P33: type contract generated ({} lines)", tc.lines().count());
} else {
    tracing::info!("P33: no type contract (will use per-module type discovery)");
}
```

- [ ] **Step 2: Pass type contract to `migrate_single_module()`**

Add parameter to `migrate_single_module()` signature (at ~line 2212):
```rust
type_contract: Option<&str>,
```

Update all call sites in `migrate_file_modular()` to pass `type_contract.as_deref()`.

- [ ] **Step 3: When type contract active, accumulate only signatures (not types)**

In `migrate_file_modular()`, modify the context accumulation block (~lines 2138-2148):

```rust
// P33: When type contract active, only accumulate function signatures (types already in contract)
if type_contract.is_none() {
    let type_defs = noricum_tools::ast::extract_rust_type_definitions(rust_output);
    if !type_defs.is_empty() {
        accumulated_rust_context.push_str(&type_defs.join("\n\n"));
        accumulated_rust_context.push('\n');
    }
}
let sigs = noricum_tools::ast::extract_rust_signatures(rust_output);
if !sigs.is_empty() {
    accumulated_rust_context.push_str(&sigs.join("\n"));
    accumulated_rust_context.push('\n');
}
```

- [ ] **Step 4: Run workspace tests**

Run: `cargo test --workspace`
Expected: ALL pass

- [ ] **Step 5: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs
git commit -m "feat: P33 wire type contract into migrate_file_modular"
```

### Task 5: Inject type contract into module translation prompt

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs:2300-2350` (migrate_single_module context injection)

- [ ] **Step 1: Modify prompt to include type contract**

In `migrate_single_module()`, modify the `augmented_c` construction (~lines 2335-2349). Replace the existing P27 context injection with:

```rust
let augmented_c = if let Some(tc) = type_contract {
    // P33: Type contract provides canonical types. accumulated_rust_context has only function sigs.
    format!(
        "/* P33 TYPE CONTRACT: The following Rust types are ALREADY DEFINED and MUST be used exactly as-is.\n\
         Do NOT redefine ANY struct, enum, const, or type alias below. They are final.\n\
         Only write functions and impl blocks that USE these types.\n\n\
         ```rust\n{}\n```\n*/\n\n\
         {}\n\n{}",
        tc,
        if accumulated_rust_context.is_empty() {
            String::new()
        } else {
            format!(
                "/* Already migrated function signatures (use these, do not redefine):\n{}\n*/",
                accumulated_rust_context
            )
        },
        module.source
    )
} else if !accumulated_rust_context.is_empty() {
    // Existing P27 behavior (no type contract)
    format!(
        "/* MIGRATION CONTEXT: The following Rust types and functions have already been migrated \
from earlier modules in this same file.\n\
\n\
CRITICAL: Do NOT redefine any struct, enum, const, or type alias that appears below. \
Use them directly — they are already defined and available in scope. \
Only define NEW types that don't exist yet. If you need a type that's listed below, \
just use it (e.g., `ZipArchive`, `ZipError`). Do NOT create your own version.\n\
\n{}\n*/\n\n{}",
        accumulated_rust_context, module.source
    )
} else {
    module.source.clone()
};
```

- [ ] **Step 2: Run workspace tests**

Run: `cargo test --workspace`
Expected: ALL pass

- [ ] **Step 3: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs
git commit -m "feat: P33 inject type contract into module translation prompt"
```

### Task 6: Prepend type contract in assembly

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs:2912` (assemble_module_outputs)

- [ ] **Step 1: Write test for assembly with type contract**

Add test in orchestrator.rs tests:

```rust
#[test]
fn test_assemble_with_type_contract() {
    let contract = "pub struct Foo { pub x: i32 }\npub enum Bar { A, B }\n";
    let modules = vec![
        ("mod1".to_string(), "pub fn create_foo() -> Foo { Foo { x: 1 } }".to_string(), true),
        ("mod2".to_string(), "pub fn get_bar() -> Bar { Bar::A }".to_string(), true),
    ];
    let result = assemble_module_outputs(&modules, Some(contract));
    // Contract should appear first
    let contract_pos = result.find("pub struct Foo").unwrap();
    let fn_pos = result.find("pub fn create_foo").unwrap();
    assert!(contract_pos < fn_pos, "Type contract should precede module code");
    // Should contain both module functions
    assert!(result.contains("pub fn get_bar"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p noricum-core test_assemble_with_type_contract`
Expected: FAIL — `assemble_module_outputs` doesn't accept `type_contract` param yet

- [ ] **Step 3: Modify `assemble_module_outputs()` to accept optional type contract**

Change the existing `assemble_module_outputs` signature from:
```rust
fn assemble_module_outputs(modules: &[(String, String, bool)]) -> String
```
to:
```rust
fn assemble_module_outputs(modules: &[(String, String, bool)], type_contract: Option<&str>) -> String
```

Inside the function, at the beginning where `defined_types` HashSet is initialized, seed it with contract type names:
```rust
let mut defined_types: std::collections::HashSet<String> = std::collections::HashSet::new();

// P33: If type contract provided, prepend it and seed defined_types
// so P27 dedup strips any module redefinitions of contract types
let contract_block = if let Some(contract) = type_contract {
    // Extract type names from contract to seed dedup
    for cline in contract.lines() {
        let trimmed = cline.trim();
        if let Some(type_name) = extract_definition_name(trimmed) {
            defined_types.insert(type_name);
        }
    }
    format!("// === P33: Type Contract (shared types) ===\n{}\n// === End Type Contract ===\n\n", contract)
} else {
    String::new()
};
```

Then at the end where the final string is assembled, prepend `contract_block`:
```rust
format!("{}{}\n{}", contract_block, merged, code_parts.join("\n\n"))
```

Update all call sites of `assemble_module_outputs` to pass the type contract. The test `test_assemble_with_type_contract` should call `assemble_module_outputs(&modules, Some(contract))` directly.

- [ ] **Step 4: Run tests**

Run: `cargo test --workspace`
Expected: ALL pass

- [ ] **Step 5: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs
git commit -m "feat: P33 prepend type contract in assembly output"
```

---

## Chunk 4: End-to-end validation

### Task 7: Integration test with miniz shared_context

**Files:**
- Test: `crates/noricum-core/src/type_contract.rs`

- [ ] **Step 1: Write test using real miniz shared_context**

```rust
#[test]
fn test_strip_impl_blocks_preserves_complex_types() {
    // Simulate miniz-like output with nested structs
    let input = r#"
/// Zip archive state.
pub struct ZipArchive {
    pub archive_size: u64,
    pub total_files: u32,
    pub zip_mode: ZipMode,
    pub state: Option<Box<ZipInternalState>>,
}

/// Zip operation mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZipMode {
    Reading,
    Writing,
    WritingHasBeenFinalized,
}

/// Zip error codes.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(i32)]
pub enum ZipError {
    NoError = 0,
    NotAnArchive = 1,
    FailedFindingCentralDir = 2,
    CrcCheckFailed = 3,
}

impl ZipArchive {
    pub fn new() -> Self {
        ZipArchive {
            archive_size: 0,
            total_files: 0,
            zip_mode: ZipMode::Reading,
            state: None,
        }
    }
}

pub const MZ_ZIP_MAX_IO_BUF_SIZE: usize = 64 * 1024;
pub type MzUint = u32;
"#;
    let result = strip_impl_blocks(input);
    assert!(result.contains("pub struct ZipArchive"));
    assert!(result.contains("pub enum ZipMode"));
    assert!(result.contains("pub enum ZipError"));
    assert!(result.contains("pub const MZ_ZIP_MAX_IO_BUF_SIZE"));
    assert!(result.contains("pub type MzUint"));
    assert!(!result.contains("impl ZipArchive"));
    assert!(!result.contains("fn new"));

    // Verify it compiles
    let compile_result = noricum_tools::compiler::check_rust_compiles(&result).unwrap();
    assert!(compile_result.success, "Stripped contract should compile: {}", compile_result.stderr);
}
```

- [ ] **Step 2: Run test**

Run: `cargo test -p noricum-core test_strip_impl_blocks_preserves_complex_types`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/noricum-core/src/type_contract.rs
git commit -m "test: P33 complex type contract stripping + compilation validation"
```

### Task 8: Verify no regression on full test suite

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: ALL 435+ tests pass, 0 failures

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace`
Expected: 0 warnings

- [ ] **Step 3: Final commit if any cleanup needed**

```bash
git add -A && git commit -m "chore: P33 cleanup and final verification"
```

---

## Summary

| Task | What | Files | Tests |
|------|------|-------|-------|
| 1 | `ModuleSplit` return type | ast.rs, orchestrator.rs | 1 new |
| 2 | `type_contract.rs` module (prompt, strip, abbreviate) | type_contract.rs, lib.rs | 7 new |
| 3 | `generate_type_contract()` async with LLM + validation | type_contract.rs, artifacts.rs | 2 new |
| 4 | Wire into `migrate_file_modular()` | orchestrator.rs | 0 (integration) |
| 5 | Inject into module translation prompt | orchestrator.rs | 0 (integration) |
| 6 | Prepend in assembly | orchestrator.rs | 1 new |
| 7 | Integration test with complex types | type_contract.rs | 1 new |
| 8 | Full regression check | — | ALL existing |

**Total: ~8 tasks, ~12 new tests, 6 commits**
