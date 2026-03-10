# P30: Hybrid Repair Engine Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the whole-file assembly repair loop with a three-phase hybrid: mechanical rule engine → surgical per-function LLM repair → legacy fallback.

**Architecture:** Phase 1 applies 5 deterministic regex-based rules for known ownership patterns (0 LLM cost). Phase 2 extracts individual failing functions with their type context and sends ~100 LOC focused repair requests. Phase 3 falls back to the existing whole-file repair loop (max 3 iterations). Only activated for assembled outputs (>MODULAR_FILE_LOC lines).

**Tech Stack:** Rust 2024, regex, noricum-tools (repair_rules), noricum-core (surgical_repair, orchestrator)

**Spec:** `docs/superpowers/specs/2026-03-10-hybrid-repair-engine-design.md`

**Test fixture:** `.noricum-artifacts/miniz_zip-20260310-150830/05-repair/iter-05.rs` — Run 3 assembly with 5 mechanical errors that were manually fixed to compile.

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/noricum-tools/src/repair_rules.rs` | Create | Error parser + 5 mechanical repair rules |
| `crates/noricum-tools/src/lib.rs` | Modify | Add `pub mod repair_rules` |
| `crates/noricum-core/src/surgical_repair.rs` | Create | Function extraction, context gathering, LLM splice |
| `crates/noricum-core/src/lib.rs` | Modify | Add `pub mod surgical_repair` |
| `crates/noricum-core/src/orchestrator.rs` | Modify | `hybrid_repair()` function, Stage 7 integration |

---

## Chunk 1: Rule Engine (Phase 1)

### Task 1: CompilerError parser

**Files:**
- Create: `crates/noricum-tools/src/repair_rules.rs`
- Modify: `crates/noricum-tools/src/lib.rs`

- [ ] **Step 1: Write the test for error parsing**

Add to `crates/noricum-tools/src/repair_rules.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rustc_errors_basic() {
        let stderr = r#"error[E0599]: the method `resize` exists for mutable reference `&mut Vec<T>`, but its trait bounds were not satisfied
    --> check.rs:1070:11
     |
1070 |     array.resize(new_size, T::default());
     |           ^^^^^^

error[E0499]: cannot borrow `*zip` as mutable more than once at a time
    --> check.rs:1199:30
     |
1199 |         mz_zip_array_clear_6(zip, &mut state.m_sorted_central_dir_offsets);
     |                              ^^^
"#;
        let errors = parse_rustc_errors(stderr);
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].code, "E0599");
        assert_eq!(errors[0].line, 1070);
        assert!(errors[0].message.contains("trait bounds were not satisfied"));
        assert_eq!(errors[1].code, "E0499");
        assert_eq!(errors[1].line, 1199);
    }

    #[test]
    fn test_parse_rustc_errors_empty() {
        let errors = parse_rustc_errors("warning: unused variable\n");
        assert!(errors.is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p noricum-tools -- test_parse_rustc_errors`
Expected: FAIL — `parse_rustc_errors` not found

- [ ] **Step 3: Write the CompilerError struct and parser**

Add to `crates/noricum-tools/src/repair_rules.rs`:

```rust
//! P30: Mechanical repair rules for known Rust ownership patterns.
//!
//! These rules fix compilation errors that LLMs consistently fail to resolve:
//! clone bounds on generics, double mutable borrows, use-after-move with
//! Option<&mut T>, etc. Applied deterministically with zero LLM cost.

use regex::Regex;
use std::sync::LazyLock;

/// A parsed compiler error from rustc output.
#[derive(Debug, Clone)]
pub struct CompilerError {
    /// Error code like "E0499"
    pub code: String,
    /// 1-indexed line number
    pub line: usize,
    /// Full error message (first line)
    pub message: String,
}

static ERROR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"error\[(?P<code>E\d+)\]: (?P<msg>.+)\n\s+--> .+:(?P<line>\d+):\d+").unwrap()
});

/// Parse rustc stderr into structured compiler errors.
pub fn parse_rustc_errors(stderr: &str) -> Vec<CompilerError> {
    ERROR_RE
        .captures_iter(stderr)
        .map(|cap| CompilerError {
            code: cap["code"].to_string(),
            line: cap["line"].parse().unwrap_or(0),
            message: cap["msg"].to_string(),
        })
        .collect()
}
```

Add to `crates/noricum-tools/src/lib.rs`:

```rust
pub mod repair_rules;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p noricum-tools -- test_parse_rustc_errors`
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/noricum-tools/src/repair_rules.rs crates/noricum-tools/src/lib.rs
git commit -m "feat: P30 CompilerError parser for repair rules"
```

---

### Task 2: Rule R1 — Clone bounds on generics

