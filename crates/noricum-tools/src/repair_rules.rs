/// Mechanical repair rules for common rustc errors.
///
/// These rules parse compiler error output and apply deterministic fixes
/// to Rust source code, avoiding unnecessary LLM calls for well-understood
/// error patterns.
use regex::Regex;
use tracing::debug;

/// A parsed rustc compiler error.
#[derive(Debug, Clone)]
pub struct CompilerError {
    /// Error code, e.g. `"E0499"`.
    pub code: String,
    /// 1-indexed line number where the error occurs.
    pub line: usize,
    /// The full error message text.
    pub message: String,
}

/// Parse rustc stderr output into structured [`CompilerError`] values.
///
/// Matches the standard rustc error format:
/// ```text
/// error[E0499]: cannot borrow `x` as mutable more than once
///   --> file.rs:12:5
/// ```
pub fn parse_rustc_errors(stderr: &str) -> Vec<CompilerError> {
    let mut results = Vec::new();

    // Pattern 1: coded errors — error[E0499]: message \n  --> file:line:col
    let coded_re =
        Regex::new(r"error\[(?P<code>E\d+)\]: (?P<message>[^\n]+)\n\s*--> [^:]+:(?P<line>\d+):\d+")
            .expect("static regex is valid");

    for cap in coded_re.captures_iter(stderr) {
        let Some(code) = cap.name("code") else {
            continue;
        };
        let Some(message) = cap.name("message") else {
            continue;
        };
        let line: usize = cap
            .name("line")
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        results.push(CompilerError {
            code: code.as_str().to_string(),
            line,
            message: message.as_str().to_string(),
        });
    }

    // Pattern 2: syntax errors — error: message \n --> file:line:col (no error code)
    let syntax_re =
        Regex::new(r"(?m)^error: (?P<message>[^\n]+)\n\s*--> [^:]+:(?P<line>\d+):\d+")
            .expect("static regex is valid");

    for cap in syntax_re.captures_iter(stderr) {
        let Some(message) = cap.name("message") else {
            continue;
        };
        let msg = message.as_str().to_string();
        // Skip summary lines
        if msg.starts_with("aborting due to") || msg.starts_with("could not compile") {
            continue;
        }
        let line: usize = cap
            .name("line")
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        results.push(CompilerError {
            code: "SYNTAX".to_string(),
            line,
            message: msg,
        });
    }

    results
}

/// R0: Strip any markdown code fences that leaked into Rust source.
///
/// LLM responses sometimes include `` ```rust `` / `` ``` `` markers that survive
/// extraction. This rule removes them as a defensive measure.
pub fn rule_strip_markdown_fences(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim().starts_with("```"))
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Apply all mechanical repair rules in sequence.
///
/// Order matters: dedup first (removes duplicate definitions), then
/// clone bounds (adds missing trait bounds), then mut option ref
/// (fixes moved `Option<&mut T>` parameters).
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

// ---------------------------------------------------------------------------
// R1: Clone bounds on generics
// ---------------------------------------------------------------------------

/// When E0599 mentions "trait bounds were not satisfied" on a line inside a
/// generic function, add `+ Clone` to the relevant type parameter bound.
///
/// Handles both `fn foo<T: Trait>(...)` and `fn foo<T: Trait + Other>(...)` forms.
pub fn rule_clone_bounds(source: &str, errors: &[CompilerError]) -> String {
    let clone_errors: Vec<&CompilerError> = errors
        .iter()
        .filter(|e| e.code == "E0599" && e.message.contains("trait bounds were not satisfied"))
        .collect();

    if clone_errors.is_empty() {
        return source.to_string();
    }

    let lines: Vec<&str> = source.lines().collect();
    let mut result_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

    let fn_re = Regex::new(r"fn\s+\w+\s*<([^>]+)>").expect("static regex is valid");
    let bound_re = Regex::new(r"(\w+)\s*:\s*([^,>]+)").expect("static regex is valid");

    for err in &clone_errors {
        // Find the enclosing function declaration by scanning upward from the error line.
        let error_line_idx = err.line.saturating_sub(1);
        if error_line_idx >= lines.len() {
            continue;
        }

        for idx in (0..=error_line_idx).rev() {
            let line = &result_lines[idx];
            if let Some(fn_match) = fn_re.find(line) {
                let fn_text = fn_match.as_str();
                // Check if Clone is already in bounds
                if fn_text.contains("Clone") {
                    break;
                }

                // Find generic params and add Clone to each bounded type param
                let new_fn_text = fn_re
                    .replace(fn_text, |caps: &regex::Captures| {
                        let params = &caps[1];
                        let new_params = bound_re.replace_all(params, |bcaps: &regex::Captures| {
                            let name = &bcaps[1];
                            let bounds = bcaps[2].trim();
                            if bounds.contains("Clone") {
                                format!("{name}: {bounds}")
                            } else {
                                format!("{name}: {bounds} + Clone")
                            }
                        });
                        caps[0].replace(&caps[1], &new_params)
                    })
                    .to_string();

                result_lines[idx] = line.replace(fn_text, &new_fn_text);
                debug!(line = idx + 1, "R1: added Clone bound to generic function");
                break;
            }

            // Stop searching if we hit another function body or top-level item
            let trimmed = line.trim();
            if idx < error_line_idx && (trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ")) {
                break;
            }
        }
    }

    result_lines.join("\n")
}

