# P31 Assembly Cleanup Fixes + Run 6

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 3 bugs found in Run 5 (markdown fence leak, duplicate use imports, syntax error blindness) and validate with Run 6 of miniz_zip.c.

**Architecture:** Three targeted fixes in 2 files. Bug 1: strip markdown fences in assembly + add Rule R0. Bug 2: merge `use` imports by base path. Bug 3: extend error parser regex for codeless errors. Then Run 6 to validate.

**Working directory:** `/home/marche/noricum`

**Tech Stack:** Rust 2024, regex, noricum-tools + noricum-core crates

**Files:**
- Modify: `crates/noricum-tools/src/repair_rules.rs` (R0 rule + extended parser)
- Modify: `crates/noricum-core/src/orchestrator.rs` (fence strip in assembly + use merge)
- Read: `.noricum-artifacts/miniz_zip-20260310-221601/05-repair/iter-00.rs` (Run 5 assembly for validation)

---

## Chunk 1: Bug fixes in repair_rules.rs

### Task 1: Extend parse_rustc_errors for syntax errors

**Files:**
- Modify: `crates/noricum-tools/src/repair_rules.rs:27-45`

- [ ] **Step 1: Write failing test for syntax error parsing**

Add to the `tests` module in `repair_rules.rs`:

```rust
#[test]
fn test_parse_rustc_errors_syntax_errors() {
    let stderr = r#"error: unknown start of token: `
 --> check.rs:730:1
  |
730 | ```rust
  | ^

error: this file contains an unclosed delimiter
 --> check.rs:3543:1

error[E0432]: unresolved import `std::io`
  --> check.rs:11:5
"#;
    let errors = parse_rustc_errors(stderr);
    assert_eq!(errors.len(), 3, "should parse both syntax and coded errors: {errors:?}");
    // Syntax errors get code "SYNTAX"
    assert_eq!(errors[0].code, "SYNTAX");
    assert_eq!(errors[0].line, 730);
    assert!(errors[0].message.contains("unknown start of token"));
    // Errors without location get line 0
    assert_eq!(errors[1].code, "SYNTAX");
    assert!(errors[1].message.contains("unclosed delimiter"));
    // Coded errors still work
    assert_eq!(errors[2].code, "E0432");
    assert_eq!(errors[2].line, 11);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p noricum-tools test_parse_rustc_errors_syntax_errors -- --nocapture
```

Expected: FAIL — current regex only matches `error[E\d+]` pattern.

- [ ] **Step 3: Extend parse_rustc_errors with second regex**

In `parse_rustc_errors()` (line 27), add a second regex after the existing one to capture codeless errors:

```rust
pub fn parse_rustc_errors(stderr: &str) -> Vec<CompilerError> {
    let mut results = Vec::new();

    // Pattern 1: coded errors — error[E0499]: message \n  --> file:line:col
    let coded_re =
        Regex::new(r"error\[(?P<code>E\d+)\]: (?P<message>[^\n]+)\n\s*--> [^:]+:(?P<line>\d+):\d+")
            .expect("static regex is valid");

    for cap in coded_re.captures_iter(stderr) {
        let Some(code) = cap.name("code") else { continue };
        let Some(message) = cap.name("message") else { continue };
        let line: usize = cap.name("line").and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
        results.push(CompilerError {
            code: code.as_str().to_string(),
            line,
            message: message.as_str().to_string(),
        });
    }

    // Pattern 2: syntax errors — error: message \n --> file:line:col (no error code)
    let syntax_re =
        Regex::new(r"(?m)^error: (?P<message>[^\n]+?)(?:\n\s*--> [^:]+:(?P<line>\d+):\d+)?")
            .expect("static regex is valid");

    for cap in syntax_re.captures_iter(stderr) {
        let Some(message) = cap.name("message") else { continue };
        let msg = message.as_str().to_string();
        // Skip the "aborting due to N previous errors" summary line
        if msg.starts_with("aborting due to") || msg.starts_with("could not compile") {
            continue;
        }
        let line: usize = cap.name("line").and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
        results.push(CompilerError {
            code: "SYNTAX".to_string(),
            line,
            message: msg,
        });
    }

    results
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p noricum-tools test_parse_rustc_errors_syntax -- --nocapture
```