The most common assembly error. When a generic function calls `.resize()` or `.clone()`, rustc requires `T: Clone` but the LLM forgets the bound.

**Error pattern:** `error[E0599]` with message containing "trait bounds were not satisfied" and the suggestion `where T: Clone`.

**Files:**
- Modify: `crates/noricum-tools/src/repair_rules.rs`

- [ ] **Step 1: Write the test**

```rust
#[test]
fn test_rule_clone_bounds() {
    let source = r#"
fn mz_zip_array_resize<T: Default>(array: &mut Vec<T>, new_size: usize) -> bool {
    array.resize(new_size, T::default());
    true
}
"#;
    let errors = vec![CompilerError {
        code: "E0599".into(),
        line: 3,
        message: "the method `resize` exists for mutable reference `&mut Vec<T>`, but its trait bounds were not satisfied".into(),
    }];
    let result = apply_all_rules(source, &errors);
    assert!(result.contains("T: Default + Clone"));
    assert!(result.contains("array.resize"));
}
```

- [ ] **Step 2: Run test — expected FAIL**

Run: `cargo test -p noricum-tools -- test_rule_clone_bounds`
Expected: FAIL — `apply_all_rules` not found

- [ ] **Step 3: Implement Rule R1 and apply_all_rules**

```rust
static GENERIC_FN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"fn\s+\w+<([^>]+)>\s*\(").unwrap()
});

/// R1: Add `+ Clone` to generic type bounds when rustc says trait bounds not satisfied.
fn rule_clone_bounds(source: &str, errors: &[CompilerError]) -> String {
    let needs_clone: Vec<usize> = errors
        .iter()
        .filter(|e| e.code == "E0599" && e.message.contains("trait bounds were not satisfied"))
        .map(|e| e.line)
        .collect();

    if needs_clone.is_empty() {
        return source.to_string();
    }

    let lines: Vec<&str> = source.lines().collect();
    let mut result = String::with_capacity(source.len() + 100);

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;
        // Find the fn declaration containing this error line
        let in_error_fn = needs_clone.iter().any(|&err_line| {
            // Search backwards from error line to find fn declaration
            (0..err_line).rev().any(|l| l == line_num && GENERIC_FN_RE.is_match(line))
        });

        if in_error_fn {
            // Add Clone to existing bounds: `T: Default` → `T: Default + Clone`
            if let Some(caps) = GENERIC_FN_RE.captures(line) {
                let bounds = &caps[1];
                if !bounds.contains("Clone") {
                    // Find the bound section and add Clone
                    let new_line = if bounds.contains(':') {
                        // Has existing bounds: `T: Default` → `T: Default + Clone`
                        line.replacen(bounds, &format!("{bounds} + Clone"), 1)
                    } else {
                        // No bounds: `T` → `T: Clone`
                        line.replacen(bounds, &format!("{bounds}: Clone"), 1)
                    };
                    result.push_str(&new_line);
                    result.push('\n');
                    continue;
                }
            }
        }
        result.push_str(line);
        result.push('\n');
    }
    result
}

/// Apply all mechanical repair rules in sequence.
/// Returns the modified source. If no rules matched, returns source unchanged.
pub fn apply_all_rules(source: &str, errors: &[CompilerError]) -> String {
    let mut current = source.to_string();
    current = rule_clone_bounds(&current, errors);
    // Additional rules will be added here
    current
}
```

- [ ] **Step 4: Run test — expected PASS**

Run: `cargo test -p noricum-tools -- test_rule_clone_bounds`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/noricum-tools/src/repair_rules.rs
git commit -m "feat: P30 Rule R1 — add Clone bounds to generics"
```

---

### Task 3: Rule R2 — Duplicate function dedup

Assembly outputs often have duplicate function definitions from multiple modules. This is the most common error type (E0428).

**Error pattern:** `error[E0428]` — "the name X is defined multiple times"

**Files:**
- Modify: `crates/noricum-tools/src/repair_rules.rs`

- [ ] **Step 1: Write the test**

```rust
#[test]
fn test_rule_dedup_functions() {
    let source = r#"fn helper(x: i32) -> i32 {
    x + 1
}

fn other() -> i32 { 42 }

fn helper(x: i32) -> i32 {
    x * 2
}
"#;
    let errors = vec![CompilerError {
        code: "E0428".into(),
        line: 7,
        message: "the name `helper` is defined multiple times".into(),
    }];
    let result = apply_all_rules(source, &errors);
    // Should keep first definition, remove duplicate
    assert_eq!(result.matches("fn helper").count(), 1);
    assert!(result.contains("x + 1")); // keeps first
    assert!(result.contains("fn other")); // preserves other functions
}
```

- [ ] **Step 2: Run test — expected FAIL**

Run: `cargo test -p noricum-tools -- test_rule_dedup_functions`

- [ ] **Step 3: Implement Rule R2**

```rust
static DEDUP_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"the name `(\w+)` is defined multiple times").unwrap()
});

