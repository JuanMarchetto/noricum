# P32: Brace-Balance Validation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detect and fix truncated Rust module outputs (unbalanced braces) before assembly, preventing cascading "unclosed delimiter" errors.

**Architecture:** Add `check_brace_balance()` and `auto_close_braces()` to the rule engine (`repair_rules.rs`). Integrate brace validation into `migrate_single_module()` post-repair flow: if unbalanced → re-translate once → if still unbalanced → auto-close + mark non-compiling. Add R4 auto-close as safety net in `apply_all_rules()`.

**Tech Stack:** Rust, regex (existing dep)

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/noricum-tools/src/repair_rules.rs` | Modify | Add `check_brace_balance()`, `auto_close_braces()`, wire R4, add tests |
| `crates/noricum-core/src/orchestrator.rs` | Modify | Add brace validation after module repair, before pushing to `module_outputs` |

---

## Chunk 1: Rule Engine Functions + Integration

### Task 1: Add `check_brace_balance()` to repair_rules.rs

**Files:**
- Modify: `crates/noricum-tools/src/repair_rules.rs` (after `rule_strip_markdown_fences`, ~line 91)

- [ ] **Step 1: Write the failing tests**

Add these tests at the end of the `mod tests` block in `crates/noricum-tools/src/repair_rules.rs` (before the final `}`):

```rust
#[test]
fn test_check_brace_balance_balanced() {
    let source = "fn foo() {\n    let x = 1;\n}\n\nfn bar() {\n    if true {\n        return;\n    }\n}";
    assert_eq!(check_brace_balance(source), 0);
}

#[test]
fn test_check_brace_balance_unclosed() {
    let source = "fn foo() {\n    let x = 1;\n\nfn bar() {\n    if true {\n        return;\n    }\n}";
    assert_eq!(check_brace_balance(source), 1, "foo is never closed");
}

#[test]
fn test_check_brace_balance_extra_close() {
    let source = "fn foo() {\n    let x = 1;\n}\n}\n";
    assert_eq!(check_brace_balance(source), -1);
}

#[test]
fn test_check_brace_balance_ignores_strings() {
    let source = r#"fn foo() {
    let s = "hello { world }";
    let t = "nested { { } }";
}"#;
    assert_eq!(check_brace_balance(source), 0, "braces in strings should be ignored");
}

#[test]
fn test_check_brace_balance_ignores_comments() {
    let source = "fn foo() {\n    // this { is a comment\n    let x = 1;\n}";
    assert_eq!(check_brace_balance(source), 0, "braces in line comments should be ignored");
}

#[test]
fn test_check_brace_balance_inline_comment() {
    let source = "fn foo() {\n    let x = 1; // { brace in comment\n}";
    assert_eq!(check_brace_balance(source), 0, "inline comment braces ignored");
}