Expected: PASS. Also run existing tests to verify no regression:

```bash
cargo test -p noricum-tools parse_rustc_errors -- --nocapture
```

---

### Task 2: Add Rule R0 — strip markdown fences

**Files:**
- Modify: `crates/noricum-tools/src/repair_rules.rs`

- [ ] **Step 1: Write failing test for R0**

Add to the `tests` module:

```rust
#[test]
fn test_rule_strip_markdown_fences() {
    let source = r#"use std::io;

fn foo() -> i32 { 1 }

// --- Module: mz_p2 ---
```rust
fn bar() -> i32 { 2 }
```

fn baz() -> i32 { 3 }
"#;
    let result = rule_strip_markdown_fences(source);
    assert!(!result.contains("```"), "fences should be stripped: {result}");
    assert!(result.contains("fn foo()"), "code before fence preserved");
    assert!(result.contains("fn bar()"), "code inside fence preserved");
    assert!(result.contains("fn baz()"), "code after fence preserved");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p noricum-tools test_rule_strip_markdown_fences -- --nocapture
```

Expected: FAIL — function doesn't exist yet.

- [ ] **Step 3: Implement rule_strip_markdown_fences**

Add before `apply_all_rules`:

```rust
/// R0: Strip any markdown code fences that leaked into Rust source.
///
/// LLM responses sometimes include ` ```rust ` / ` ``` ` markers that survive
/// extraction. This rule removes them as a defensive measure.
pub fn rule_strip_markdown_fences(source: &str) -> String {
    source
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("```")
        })
        .collect::<Vec<&str>>()
        .join("\n")
}
```

- [ ] **Step 4: Wire R0 into apply_all_rules as the first rule**

Change `apply_all_rules`:

```rust
pub fn apply_all_rules(source: &str, errors: &[CompilerError]) -> String {
    // R0 first: strip markdown fences (always, no error check needed)
    let mut result = rule_strip_markdown_fences(source);

    // R2: dedup removes duplicate definitions, which can cascade
    result = rule_dedup_functions(&result, errors);
    // R1: add Clone bounds where needed
    result = rule_clone_bounds(&result, errors);
    // R3: fix Option<&mut T> move errors
    result = rule_mut_option_ref(&result, errors);

    result
}
```

- [ ] **Step 5: Run all repair_rules tests**

```bash
cargo test -p noricum-tools repair_rules -- --nocapture
```

Expected: ALL PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/noricum-tools/src/repair_rules.rs
git commit -m "feat: P31 R0 fence stripping + syntax error parsing in rule engine"
```

---

## Chunk 2: Bug fixes in orchestrator.rs