// ---------------------------------------------------------------------------
// R2: Duplicate function dedup
// ---------------------------------------------------------------------------

/// When E0428 mentions "name `X` is defined multiple times", remove the
/// second definition of X while keeping the first. Uses brace counting to
/// handle multi-line function bodies.
pub fn rule_dedup_functions(source: &str, errors: &[CompilerError]) -> String {
    let dup_errors: Vec<&CompilerError> = errors
        .iter()
        .filter(|e| e.code == "E0428" && e.message.contains("defined multiple times"))
        .collect();

    if dup_errors.is_empty() {
        return source.to_string();
    }

    // Extract the duplicate names from error messages
    let name_re = Regex::new(r"name `(\w+)` is defined multiple times")
        .expect("static regex is valid");

    let mut names_to_dedup: Vec<String> = Vec::new();
    for err in &dup_errors {
        if let Some(caps) = name_re.captures(&err.message) {
            names_to_dedup.push(caps[1].to_string());
        }
    }

    if names_to_dedup.is_empty() {
        return source.to_string();
    }

    let lines: Vec<&str> = source.lines().collect();
    let mut removed_ranges: Vec<(usize, usize)> = Vec::new();

    for name in &names_to_dedup {
        // Build a regex that matches `fn name(` with optional visibility/qualifiers
        let fn_pattern = format!(r"(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?fn\s+{}\s*[\(<]", regex::escape(name));
        let fn_re = Regex::new(&fn_pattern).expect("dynamic regex is valid");

        let mut first_seen = false;
        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim();
            if fn_re.is_match(trimmed) {
                if !first_seen {
                    // Keep the first definition — skip past it
                    first_seen = true;
                    i = skip_function_body(&lines, i);
                    continue;
                }
                // Second (or later) definition — mark for removal
                let start = i;
                let end = skip_function_body(&lines, i);
                removed_ranges.push((start, end));
                debug!(name, start = start + 1, end, "R2: removing duplicate function");
                i = end;
                continue;
            }
            i += 1;
        }
    }

    if removed_ranges.is_empty() {
        return source.to_string();
    }

    // Sort and merge ranges, then remove
    removed_ranges.sort_by_key(|&(start, _)| start);

    let mut result_lines: Vec<String> = Vec::new();
    let mut skip_until = 0usize;
    for (i, line) in lines.iter().enumerate() {
        if i < skip_until {
            continue;
        }
        if let Some(&(start, end)) = removed_ranges.iter().find(|&&(s, _)| s == i) {
            skip_until = end;
            let _ = start; // already used as `i`
            continue;
        }
        result_lines.push(line.to_string());
    }

    result_lines.join("\n")
}

/// Skip past a function body starting at `start_idx` using brace counting.
/// Returns the index of the first line *after* the function body.
fn skip_function_body(lines: &[&str], start_idx: usize) -> usize {
    let mut depth: i32 = 0;
    let mut found_open = false;

    for (i, line) in lines.iter().enumerate().skip(start_idx) {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                found_open = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        if found_open && depth <= 0 {
            return i + 1;
        }
    }

    // If no closing brace found, consume rest of file
    lines.len()
}

// ---------------------------------------------------------------------------
// R3: mut binding for Option<&mut T>
// ---------------------------------------------------------------------------