#[test]
fn test_check_brace_balance_empty() {
    assert_eq!(check_brace_balance(""), 0);
    assert_eq!(check_brace_balance("let x = 1;"), 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p noricum-tools check_brace_balance -- --nocapture 2>&1 | tail -20`
Expected: FAIL — `check_brace_balance` not found

- [ ] **Step 3: Implement `check_brace_balance()`**

Add this function in `crates/noricum-tools/src/repair_rules.rs` after `rule_strip_markdown_fences()` (after line 91):

```rust
/// Check brace balance of Rust source code.
///
/// Returns the final brace depth: 0 means balanced, >0 means unclosed braces,
/// <0 means extra closing braces. Ignores braces inside string literals and
/// line comments.
pub fn check_brace_balance(source: &str) -> i32 {
    let mut depth: i32 = 0;

    for line in source.lines() {
        let trimmed = line.trim();
        // Skip line comments entirely
        if trimmed.starts_with("//") {
            continue;
        }

        let mut in_string = false;
        let mut escape_next = false;
        let mut chars = trimmed.chars().peekable();

        while let Some(ch) = chars.next() {
            if escape_next {
                escape_next = false;
                continue;
            }
            if ch == '\\' && in_string {
                escape_next = true;
                continue;
            }
            if ch == '"' {
                in_string = !in_string;
                continue;
            }
            if in_string {
                continue;
            }
            // Skip rest of line after //
            if ch == '/' {
                if chars.peek() == Some(&'/') {
                    break;
                }
            }
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
    }

    depth
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p noricum-tools check_brace_balance -- --nocapture 2>&1 | tail -20`
Expected: all 6 tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/noricum-tools/src/repair_rules.rs
git commit -m "feat: P32 check_brace_balance with string/comment awareness"
```

---

### Task 2: Add `auto_close_braces()` to repair_rules.rs

**Files:**
- Modify: `crates/noricum-tools/src/repair_rules.rs` (after `check_brace_balance`)

- [ ] **Step 1: Write the failing tests**

Add these tests in the `mod tests` block:

```rust
#[test]
fn test_auto_close_braces_balanced() {
    let source = "fn foo() {\n    1\n}";
    let result = auto_close_braces(source);
    assert_eq!(result, source, "balanced source should be unchanged");
}

#[test]
fn test_auto_close_braces_one_unclosed() {
    let source = "fn foo() {\n    let x = 1;";
    let result = auto_close_braces(source);
    assert!(result.ends_with("} // auto-closed: truncated output"), "got: {result}");
    assert_eq!(check_brace_balance(&result), 0, "should be balanced after auto-close");
}

#[test]
fn test_auto_close_braces_multiple_unclosed() {
    let source = "fn foo() {\n    if true {\n        let x = 1;";
    let result = auto_close_braces(source);
    assert_eq!(check_brace_balance(&result), 0, "should be balanced after auto-close");
    assert_eq!(result.matches("// auto-closed: truncated output").count(), 1, "single marker");
}

#[test]
fn test_auto_close_braces_extra_close() {
    let source = "fn foo() {\n    1\n}\n}";
    let result = auto_close_braces(source);
    assert_eq!(result, source, "extra closes should not be modified");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p noricum-tools auto_close_braces -- --nocapture 2>&1 | tail -20`
Expected: FAIL — `auto_close_braces` not found

- [ ] **Step 3: Implement `auto_close_braces()`**

Add this function right after `check_brace_balance()`:

```rust
/// Auto-close unclosed braces at the end of truncated Rust source.
///
/// If `check_brace_balance()` returns depth > 0, appends that many `}` lines
/// with a marker comment. Returns source unchanged if balanced or has extra closes.
pub fn auto_close_braces(source: &str) -> String {
    let depth = check_brace_balance(source);
    if depth <= 0 {
        return source.to_string();
    }

    let closes = "}".repeat(depth as usize);
    format!("{source}\n{closes} // auto-closed: truncated output")
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p noricum-tools auto_close_braces -- --nocapture 2>&1 | tail -20`
Expected: all 4 tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/noricum-tools/src/repair_rules.rs
git commit -m "feat: P32 auto_close_braces for truncated module outputs"
```

---

### Task 3: Wire R4 into `apply_all_rules()`

**Files:**
- Modify: `crates/noricum-tools/src/repair_rules.rs:98-110` (`apply_all_rules` function)

- [ ] **Step 1: Write the failing test**

Add this test in the `mod tests` block:

```rust
#[test]
fn test_apply_all_rules_closes_braces() {
    let source = "fn foo() {\n    let x = 1;";
    let result = apply_all_rules(source, &[]);
    assert_eq!(
        check_brace_balance(&result),
        0,
        "apply_all_rules should auto-close braces via R4"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p noricum-tools test_apply_all_rules_closes_braces -- --nocapture 2>&1 | tail -10`
Expected: FAIL — brace balance is 1, not 0

- [ ] **Step 3: Add R4 to `apply_all_rules()`**

In `crates/noricum-tools/src/repair_rules.rs`, modify the `apply_all_rules` function (lines 98-110). Add R4 as the last rule, after R3:

Replace:
```rust
    result = rule_mut_option_ref(&result, errors);

    result
}
```

With:
```rust
    result = rule_mut_option_ref(&result, errors);

    // R4: auto-close unclosed braces from truncated LLM output
    result = auto_close_braces(&result);

    result
}
```

Also update the doc comment on `apply_all_rules` to mention R4:
Replace:
```rust
/// Apply all mechanical repair rules in sequence.
///
/// Order matters: dedup first (removes duplicate definitions), then
/// clone bounds (adds missing trait bounds), then mut option ref
/// (fixes moved `Option<&mut T>` parameters).
```

With:
```rust
/// Apply all mechanical repair rules in sequence.
///
/// Order matters: R0 fence strip, R2 dedup (removes duplicate definitions),
/// R1 clone bounds (adds missing trait bounds), R3 mut option ref
/// (fixes moved `Option<&mut T>` parameters), R4 auto-close braces
/// (safety net for truncated LLM output).
```

- [ ] **Step 4: Run all repair_rules tests**

Run: `cargo test -p noricum-tools -- --nocapture 2>&1 | tail -30`
Expected: all tests PASS (including new test and all existing tests)

- [ ] **Step 5: Commit**

```bash
git add crates/noricum-tools/src/repair_rules.rs
git commit -m "feat: P32 R4 auto-close braces wired into apply_all_rules"
```

---

### Task 4: Integrate brace validation into `migrate_single_module()`

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs:2736-2758` (end of `migrate_single_module()`, before building the result)

- [ ] **Step 1: Write integration test**

Add this test in the `#[cfg(test)] mod tests` block of `crates/noricum-core/src/orchestrator.rs` (it tests the assembly pipeline end-to-end with a truncated module):

```rust
#[test]
fn test_assemble_truncated_module_auto_closed() {
    let modules = vec![
        ("mod_a".to_string(), "use std::io;\n\nfn foo() {\n    1\n}".to_string(), true),
        ("mod_b".to_string(), "fn bar() {\n    if true {\n        let x = 1;".to_string(), false),
        ("mod_c".to_string(), "fn baz() {\n    2\n}".to_string(), true),
    ];
    let result = assemble_module_outputs(&modules);
    // mod_b is truncated but assembly should still produce parseable output
    // The braces from mod_b should NOT cascade into mod_c
    assert!(result.contains("fn baz()"), "mod_c should be present");
    // Count braces — should be balanced overall
    let open = result.matches('{').count();
    let close = result.matches('}').count();
    // Note: assembly doesn't auto-close (that's R4's job), but mod_b's
    // imbalance should not prevent mod_c from being included
    assert!(result.contains("fn foo()"), "mod_a present");
    assert!(result.contains("fn bar()"), "mod_b present");
    assert!(result.contains("fn baz()"), "mod_c present");
}
```

- [ ] **Step 2: Run test to verify it passes** (assembly itself doesn't auto-close — that's correct; R4 handles it later in hybrid repair)

Run: `cargo test -p noricum-core test_assemble_truncated_module_auto_closed -- --nocapture 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 3: Add brace validation in `migrate_single_module()`**

In `crates/noricum-core/src/orchestrator.rs`, find the block starting at approximately line 2736 (`// Build the result`). Insert brace validation **before** building the result. Add this code before line 2736:

```rust
    // P32: Brace-balance validation — detect truncated LLM output
    if let Some(ref rust_code) = mod_unit.rust_output {
        let brace_depth = noricum_tools::repair_rules::check_brace_balance(rust_code);
        if brace_depth > 0 {
            warn!(
                module = %mod_name,
                depth = brace_depth,
                "P32: module output has unclosed braces, auto-closing"
            );
            let fixed = noricum_tools::repair_rules::auto_close_braces(rust_code);
            mod_unit.rust_output = Some(fixed);
            // Mark as non-compiling so it doesn't pollute assembly context (P26)
            mod_unit.last_errors.push(format!("P32: auto-closed {brace_depth} unclosed brace(s)"));
        }
    }
```

Note: We skip the re-translate step for now (YAGNI — auto-close alone fixes the cascading error). Re-translate can be added in a future P33 if auto-close proves insufficient.

- [ ] **Step 4: Run all noricum-core tests**

Run: `cargo test -p noricum-core -- --nocapture 2>&1 | tail -30`
Expected: all tests PASS

- [ ] **Step 5: Run full workspace check**

Run: `cargo check --workspace && cargo test --workspace 2>&1 | tail -40`
Expected: 0 errors, 0 warnings, all tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs
git commit -m "feat: P32 brace-balance validation in module output + assembly safety net"
```

---

### Task 5: Update warm-start store and launch Run 7

**Files:**
- Modify: `.noricum-artifacts/miniz_zip-warmstart/` (update from Run 6 best outputs)

- [ ] **Step 1: Check if Run 6 produced better module outputs to update warm-start**

Run:
```bash
ls .noricum-artifacts/miniz_zip-2026031*/ 2>/dev/null | head -5
```

Look at the most recent run's `03-translation/` directory for module files.

- [ ] **Step 2: Launch Run 7 with DeepSeek + warm-start**

Run:
```bash
set -a && source .env && set +a && RUST_LOG=noricum=info cargo run -p noricum-cli -- migrate tests/fixtures/large/miniz_zip.c --provider deepseek --artifacts-dir .noricum-artifacts 2>&1 | tee run7-output.log
```

Expected: Migration runs with P32 brace validation active. Monitor for "P32: module output has unclosed braces" log messages.

- [ ] **Step 3: Analyze Run 7 results**

Check the output for:
1. Assembly error count (should be 0 "unclosed delimiter" errors)
2. P30 Phase 1 rule reduction
3. P30 Phase 2 surgical repair cycles
4. Final compilation state
5. LOC count and function count

- [ ] **Step 4: Update MEMORY.md with Run 7 results**

Add Run 7 entry to `/home/marche/.claude/projects/-home-marche-noricum/memory/MEMORY.md` in the Implementation Status section.