/// R2: Remove duplicate function definitions, keeping the first occurrence.
fn rule_dedup_functions(source: &str, errors: &[CompilerError]) -> String {
    let dup_names: Vec<String> = errors
        .iter()
        .filter(|e| e.code == "E0428")
        .filter_map(|e| {
            DEDUP_NAME_RE
                .captures(&e.message)
                .map(|c| c[1].to_string())
        })
        .collect();

    if dup_names.is_empty() {
        return source.to_string();
    }

    let lines: Vec<&str> = source.lines().collect();
    let mut result = String::with_capacity(source.len());
    let mut seen_fns: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut skip_until_close = false;
    let mut brace_depth: i32 = 0;

    for line in &lines {
        if skip_until_close {
            for ch in line.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            if brace_depth <= 0 {
                skip_until_close = false;
            }
            continue;
        }

        // Check if this line starts a function definition for a duplicate name
        let is_dup = dup_names.iter().any(|name| {
            let pattern = format!("fn {name}");
            line.contains(&pattern) && !seen_fns.insert(name.clone())
        });

        // Also track first occurrences
        if !is_dup {
            for name in &dup_names {
                let pattern = format!("fn {name}");
                if line.contains(&pattern) {
                    seen_fns.insert(name.clone());
                }
            }
        }

        if is_dup {
            // Skip this function body
            skip_until_close = true;
            brace_depth = 0;
            for ch in line.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            if brace_depth <= 0 {
                skip_until_close = false;
            }
            continue;
        }

        result.push_str(line);
        result.push('\n');
    }
    result
}
```

Update `apply_all_rules`:

```rust
pub fn apply_all_rules(source: &str, errors: &[CompilerError]) -> String {
    let mut current = source.to_string();
    current = rule_dedup_functions(&current, errors);
    current = rule_clone_bounds(&current, errors);
    current
}
```

- [ ] **Step 4: Run test — expected PASS**

Run: `cargo test -p noricum-tools -- test_rule_dedup`

- [ ] **Step 5: Commit**

```bash
git add crates/noricum-tools/src/repair_rules.rs
git commit -m "feat: P30 Rule R2 — deduplicate function definitions"
```

---

### Task 4: Rule R3 — mut binding for Option<&mut T>

When code does `if let Some(v) = opt_ref_mut`, rustc requires `mut` on the binding and `ref mut` in the pattern to avoid moving the `&mut` reference.

**Error pattern:** `error[E0382]` with message "use of moved value" and note about `&mut` not implementing `Copy`.

**Files:**
- Modify: `crates/noricum-tools/src/repair_rules.rs`

- [ ] **Step 1: Write the test**

```rust
#[test]
fn test_rule_mut_option_ref() {
    let source = r#"fn write_data(p_index: Option<&mut u32>, data: &[u8]) -> bool {
    if let Some(index) = p_index {
        *index = 42;
    }
    // later use
    if let Some(index) = p_index {
        *index += 1;
    }
    true
}
"#;
    let errors = vec![CompilerError {
        code: "E0382".into(),
        line: 6,
        message: "use of moved value: `p_index`".into(),
    }];
    let result = apply_all_rules(source, &errors);
    // Should add `mut` to parameter and `ref mut` to patterns
    assert!(result.contains("mut p_index: Option<&mut u32>"));
    assert!(result.contains("Some(ref mut index)"));
}
```

- [ ] **Step 2: Run test — expected FAIL**

Run: `cargo test -p noricum-tools -- test_rule_mut_option`

- [ ] **Step 3: Implement Rule R3**

```rust
static OPTION_MUT_PARAM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\w+):\s*Option<&mut\s+\w+>").unwrap()
});

static IF_LET_SOME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"if let Some\((\w+)\)\s*=\s*(\w+)").unwrap()
});