/// When E0382 mentions "use of moved value" for an `Option<&mut T>` parameter,
/// add `mut` to the parameter binding and change `if let Some(v)` patterns
/// to `if let Some(ref mut v)`.
pub fn rule_mut_option_ref(source: &str, errors: &[CompilerError]) -> String {
    let move_errors: Vec<&CompilerError> = errors
        .iter()
        .filter(|e| e.code == "E0382" && e.message.contains("use of moved value"))
        .collect();

    if move_errors.is_empty() {
        return source.to_string();
    }

    // Extract variable names from "use of moved value: `name`"
    let var_re = Regex::new(r"use of moved value: `(\w+)`").expect("static regex is valid");
    let mut moved_vars: Vec<String> = Vec::new();
    for err in &move_errors {
        if let Some(caps) = var_re.captures(&err.message) {
            let name = caps[1].to_string();
            if !moved_vars.contains(&name) {
                moved_vars.push(name);
            }
        }
    }

    if moved_vars.is_empty() {
        return source.to_string();
    }

    let mut result = source.to_string();

    for var_name in &moved_vars {
        // Check if this variable is an Option<&mut ...> parameter
        let param_pattern = format!(
            r"(?P<pre>\b){}\s*:\s*Option\s*<\s*&mut\s+",
            regex::escape(var_name)
        );
        let param_re = Regex::new(&param_pattern).expect("dynamic regex is valid");

        if !param_re.is_match(&result) {
            continue;
        }

        // Step 1: Add `mut` to the parameter binding if not already present
        // Match `name: Option<&mut T>` and replace with `mut name: Option<&mut T>`
        let binding_pattern = format!(
            r"(?P<before>[(,]\s*)(?P<name>{})\s*:\s*(?P<type>Option\s*<\s*&mut\s+)",
            regex::escape(var_name)
        );
        let binding_re = Regex::new(&binding_pattern).expect("dynamic regex is valid");

        // Only add `mut` if not already there
        let mut_check_pattern = format!(
            r"mut\s+{}\s*:\s*Option\s*<\s*&mut\s+",
            regex::escape(var_name)
        );
        let mut_check_re = Regex::new(&mut_check_pattern).expect("dynamic regex is valid");

        if !mut_check_re.is_match(&result) {
            result = binding_re
                .replace_all(&result, |caps: &regex::Captures| {
                    format!("{}mut {}: {}", &caps["before"], &caps["name"], &caps["type"])
                })
                .to_string();
            debug!(var = var_name, "R3: added mut to parameter binding");
        }

        // Step 2: Change `if let Some(v)` to `if let Some(ref mut v)` for this variable's destructures
        // We look for patterns like `if let Some(ident)` in blocks where `var_name` is used
        let some_pattern = format!(
            r"if\s+let\s+Some\(\s*(?P<inner>\w+)\s*\)\s*=\s*{}\b",
            regex::escape(var_name)
        );
        let some_re = Regex::new(&some_pattern).expect("dynamic regex is valid");

        // Also handle cases where `ref mut` is not already present
        result = some_re
            .replace_all(&result, |caps: &regex::Captures| {
                let inner = &caps["inner"];
                format!("if let Some(ref mut {inner}) = {var_name}")
            })
            .to_string();
        debug!(var = var_name, "R3: added ref mut to Some destructures");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // parse_rustc_errors tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_rustc_errors_basic() {
        let stderr = r#"error[E0499]: cannot borrow `x` as mutable more than once at a time
  --> check.rs:12:5
   |
12 |     let b = &mut x;
   |             ^^^^^^ second mutable borrow occurs here

error[E0308]: mismatched types
  --> check.rs:25:10
   |
25 |     return "hello";
   |            ^^^^^^^ expected `i32`, found `&str`
"#;
        let errors = parse_rustc_errors(stderr);
        assert_eq!(errors.len(), 2);

        assert_eq!(errors[0].code, "E0499");
        assert_eq!(errors[0].line, 12);
        assert_eq!(
            errors[0].message,
            "cannot borrow `x` as mutable more than once at a time"
        );

        assert_eq!(errors[1].code, "E0308");
        assert_eq!(errors[1].line, 25);
        assert_eq!(errors[1].message, "mismatched types");
    }

    #[test]
    fn test_parse_rustc_errors_empty() {
        let stderr = r#"warning: unused variable: `x`
  --> check.rs:3:9
   |
3  |     let x = 5;
   |         ^ help: if this is intentional, prefix it with an underscore: `_x`
"#;
        let errors = parse_rustc_errors(stderr);
        assert!(errors.is_empty());
    }

    // -----------------------------------------------------------------------
    // R1: rule_clone_bounds tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_rule_clone_bounds() {
        let source = r#"fn resize<T: Default>(vec: &mut Vec<T>, new_len: usize) {
    vec.resize(new_len, T::default());
    let cloned = vec[0].clone();
}"#;

        let errors = vec![CompilerError {
            code: "E0599".to_string(),
            line: 3,
            message: "no method named `clone` found for type `T` in the current scope; trait bounds were not satisfied".to_string(),
        }];

        let result = rule_clone_bounds(source, &errors);
        assert!(
            result.contains("T: Default + Clone"),
            "Expected Clone bound added, got:\n{result}"
        );
        // Original function body should be preserved
        assert!(result.contains("vec.resize(new_len, T::default());"));
    }

    #[test]
    fn test_rule_clone_bounds_already_present() {
        let source = r#"fn resize<T: Default + Clone>(vec: &mut Vec<T>, new_len: usize) {
    vec.resize(new_len, T::default());
}"#;

        let errors = vec![CompilerError {
            code: "E0599".to_string(),
            line: 2,
            message: "trait bounds were not satisfied".to_string(),
        }];

        let result = rule_clone_bounds(source, &errors);
        // Should not double-add Clone
        assert!(
            !result.contains("Clone + Clone"),
            "Should not duplicate Clone bound"
        );
    }

    // -----------------------------------------------------------------------
    // R2: rule_dedup_functions tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_rule_dedup_functions() {
        let source = r#"fn helper(x: i32) -> i32 {
    x + 1
}