### Task 3: Strip markdown fences in assemble_module_outputs

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs:2779-2807`

- [ ] **Step 1: Write failing test**

Add to the existing `tests` module in `orchestrator.rs`:

```rust
#[test]
fn test_assemble_strips_markdown_fences() {
    let modules = vec![
        ("mod_a".to_string(), "use std::io;\n\nfn foo() -> i32 { 1 }".to_string(), true),
        ("mod_b".to_string(), "```rust\nfn bar() -> i32 { 2 }\n```".to_string(), false),
    ];
    let assembled = assemble_module_outputs(&modules);
    assert!(!assembled.contains("```"), "fences should be stripped from assembly:\n{assembled}");
    assert!(assembled.contains("fn foo()"));
    assert!(assembled.contains("fn bar()"));
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p noricum-core test_assemble_strips_markdown_fences -- --nocapture
```

Expected: FAIL — fences pass through currently.

- [ ] **Step 3: Add fence stripping in assemble_module_outputs**

In `assemble_module_outputs`, filter fence lines from each module before dedup:

```rust
fn assemble_module_outputs(modules: &[(String, String, bool)]) -> String {
    let mut all_uses: Vec<String> = Vec::new();
    let mut code_parts: Vec<String> = Vec::new();
    let mut defined_types: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (mod_name, rust_code, _compiles) in modules {
        // P31: Strip markdown fences before processing
        let clean_code: String = rust_code
            .lines()
            .filter(|line| !line.trim().starts_with("```"))
            .collect::<Vec<&str>>()
            .join("\n");

        let mod_code_lines = dedup_module_definitions(&clean_code, &mut all_uses, &mut defined_types);
        // ... rest unchanged
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p noricum-core test_assemble_strips_markdown_fences -- --nocapture
```

Expected: PASS.

---

### Task 4: Merge duplicate use imports

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs:2816-2864`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn test_assemble_merges_use_imports() {
    let modules = vec![
        ("a".to_string(), "use std::io::{self, Read};\nfn a() {}".to_string(), true),
        ("b".to_string(), "use std::io::{self, Read, Seek, SeekFrom};\nfn b() {}".to_string(), true),
        ("c".to_string(), "use std::io::{self, Write, Seek, SeekFrom};\nfn c() {}".to_string(), true),
    ];
    let assembled = assemble_module_outputs(&modules);

    // Should have exactly ONE std::io import with all items merged
    let io_lines: Vec<&str> = assembled.lines()
        .filter(|l| l.contains("use std::io"))
        .collect();
    assert_eq!(io_lines.len(), 1, "should merge into one use std::io line, got: {io_lines:?}");

    let io_line = io_lines[0];
    for item in &["Read", "Seek", "SeekFrom", "Write", "self"] {
        assert!(io_line.contains(item), "merged import should contain {item}: {io_line}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p noricum-core test_assemble_merges_use_imports -- --nocapture
```

Expected: FAIL — current dedup uses exact string match.

- [ ] **Step 3: Add merge_use_statements helper**

Add a new helper function before `assemble_module_outputs`:

```rust
/// Merge `use` statements that share the same base path.
///
/// Groups `use std::io::{Read, Write};` and `use std::io::{self, Seek};`
/// into `use std::io::{self, Read, Seek, Write};`.
/// Simple `use foo::Bar;` are kept as-is (deduplicated by exact match).
fn merge_use_statements(uses: Vec<String>) -> Vec<String> {
    use std::collections::{BTreeMap, BTreeSet};

    let brace_re = regex::Regex::new(r"^use\s+(?P<path>[^{;]+)::\{(?P<items>[^}]+)\};$")
        .expect("static regex");
    let simple_re = regex::Regex::new(r"^use\s+(?P<full>[^{]+);$")
        .expect("static regex");

    // path -> set of items
    let mut groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut simple_uses: BTreeSet<String> = BTreeSet::new();

    for u in &uses {
        let trimmed = u.trim();
        if let Some(caps) = brace_re.captures(trimmed) {
            let path = caps["path"].trim().to_string();
            let items: Vec<String> = caps["items"]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let entry = groups.entry(path).or_default();
            for item in items {
                entry.insert(item);
            }
        } else if simple_re.is_match(trimmed) {
            simple_uses.insert(trimmed.to_string());
        }
    }

    let mut result: Vec<String> = Vec::new();

    // Emit merged brace imports
    for (path, items) in &groups {
        let sorted: Vec<&String> = {
            let mut v: Vec<&String> = items.iter().collect();
            // Put `self` first if present
            v.sort_by(|a, b| {
                if a.as_str() == "self" { std::cmp::Ordering::Less }
                else if b.as_str() == "self" { std::cmp::Ordering::Greater }
                else { a.cmp(b) }
            });
            v
        };
        let items_str = sorted.iter().map(|s| s.as_str()).collect::<Vec<&str>>().join(", ");
        result.push(format!("use {path}::{{{items_str}}};"));
    }

    // Emit simple imports (but skip if already covered by a brace import)
    for s in &simple_uses {
        // e.g., "use std::io::Read;" is covered by "use std::io::{Read, ...};"
        let covered = groups.iter().any(|(path, items)| {
            if let Some(rest) = s.strip_prefix(&format!("use {path}::")) {
                let name = rest.trim_end_matches(';').trim();
                items.contains(name)
            } else {
                false
            }
        });
        if !covered {
            result.push(s.clone());
        }
    }

    result.sort();
    result
}
```

- [ ] **Step 4: Wire merge_use_statements into assemble_module_outputs**

Replace the simple `all_uses.sort()` with the merge call. In `assemble_module_outputs`:

```rust
    let mut output = String::new();
    if !all_uses.is_empty() {
        let merged = merge_use_statements(all_uses);
        output.push_str(&merged.join("\n"));
        output.push_str("\n\n");
    }
```

- [ ] **Step 5: Run all tests**

```bash
cargo test -p noricum-core test_assemble -- --nocapture
```

Expected: ALL PASS (both new tests + existing assembly tests).

- [ ] **Step 6: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs
git commit -m "feat: P31 fence strip in assembly + use import merging"
```

---

## Chunk 3: Build verification + Run 6

### Task 5: Full build and test verification

- [ ] **Step 1: Workspace compile check**

```bash
cargo check --workspace
```

Expected: `Finished` with no errors.

- [ ] **Step 2: Run all workspace tests**

```bash
cargo test --workspace
```

Expected: ALL PASS (435+ tests).

- [ ] **Step 3: Clippy check**

```bash
cargo clippy --workspace -- -D warnings 2>&1 | tail -5
```

Expected: No warnings.

- [ ] **Step 4: Quick validation — apply R0 to Run 5 assembly in-memory**

Test that the Run 5 iter-00 assembly compiles better after the fixes:

```bash
set -a && source .env && set +a && \
cargo test -p noricum-tools test_rules_on_real_assembly -- --nocapture
```

This uses the existing fixture. To also test on the Run 5 assembly, add a temporary test or just note the error count reduction.

---

### Task 6: Launch Run 6

- [ ] **Step 1: Export API keys and launch migration**

```bash
set -a && source .env && set +a && \
RUST_LOG=noricum=debug cargo run -p noricum-cli -- migrate tests/fixtures/miniz/miniz_zip.c \
  --provider deepseek \
  --warm-start .noricum-artifacts/miniz_zip-warmstart \
  --skip-c2rust \
  --max-llm-calls 80 \
  2>&1 | tee run6-output.log
```

This will take 30-90 minutes. Key log lines to watch:

**Module phase:**
- `warm-start: skip` — expect 4-5 validated modules reused
- Module scores and states

**Assembly phase:**
- `P30 Phase 1 complete` — check `errors_before` and `errors_after` (should now show non-zero if fences were the issue)
- `P30 Phase 2: surgical` — should fire now that errors are parseable
- Final state: ideally `Validated` or `NearlyCompiles`

- [ ] **Step 2: Analyze results**

```bash
RUN6_DIR=$(ls -td .noricum-artifacts/miniz_zip-2026* | head -1)
echo "Run 6 artifacts: $RUN6_DIR"

# Check manifest
cat "$RUN6_DIR/manifest.json" | python3 -m json.tool

# Check output size
wc -l "$RUN6_DIR/06-final.rs"
grep -c "^fn \|^pub fn " "$RUN6_DIR/06-final.rs"

# P30 metrics from log
echo "=== P30 Phases ==="
grep "P30 Phase" run6-output.log
```

- [ ] **Step 3: Compare with Run 5**

Fill in:

```
| Metric                  | Run 5 (pre-P31) | Run 6 (post-P31) |
|-------------------------|-----------------|------------------|
| Module avg_score        | 94              |                  |
| Assembly errors (start) | 5 syntax        |                  |
| P30 Phase 1 reduction   | 0→0 (blind)     |                  |
| P30 Phase 2 reduction   | skipped         |                  |
| Assembly errors (final) | 3               |                  |
| Final LOC               | 3543            |                  |
| Final functions         | 103             |                  |
| Final state             | FallbackUnsafe  |                  |
```

- [ ] **Step 4: Update MEMORY.md with Run 6 results**

Add Run 6 results to Implementation Status section.

- [ ] **Step 5: Commit results**

```bash
git add run6-output.log
git commit -m "docs: miniz_zip.c Run 6 results — P31 assembly cleanup validation"
```