/// R3: Fix use-after-move for Option<&mut T> by adding `mut` to binding
/// and `ref mut` to pattern matches.
fn rule_mut_option_ref(source: &str, errors: &[CompilerError]) -> String {
    let moved_vars: Vec<&str> = errors
        .iter()
        .filter(|e| e.code == "E0382" && e.message.contains("use of moved value"))
        .filter_map(|e| {
            e.message
                .strip_prefix("use of moved value: `")
                .and_then(|s| s.strip_suffix('`'))
        })
        .collect();

    if moved_vars.is_empty() {
        return source.to_string();
    }

    let mut result = source.to_string();
    for var in &moved_vars {
        // Check if this variable is an Option<&mut T> parameter
        let is_option_mut = OPTION_MUT_PARAM_RE
            .captures(&result)
            .is_some_and(|c| &c[1] == *var);
        if !is_option_mut {
            continue;
        }

        // Add `mut` to the parameter: `var: Option` → `mut var: Option`
        let param_pattern = format!("{var}: Option<&mut");
        let param_replacement = format!("mut {var}: Option<&mut");
        if !result.contains(&param_replacement) {
            result = result.replacen(&param_pattern, &param_replacement, 1);
        }

        // Change `if let Some(x) = var` → `if let Some(ref mut x) = var`
        let new_result = IF_LET_SOME_RE
            .replace_all(&result, |caps: &regex::Captures| {
                if &caps[2] == *var {
                    format!("if let Some(ref mut {}) = {}", &caps[1], &caps[2])
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();
        result = new_result;
    }
    result
}
```

Update `apply_all_rules`:

```rust
pub fn apply_all_rules(source: &str, errors: &[CompilerError]) -> String {
    let mut current = source.to_string();
    current = rule_dedup_functions(&current, errors);
    current = rule_clone_bounds(&current, errors);
    current = rule_mut_option_ref(&current, errors);
    current
}
```

- [ ] **Step 4: Run test — expected PASS**

Run: `cargo test -p noricum-tools -- test_rule_mut_option`

- [ ] **Step 5: Commit**

```bash
git add crates/noricum-tools/src/repair_rules.rs
git commit -m "feat: P30 Rule R3 — fix Option<&mut T> use-after-move"
```

---

### Task 5: Integration test with real fixture

Use the Run 3 iter-05 artifact (5 real errors) to validate rules work on actual assembly output.

**Files:**
- Modify: `crates/noricum-tools/src/repair_rules.rs`

- [ ] **Step 1: Copy fixture and write integration test**

```bash
cp .noricum-artifacts/miniz_zip-20260310-150830/05-repair/iter-05.rs tests/fixtures/repair/assembly-iter05.rs
```

```rust
#[test]
fn test_rules_on_real_assembly() {
    let source = include_str!("../../../tests/fixtures/repair/assembly-iter05.rs");
    // Get real errors by compiling
    let compile_result = crate::compiler::check_rust_compiles(source).unwrap();
    assert!(!compile_result.success, "fixture should have errors");

    let errors = parse_rustc_errors(&compile_result.stderr);
    assert!(!errors.is_empty(), "should parse errors from fixture");

    let fixed = apply_all_rules(source, &errors);
    // Verify rules changed something
    assert_ne!(source, fixed, "rules should have modified the source");

    // Check that key fixes were applied
    assert!(
        fixed.contains("Clone"),
        "R1 should have added Clone bound"
    );
}
```

- [ ] **Step 2: Run test**

Run: `cargo test -p noricum-tools -- test_rules_on_real_assembly`
Expected: PASS

- [ ] **Step 3: Verify error reduction**

Add a second assertion to the test:

```rust
// Re-compile the fixed version and check error count reduced
let fixed_result = crate::compiler::check_rust_compiles(&fixed).unwrap();
let fixed_errors = parse_rustc_errors(&fixed_result.stderr);
assert!(
    fixed_errors.len() < errors.len(),
    "rules should reduce error count: {} -> {}",
    errors.len(),
    fixed_errors.len()
);
```

- [ ] **Step 4: Run test — expected PASS**

Run: `cargo test -p noricum-tools -- test_rules_on_real_assembly`

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/repair/ crates/noricum-tools/src/repair_rules.rs
git commit -m "test: P30 integration test with real assembly fixture"
```

---

## Chunk 2: Surgical Repair (Phase 2) + Orchestrator Integration

### Task 6: Function extractor

Given a line number from a compiler error, extract the complete function containing that line.

**Files:**
- Create: `crates/noricum-core/src/surgical_repair.rs`
- Modify: `crates/noricum-core/src/lib.rs`

- [ ] **Step 1: Write the test**

Add to `crates/noricum-core/src/surgical_repair.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_function_at_line() {
        let source = r#"use std::io;

fn foo() -> i32 {
    let x = 1;
    x + 1
}

fn bar(zip: &mut ZipArchive) -> bool {
    let state = &mut zip.state;
    mz_zip_clear(zip, state);
    true
}

fn baz() {}
"#;
        let (name, body) = extract_function_at_line(source, 10).unwrap();
        assert_eq!(name, "bar");
        assert!(body.contains("mz_zip_clear"));
        assert!(!body.contains("fn foo")); // should not include other functions

        // Line in foo
        let (name, _) = extract_function_at_line(source, 4).unwrap();
        assert_eq!(name, "foo");

        // Line outside any function
        assert!(extract_function_at_line(source, 1).is_none());
    }
}
```

- [ ] **Step 2: Run test — expected FAIL**

Run: `cargo test -p noricum-core -- test_extract_function_at_line`

- [ ] **Step 3: Implement function extractor**

```rust
//! P30 Phase 2: Surgical repair — extract failing functions, gather context,
//! send focused LLM requests, and splice fixes back into the source.

use regex::Regex;
use std::sync::LazyLock;

static FN_START_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)").unwrap()
});

/// Extract the complete function containing the given line number.
/// Returns (function_name, function_source) or None if line is not inside a function.
pub fn extract_function_at_line(source: &str, target_line: usize) -> Option<(String, String)> {
    let lines: Vec<&str> = source.lines().collect();
    if target_line == 0 || target_line > lines.len() {
        return None;
    }

    // Search backwards from target_line to find fn declaration
    let mut fn_start = None;
    let mut fn_name = String::new();
    for i in (0..target_line).rev() {
        if let Some(caps) = FN_START_RE.captures(lines[i]) {
            fn_start = Some(i);
            fn_name = caps[1].to_string();
            break;
        }
    }
    let fn_start = fn_start?;

    // Search forward from fn_start to find matching closing brace
    let mut brace_depth: i32 = 0;
    let mut fn_end = fn_start;
    for i in fn_start..lines.len() {
        for ch in lines[i].chars() {
            match ch {
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                _ => {}
            }
        }
        fn_end = i;
        if brace_depth == 0 && i > fn_start {
            break;
        }
    }

    let body: String = lines[fn_start..=fn_end].join("\n");
    Some((fn_name, body))
}
```

Add to `crates/noricum-core/src/lib.rs`:

```rust
pub mod surgical_repair;
```

- [ ] **Step 4: Run test — expected PASS**

Run: `cargo test -p noricum-core -- test_extract_function_at_line`

- [ ] **Step 5: Commit**

```bash
git add crates/noricum-core/src/surgical_repair.rs crates/noricum-core/src/lib.rs
git commit -m "feat: P30 function extractor for surgical repair"
```

---

### Task 7: Context gatherer

For a failing function, extract the types it references (structs, enums) and sibling function signatures.

**Files:**
- Modify: `crates/noricum-core/src/surgical_repair.rs`

- [ ] **Step 1: Write the test**

```rust
#[test]
fn test_gather_type_context() {
    let source = r#"pub struct ZipArchive {
    pub mode: u32,
    pub state: Option<ZipState>,
}

pub struct ZipState {
    pub offsets: Vec<u64>,
}

pub enum ZipError {
    InvalidParam,
    Internal,
}

fn helper(zip: &mut ZipArchive) -> bool { true }

fn broken_fn(zip: &mut ZipArchive) -> Result<(), ZipError> {
    let state = &mut zip.state;
    helper(zip);
    Ok(())
}

fn unrelated(x: i32) -> i32 { x }
"#;
    let ctx = gather_context(source, "broken_fn");
    // Should include types used by broken_fn
    assert!(ctx.contains("struct ZipArchive"));
    assert!(ctx.contains("enum ZipError"));
    // Should include signatures of called functions
    assert!(ctx.contains("fn helper(zip: &mut ZipArchive) -> bool"));
    // Should NOT include unrelated function body
    assert!(!ctx.contains("fn unrelated"));
    // Should NOT include full function bodies of siblings
    assert!(!ctx.contains("{ true }"));
}
```

- [ ] **Step 2: Run test — expected FAIL**

Run: `cargo test -p noricum-core -- test_gather_type_context`

- [ ] **Step 3: Implement context gatherer**

```rust
static STRUCT_ENUM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:pub\s+)?(?:struct|enum)\s+(\w+)").unwrap()
});

static TYPE_REF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b([A-Z]\w+)\b").unwrap()
});

/// Gather type definitions and sibling function signatures relevant to a function.
pub fn gather_context(source: &str, fn_name: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();

    // 1. Find the target function body to know what types it references
    let fn_body = lines
        .iter()
        .enumerate()
        .find_map(|(i, _)| extract_function_at_line(source, i + 1).filter(|(n, _)| n == fn_name))
        .map(|(_, body)| body)
        .unwrap_or_default();

    // 2. Collect type names referenced in the function
    let referenced_types: std::collections::HashSet<String> = TYPE_REF_RE
        .captures_iter(&fn_body)
        .map(|c| c[1].to_string())
        .collect();

    // 3. Extract matching struct/enum definitions from source
    let mut context_parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(caps) = STRUCT_ENUM_RE.captures(lines[i]) {
            let type_name = &caps[1];
            if referenced_types.contains(type_name) {
                // Extract the full definition (until closing brace)
                let mut brace_depth: i32 = 0;
                let start = i;
                loop {
                    for ch in lines[i].chars() {
                        match ch {
                            '{' => brace_depth += 1,
                            '}' => brace_depth -= 1,
                            _ => {}
                        }
                    }
                    i += 1;
                    if brace_depth == 0 && i > start + 1 {
                        break;
                    }
                    if i >= lines.len() {
                        break;
                    }
                }
                context_parts.push(lines[start..i].join("\n"));
                continue;
            }
        }

        // 4. Extract sibling function signatures (not bodies, not the target fn)
        if let Some(caps) = FN_START_RE.captures(lines[i]) {
            let name = &caps[1];
            if name != fn_name && fn_body.contains(name) {
                // Include just the signature line
                let sig = lines[i].trim_end_matches('{').trim();
                context_parts.push(format!("{sig};"));
            }
        }

        i += 1;
    }

    context_parts.join("\n\n")
}
```

- [ ] **Step 4: Run test — expected PASS**

Run: `cargo test -p noricum-core -- test_gather_type_context`

- [ ] **Step 5: Commit**

```bash
git add crates/noricum-core/src/surgical_repair.rs
git commit -m "feat: P30 context gatherer for surgical repair"
```

---

### Task 8: Function splicer

Replace a function in the source with a fixed version.

**Files:**
- Modify: `crates/noricum-core/src/surgical_repair.rs`

- [ ] **Step 1: Write the test**

```rust
#[test]
fn test_splice_function() {
    let source = r#"fn foo() -> i32 { 1 }

fn bar(x: i32) -> i32 {
    x + 1
}

fn baz() -> bool { true }
"#;
    let new_bar = "fn bar(x: i32) -> i32 {\n    x * 2\n}";
    let result = splice_function(source, "bar", new_bar);
    assert!(result.contains("x * 2"));
    assert!(!result.contains("x + 1"));
    assert!(result.contains("fn foo"));
    assert!(result.contains("fn baz"));
}
```

- [ ] **Step 2: Run test — expected FAIL**

Run: `cargo test -p noricum-core -- test_splice_function`

- [ ] **Step 3: Implement splicer**

```rust
/// Replace a function in the source with a new version.
/// Finds the function by name, removes the old body, inserts the new one.
pub fn splice_function(source: &str, fn_name: &str, new_fn: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();

    // Find function start
    let mut fn_start = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = FN_START_RE.captures(line) {
            if &caps[1] == fn_name {
                fn_start = Some(i);
                break;
            }
        }
    }
    let Some(fn_start) = fn_start else {
        return source.to_string();
    };

    // Find function end
    let mut brace_depth: i32 = 0;
    let mut fn_end = fn_start;
    for i in fn_start..lines.len() {
        for ch in lines[i].chars() {
            match ch {
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                _ => {}
            }
        }
        fn_end = i;
        if brace_depth == 0 && i > fn_start {
            break;
        }
    }

    // Splice: before + new function + after
    let mut result = String::with_capacity(source.len());
    for line in &lines[..fn_start] {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(new_fn);
    result.push('\n');
    for line in &lines[fn_end + 1..] {
        result.push_str(line);
        result.push('\n');
    }
    result
}
```

- [ ] **Step 4: Run test — expected PASS**

Run: `cargo test -p noricum-core -- test_splice_function`

- [ ] **Step 5: Commit**

```bash
git add crates/noricum-core/src/surgical_repair.rs
git commit -m "feat: P30 function splicer for surgical repair"
```

---

### Task 9: Orchestrator integration — hybrid_repair

Wire everything together in the orchestrator.

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs`

- [ ] **Step 1: Write a unit test**

```rust
#[test]
fn test_hybrid_repair_phases() {
    // Test that hybrid_repair is only used for large files
    let small = 500; // < MODULAR_FILE_LOC
    let large = 3000; // > MODULAR_FILE_LOC
    assert!(small <= MODULAR_FILE_LOC);
    assert!(large > MODULAR_FILE_LOC);
}
```

- [ ] **Step 2: Implement hybrid_repair in orchestrator.rs**

Add near the top of the file, after existing imports:

```rust
use noricum_tools::repair_rules::{apply_all_rules, parse_rustc_errors};
use crate::surgical_repair::{extract_function_at_line, gather_context, splice_function};
```

Add the `hybrid_repair` async function (called from Stage 7 when `c_lines > MODULAR_FILE_LOC`):

```rust
/// P30: Three-phase hybrid repair for assembled outputs.
/// Phase 1: Mechanical rules (0 LLM cost)
/// Phase 2: Surgical per-function repair (focused LLM)
/// Phase 3: Legacy full-file repair (max 3 iterations, fallback)
async fn hybrid_repair(
    unit: &mut noricum_ir::FunctionUnit,
    client: &noricum_agents::providers::LlmClient,
    provider_config: &noricum_agents::providers::ProviderConfig,
    difficulty: noricum_ir::Difficulty,
    config: &MigrationConfig,
    artifacts: &Option<crate::artifacts::ArtifactStore>,
) -> Result<(), CoreError> {
    let name = unit.name.clone();
    let rust_source = unit.rust_output.as_deref().unwrap_or("");

    // --- Phase 1: Rule Engine ---
    info!(function = %name, "P30 Phase 1: applying mechanical repair rules");
    let compile_result = noricum_tools::compiler::check_rust_compiles(rust_source)?;
    if compile_result.success {
        info!(function = %name, "P30: already compiles, no repair needed");
        return Ok(());
    }

    let errors = parse_rustc_errors(&compile_result.stderr);
    let error_count_before = errors.len();
    let fixed = apply_all_rules(rust_source, &errors);

    // Re-compile after rules
    let post_rules = noricum_tools::compiler::check_rust_compiles(&fixed)?;
    let post_errors = parse_rustc_errors(&post_rules.stderr);
    info!(
        function = %name,
        errors_before = error_count_before,
        errors_after = post_errors.len(),
        "P30 Phase 1 complete"
    );

    unit.rust_output = Some(fixed.clone());
    if post_rules.success {
        info!(function = %name, "P30 Phase 1: rules resolved all errors");
        let validation = noricum_validation::validate_with_threshold(unit, config.min_idiomatic_score)?;
        noricum_validation::apply_validation(unit, &validation);
        return Ok(());
    }

    // --- Phase 2: Surgical Repair ---
    info!(
        function = %name,
        remaining_errors = post_errors.len(),
        "P30 Phase 2: surgical per-function repair"
    );

    let repair_model = noricum_agents::providers::select_repair_model(provider_config, difficulty)?;
    let mut current_source = fixed;
    let max_surgical = 5;

    for cycle in 0..max_surgical {
        let compile_check = noricum_tools::compiler::check_rust_compiles(&current_source)?;
        if compile_check.success {
            info!(function = %name, cycle, "P30 Phase 2: surgical repair resolved all errors");
            break;
        }

        let cycle_errors = parse_rustc_errors(&compile_check.stderr);
        if cycle_errors.is_empty() {
            break;
        }

        // Take the first error and fix its function
        let err = &cycle_errors[0];
        let Some((fn_name, fn_body)) = extract_function_at_line(&current_source, err.line) else {
            info!(function = %name, line = err.line, "P30 Phase 2: could not extract function at error line, skipping");
            break;
        };

        let context = gather_context(&current_source, &fn_name);

        let prompt = format!(
            "Fix this Rust function. The compiler error is:\n\
             {}: {}\n\n\
             These types and function signatures are already defined (DO NOT redefine them):\n\
             {}\n\n\
             Here is the function to fix:\n\
             {}\n\n\
             Return ONLY the fixed function, nothing else. No markdown fences.",
            err.code, err.message, context, fn_body
        );

        info!(
            function = %name,
            cycle,
            error = %err.code,
            target_fn = %fn_name,
            "P30 Phase 2: sending surgical repair request"
        );

        let repair_result = noricum_agents::repair::repair_with_prompt(
            client,
            &repair_model.model,
            &prompt,
        )
        .await;

        match repair_result {
            Ok(fixed_fn) => {
                let extracted = noricum_tools::ast::extract_rust_code(&fixed_fn);
                current_source = splice_function(&current_source, &fn_name, &extracted);
                unit.metrics.llm_calls += 1;
            }
            Err(e) => {
                warn!(function = %name, error = %e, "P30 Phase 2: surgical repair LLM call failed");
                break;
            }
        }
    }

    unit.rust_output = Some(current_source);

    // --- Phase 3: Legacy fallback (max 3 iterations) ---
    let final_check = noricum_tools::compiler::check_rust_compiles(
        unit.rust_output.as_deref().unwrap_or(""),
    )?;

    if !final_check.success {
        info!(
            function = %name,
            remaining_errors = parse_rustc_errors(&final_check.stderr).len(),
            "P30 Phase 3: falling back to legacy whole-file repair (max 3 iterations)"
        );
        // The existing repair loop will handle this — return and let Stage 7 continue
        // with max_iters capped at 3
    }

    Ok(())
}
```

- [ ] **Step 3: Wire into Stage 7**

In the Stage 7 repair section of `migrate_function_async`, replace the current repair model selection and add the hybrid branch:

Find the block starting with:
```rust
if !validation.passed {
    let repair_start = Instant::now();
    // P29: Use fast repair model...
    let repair_model_sel = select_repair_model(&provider_config, difficulty)?;
```

Add hybrid branch before it:

```rust
if !validation.passed {
    let repair_start = Instant::now();

    // P30: Use hybrid repair for assembled outputs
    if c_lines > MODULAR_FILE_LOC {
        info!(function = %name, c_lines, "P30: using hybrid repair for assembled output");
        hybrid_repair(&mut unit, &client, &provider_config, difficulty, config, &artifacts).await?;

        // Re-validate after hybrid repair
        let post_hybrid = noricum_validation::validate_with_threshold(&unit, config.min_idiomatic_score)?;
        noricum_validation::apply_validation_with_max(&mut unit, &post_hybrid, 3);

        if post_hybrid.passed {
            unit.metrics.repair_ms = repair_start.elapsed().as_millis() as u64;
            // skip legacy repair loop
        } else {
            // Fall through to legacy repair with max 3 iterations
            info!(function = %name, "P30 Phase 3: hybrid repair incomplete, entering legacy repair");
        }
    }

    // Existing repair loop continues here (unchanged for small files,
    // or as Phase 3 fallback for hybrid)
```

- [ ] **Step 4: Check compilation**

Run: `cargo check`
Expected: success

- [ ] **Step 5: Run all tests**

Run: `cargo test -p noricum-core`
Expected: all pass (existing + new tests)

- [ ] **Step 6: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs
git commit -m "feat: P30 hybrid repair integration in orchestrator Stage 7"
```

---

### Task 10: Repair agent — add repair_with_prompt function

Phase 2 needs a way to send a custom prompt to the repair agent (not the standard repair prompt with full file + errors).

**Files:**
- Modify: `crates/noricum-agents/src/repair.rs`

- [ ] **Step 1: Check if repair_with_prompt exists**

Run: `grep -n "repair_with_prompt" crates/noricum-agents/src/repair.rs`
If it doesn't exist, add it.

- [ ] **Step 2: Implement repair_with_prompt**

```rust
/// Send a custom repair prompt to the LLM and return the response.
/// Used by P30 surgical repair for focused per-function fixes.
pub async fn repair_with_prompt(
    client: &crate::providers::LlmClient,
    model: &str,
    prompt: &str,
) -> Result<String, crate::AgentError> {
    use rig::completion::Prompt;

    info!(model, prompt_len = prompt.len(), "P30: surgical repair call");

    let response = match client {
        crate::providers::LlmClient::Anthropic(c) => {
            c.agent(model).build().prompt(prompt).await?
        }
        crate::providers::LlmClient::DeepSeek(c) => {
            c.agent(model).build().prompt(prompt).await?
        }
        crate::providers::LlmClient::Ollama(c) => {
            c.agent(model).build().prompt(prompt).await?
        }
    };

    Ok(response)
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: success

- [ ] **Step 4: Commit**

```bash
git add crates/noricum-agents/src/repair.rs
git commit -m "feat: P30 repair_with_prompt for surgical repair"
```

---

### Task 11: Final integration test

End-to-end test: load the fixture, run hybrid repair phases, verify improvement.

**Files:**
- Modify: `crates/noricum-tools/src/repair_rules.rs`

- [ ] **Step 1: Write comprehensive rule engine test**

```rust
#[test]
fn test_apply_all_rules_reduces_errors() {
    // This test uses a small synthetic fixture that exercises multiple rules
    let source = r#"
fn resize_array<T: Default>(arr: &mut Vec<T>, n: usize) {
    arr.resize(n, T::default());
}

fn resize_array<T>(arr: &mut Vec<T>, n: usize) {
    arr.resize(n, T::default());
}

fn process(mut p: Option<&mut u32>) {
    if let Some(v) = p {
        *v = 1;
    }
    if let Some(v) = p {
        *v = 2;
    }
}
"#;
    let compile_result = crate::compiler::check_rust_compiles(source).unwrap();
    assert!(!compile_result.success);
    let errors = parse_rustc_errors(&compile_result.stderr);

    let fixed = apply_all_rules(source, &errors);
    let fixed_result = crate::compiler::check_rust_compiles(&fixed).unwrap();
    let fixed_errors = parse_rustc_errors(&fixed_result.stderr);

    assert!(
        fixed_errors.len() < errors.len(),
        "should reduce errors: {} -> {}",
        errors.len(),
        fixed_errors.len()
    );
}
```

- [ ] **Step 2: Run all tests**

Run: `cargo test -p noricum-tools -p noricum-core`
Expected: all pass

- [ ] **Step 3: Commit**

```bash
git add crates/noricum-tools/src/repair_rules.rs
git commit -m "test: P30 comprehensive rule engine integration test"
```