fn main_logic() -> i32 {
    helper(41)
}

fn helper(x: i32) -> i32 {
    x * 2
}"#;

        let errors = vec![CompilerError {
            code: "E0428".to_string(),
            line: 9,
            message: "the name `helper` is defined multiple times".to_string(),
        }];

        let result = rule_dedup_functions(source, &errors);

        // Count occurrences of "fn helper"
        let count = result.matches("fn helper").count();
        assert_eq!(count, 1, "Expected exactly one fn helper, got {count}:\n{result}");

        // First definition (x + 1) should be kept
        assert!(
            result.contains("x + 1"),
            "First definition should be kept:\n{result}"
        );
        // Second definition (x * 2) should be removed
        assert!(
            !result.contains("x * 2"),
            "Second definition should be removed:\n{result}"
        );
    }

    #[test]
    fn test_rule_dedup_functions_multiline() {
        let source = r#"fn compute(
    a: i32,
    b: i32,
) -> i32 {
    let result = a + b;
    result * 2
}

fn compute(
    a: i32,
    b: i32,
) -> i32 {
    a - b
}"#;

        let errors = vec![CompilerError {
            code: "E0428".to_string(),
            line: 9,
            message: "the name `compute` is defined multiple times".to_string(),
        }];

        let result = rule_dedup_functions(source, &errors);
        let count = result.matches("fn compute").count();
        assert_eq!(count, 1, "Expected exactly one fn compute:\n{result}");
        assert!(result.contains("result * 2"), "First definition kept:\n{result}");
    }

    // -----------------------------------------------------------------------
    // R3: rule_mut_option_ref tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_rule_mut_option_ref() {
        let source = r#"fn process(data: &[u8], p_index: Option<&mut u32>) {
    if let Some(index) = p_index {
        *index = 0;
    }
    // later usage
    if let Some(index) = p_index {
        *index += data.len() as u32;
    }
}"#;

        let errors = vec![CompilerError {
            code: "E0382".to_string(),
            line: 6,
            message: "use of moved value: `p_index`".to_string(),
        }];

        let result = rule_mut_option_ref(source, &errors);

        // Parameter should have `mut`
        assert!(
            result.contains("mut p_index: Option<&mut"),
            "Expected mut binding, got:\n{result}"
        );

        // `if let Some(index)` should become `if let Some(ref mut index)`
        assert!(
            result.contains("Some(ref mut index)"),
            "Expected ref mut in destructure, got:\n{result}"
        );

        // Should appear twice (both usages)
        let ref_mut_count = result.matches("Some(ref mut index)").count();
        assert_eq!(
            ref_mut_count, 2,
            "Expected 2 ref mut destructures, got {ref_mut_count}:\n{result}"
        );
    }

    #[test]
    fn test_rule_mut_option_ref_no_match() {
        let source = r#"fn process(data: &[u8], count: usize) {
    let x = count;
    let y = count;
}"#;

        let errors = vec![CompilerError {
            code: "E0382".to_string(),
            line: 3,
            message: "use of moved value: `count`".to_string(),
        }];

        // count is not Option<&mut T>, so no change
        let result = rule_mut_option_ref(source, &errors);
        assert_eq!(result, source, "Should not modify non-Option<&mut> parameters");
    }

    // -----------------------------------------------------------------------
    // apply_all_rules tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_apply_all_rules_no_errors() {
        let source = "fn main() { println!(\"hello\"); }";
        let result = apply_all_rules(source, &[]);
        assert_eq!(result, source);
    }

    #[test]
    fn test_apply_all_rules_mixed() {
        let source = r#"fn helper(x: i32) -> i32 { x + 1 }
fn helper(x: i32) -> i32 { x * 2 }"#;

        let errors = vec![CompilerError {
            code: "E0428".to_string(),
            line: 2,
            message: "the name `helper` is defined multiple times".to_string(),
        }];

        let result = apply_all_rules(source, &errors);
        assert_eq!(
            result.matches("fn helper").count(),
            1,
            "Dedup should remove second definition"
        );
    }

    #[test]
    fn test_rule_strip_markdown_fences() {
        let source = "use std::io;\n\nfn foo() -> i32 { 1 }\n\n\
                       // --- Module: mz_p2 ---\n\
                       ```rust\n\
                       fn bar() -> i32 { 2 }\n\
                       ```\n\n\
                       fn baz() -> i32 { 3 }\n";
        let result = rule_strip_markdown_fences(source);
        assert!(
            !result.contains("```"),
            "fences should be stripped: {result}"
        );
        assert!(result.contains("fn foo()"), "code before fence preserved");
        assert!(result.contains("fn bar()"), "code inside fence preserved");
        assert!(result.contains("fn baz()"), "code after fence preserved");
    }

    #[test]
    fn test_rules_on_real_assembly() {
        let source = include_str!("../../../tests/fixtures/repair/assembly-iter05.rs");
        let compile_result = crate::compiler::check_rust_compiles(source).unwrap();
        assert!(!compile_result.success, "fixture should have errors");

        let errors = parse_rustc_errors(&compile_result.stderr);
        assert!(!errors.is_empty(), "should parse errors from fixture");

        let fixed = apply_all_rules(source, &errors);
        assert_ne!(source, &fixed, "rules should have modified the source");

        // Re-compile the fixed version and check error count reduced
        let fixed_result = crate::compiler::check_rust_compiles(&fixed).unwrap();
        let fixed_errors = parse_rustc_errors(&fixed_result.stderr);
        println!(
            "Rule engine: {} errors -> {} errors (reduced {})",
            errors.len(),
            fixed_errors.len(),
            errors.len() - fixed_errors.len()
        );
        assert!(
            fixed_errors.len() < errors.len(),
            "rules should reduce error count: {} -> {}",
            errors.len(),
            fixed_errors.len()
        );
    }

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
        assert_eq!(
            errors.len(),
            3,
            "should parse both syntax and coded errors: {errors:?}"
        );
        // Coded error
        let coded: Vec<_> = errors.iter().filter(|e| e.code == "E0432").collect();
        assert_eq!(coded.len(), 1);
        assert_eq!(coded[0].line, 11);
        // Syntax errors
        let syntax: Vec<_> = errors.iter().filter(|e| e.code == "SYNTAX").collect();
        assert_eq!(syntax.len(), 2, "should have 2 syntax errors: {errors:?}");
        assert!(syntax.iter().any(|e| e.message.contains("unknown start of token")));
        assert!(syntax.iter().any(|e| e.message.contains("unclosed delimiter")));
    }

    #[test]
    fn test_apply_all_rules_reduces_errors() {
        // Synthetic fixture exercising R2 (dedup) and R1 (Clone bounds)
        let source = r#"fn resize_array<T: Default>(arr: &mut Vec<T>, n: usize) {
    arr.resize(n, T::default());
}

fn resize_array<T>(arr: &mut Vec<T>, n: usize) {
    arr.resize(n, T::default());
}
"#;
        let compile_result = crate::compiler::check_rust_compiles(source).unwrap();
        assert!(!compile_result.success, "fixture should have compile errors");

        let errors = parse_rustc_errors(&compile_result.stderr);
        assert!(!errors.is_empty(), "should parse at least one error");

        let fixed = apply_all_rules(source, &errors);
        let fixed_result = crate::compiler::check_rust_compiles(&fixed).unwrap();
        let fixed_errors = parse_rustc_errors(&fixed_result.stderr);

        println!(
            "Multi-rule test: {} errors -> {} errors",
            errors.len(),
            fixed_errors.len()
        );
        assert!(
            fixed_errors.len() < errors.len(),
            "should reduce errors: {} -> {}",
            errors.len(),
            fixed_errors.len()
        );
    }
}
