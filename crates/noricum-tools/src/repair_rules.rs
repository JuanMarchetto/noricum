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
            if ch == '/' && chars.peek() == Some(&'/') {
                break;
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

/// Fix unclosed braces in truncated Rust source by truncating at the last balanced point.
///
/// Strategy: scan through the source tracking brace depth. Remember the last line
/// where depth returned to 0 (i.e., a complete top-level item ended). If the source
/// has unclosed braces, truncate everything after that last balanced point.
/// This removes incomplete function bodies rather than blindly appending `}`.
///
/// Falls back to appending `}` only if no balanced point is found (e.g., the very
/// first function is truncated).
pub fn auto_close_braces(source: &str) -> String {
    let depth = check_brace_balance(source);
    if depth <= 0 {
        return source.to_string();
    }

    // Find the last line where cumulative brace depth was 0
    let lines: Vec<&str> = source.lines().collect();
    let mut running_depth: i32 = 0;
    let mut last_balanced_line: Option<usize> = None;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            // Line comments don't affect depth; if we're at 0, this is still balanced
            if running_depth == 0 {
                last_balanced_line = Some(i);
            }
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
            if ch == '/' && chars.peek() == Some(&'/') {
                break;
            }
            match ch {
                '{' => running_depth += 1,
                '}' => running_depth -= 1,
                _ => {}
            }
        }

        if running_depth == 0 {
            last_balanced_line = Some(i);
        }
    }

    if let Some(cut_line) = last_balanced_line
        && cut_line + 1 < lines.len()
    {
        let truncated_count = lines.len() - cut_line - 1;
        debug!(
            cut_line = cut_line + 1,
            truncated_lines = truncated_count,
            "P32: truncating at last balanced brace point"
        );
        let kept: String = lines[..=cut_line].join("\n");
        return format!("{kept}\n// P32: truncated {truncated_count} lines of incomplete code");
    }

    // Fallback: no balanced point found, just close braces
    let closes = "}".repeat(depth as usize);
    format!("{source}\n{closes} // auto-closed: truncated output")
}

/// Apply all mechanical repair rules in sequence.
///
/// Order matters: R0 fence strip, R5 inner attributes, R6 inner doc comments,
/// R7 duplicate use imports, R8 orphaned derive, R2 dedup (removes duplicate definitions),
/// R9 conflicting trait impls, R10 external crate imports, R11 windows imports,
/// R1 clone bounds (adds missing trait bounds), R3 mut option ref
/// (fixes moved `Option<&mut T>` parameters), R4 auto-close braces
/// (safety net for truncated LLM output).
pub fn apply_all_rules(source: &str, errors: &[CompilerError]) -> String {
    // R0 first: strip markdown fences (always, no error check needed)
    let mut result = rule_strip_markdown_fences(source);

    // R5: inner attributes → outer attributes (must come before compilation-sensitive rules)
    result = rule_inner_attribute_to_outer(&result, errors);
    // R6: inner doc comments → regular comments
    result = rule_inner_doc_to_comment(&result, errors);
    // R7: duplicate use imports
    result = rule_dedup_use_imports(&result, errors);
    // R8: orphaned derive on non-struct
    result = rule_orphaned_derive(&result, errors);
    // R2: dedup removes duplicate definitions, which can cascade
    result = rule_dedup_functions(&result, errors);
    // R9: conflicting trait implementations
    result = rule_conflicting_trait_impls(&result, errors);
    // R10: unresolved external crate imports
    result = rule_strip_external_crate_imports(&result, errors);
    // R11: windows-specific imports on non-windows
    result = rule_strip_windows_imports(&result, errors);
    // R1: add Clone bounds where needed
    result = rule_clone_bounds(&result, errors);
    // R3: fix Option<&mut T> move errors
    result = rule_mut_option_ref(&result, errors);

    // R12: fix truncated functions at module boundaries (must come before R4)
    result = rule_fix_truncated_module_boundary(&result);
    // R13: field name prefix normalization (m_xyz ↔ xyz)
    result = rule_field_name_prefix(&result, errors);
    // R14: free function vs method call fix
    result = rule_method_to_free_fn(&result, errors);

    // R4: auto-close unclosed braces from truncated LLM output
    result = auto_close_braces(&result);

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

// ---------------------------------------------------------------------------
// R5: Inner attributes → outer attributes
// ---------------------------------------------------------------------------

/// R5: When an inner attribute (`#![allow(...)]`) appears outside the crate root
/// (line > 5), convert it to an outer attribute (`#[allow(...)]`).
///
/// Error: `an inner attribute is not permitted in this context`
pub fn rule_inner_attribute_to_outer(source: &str, errors: &[CompilerError]) -> String {
    let inner_attr_errors: Vec<&CompilerError> = errors
        .iter()
        .filter(|e| {
            e.message
                .contains("inner attribute is not permitted in this context")
        })
        .collect();

    if inner_attr_errors.is_empty() {
        return source.to_string();
    }

    let error_lines: Vec<usize> = inner_attr_errors.iter().map(|e| e.line).collect();

    let lines: Vec<&str> = source.lines().collect();
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len());

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1; // 1-indexed
        if error_lines.contains(&line_num) && line_num > 5 {
            let trimmed = line.trim();
            if trimmed.starts_with("#![") {
                // Replace #![ with #[
                let fixed = line.replacen("#![", "#[", 1);
                debug!(line = line_num, "R5: converted inner attribute to outer");
                result_lines.push(fixed);
                continue;
            }
        }
        result_lines.push(line.to_string());
    }

    result_lines.join("\n")
}

// ---------------------------------------------------------------------------
// R6: Inner doc comments → regular comments
// ---------------------------------------------------------------------------

/// R6: When an inner doc comment (`//!`) appears outside the crate root,
/// convert it to a regular comment (`//`).
///
/// Error: `E0753: expected outer doc comment`
pub fn rule_inner_doc_to_comment(source: &str, errors: &[CompilerError]) -> String {
    let doc_errors: Vec<&CompilerError> = errors
        .iter()
        .filter(|e| e.code == "E0753" || e.message.contains("expected outer doc comment"))
        .collect();

    if doc_errors.is_empty() {
        return source.to_string();
    }

    let error_lines: Vec<usize> = doc_errors.iter().map(|e| e.line).collect();

    let lines: Vec<&str> = source.lines().collect();
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len());

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;
        if error_lines.contains(&line_num) && line_num > 5 {
            let trimmed = line.trim();
            if trimmed.starts_with("//!") {
                // Replace //! with // preserving leading whitespace
                let fixed = line.replacen("//!", "//", 1);
                debug!(line = line_num, "R6: converted inner doc comment to regular comment");
                result_lines.push(fixed);
                continue;
            }
        }
        result_lines.push(line.to_string());
    }

    result_lines.join("\n")
}

// ---------------------------------------------------------------------------
// R7: Duplicate use imports
// ---------------------------------------------------------------------------

/// R7: When `E0252` reports "the name X is defined multiple times" from
/// duplicate `use` statements, remove the second `use` line that imports
/// the same name.
///
/// Also handles `use std::io;` duplicated after `use std::io::{self, ...}`.
pub fn rule_dedup_use_imports(source: &str, errors: &[CompilerError]) -> String {
    let dup_errors: Vec<&CompilerError> = errors
        .iter()
        .filter(|e| e.code == "E0252" && e.message.contains("defined multiple times"))
        .collect();

    if dup_errors.is_empty() {
        return source.to_string();
    }

    // Extract the duplicate names from error messages
    let name_re =
        Regex::new(r"name `(\w+)` is defined multiple times").expect("static regex is valid");

    let mut dup_names: Vec<String> = Vec::new();
    for err in &dup_errors {
        if let Some(caps) = name_re.captures(&err.message) {
            let name = caps[1].to_string();
            if !dup_names.contains(&name) {
                dup_names.push(name);
            }
        }
    }

    if dup_names.is_empty() {
        return source.to_string();
    }

    let lines: Vec<&str> = source.lines().collect();
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len());
    let mut seen_imports: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Only process `use` statements
        if trimmed.starts_with("use ") || trimmed.starts_with("pub use ") {
            // Check if this use line imports any of the duplicated names
            let mut is_dup = false;
            for name in &dup_names {
                // Match patterns like: `use foo::Name;` or `use foo::Name as ...;`
                // or `use foo::{self, ...}` where foo ends with the module name
                if trimmed.contains(&format!("::{name}")) || trimmed.ends_with(&format!("{name};")) {
                    // Use the full line (normalized) as key
                    let key = format!("{name}:{trimmed}");
                    if seen_imports.contains(&format!("{name}:SEEN")) {
                        // This is a duplicate import of the same name — remove it
                        debug!(
                            line = i + 1,
                            name,
                            "R7: removing duplicate use import"
                        );
                        is_dup = true;
                        break;
                    }
                    seen_imports.insert(format!("{name}:SEEN"));
                    let _ = key;
                }
            }
            if is_dup {
                continue;
            }
        }

        result_lines.push(line.to_string());
    }

    result_lines.join("\n")
}

// ---------------------------------------------------------------------------
// R8: Orphaned derive on non-struct
// ---------------------------------------------------------------------------

/// R8: When `E0774` reports "derive may only be applied to structs, enums and unions",
/// remove the `#[derive(...)]` line if the following non-blank line is not a
/// struct/enum/union declaration.
pub fn rule_orphaned_derive(source: &str, errors: &[CompilerError]) -> String {
    let derive_errors: Vec<&CompilerError> = errors
        .iter()
        .filter(|e| {
            e.code == "E0774"
                || e.message
                    .contains("derive may only be applied to structs, enums and unions")
        })
        .collect();

    if derive_errors.is_empty() {
        return source.to_string();
    }

    let error_lines: Vec<usize> = derive_errors.iter().map(|e| e.line).collect();

    let lines: Vec<&str> = source.lines().collect();
    let mut remove_lines: Vec<usize> = Vec::new();

    let derive_re = Regex::new(r"^\s*#\[derive\(").expect("static regex is valid");

    for &line_num in &error_lines {
        let idx = line_num.saturating_sub(1);
        if idx >= lines.len() {
            continue;
        }
        if derive_re.is_match(lines[idx]) {
            // Check the next non-blank line
            let mut next_idx = idx + 1;
            while next_idx < lines.len() && lines[next_idx].trim().is_empty() {
                next_idx += 1;
            }
            let is_valid_target = if next_idx < lines.len() {
                let next_trimmed = lines[next_idx].trim();
                next_trimmed.starts_with("struct ")
                    || next_trimmed.starts_with("pub struct ")
                    || next_trimmed.starts_with("pub(crate) struct ")
                    || next_trimmed.starts_with("enum ")
                    || next_trimmed.starts_with("pub enum ")
                    || next_trimmed.starts_with("pub(crate) enum ")
                    || next_trimmed.starts_with("union ")
                    || next_trimmed.starts_with("pub union ")
                    || next_trimmed.starts_with("pub(crate) union ")
            } else {
                false
            };

            if !is_valid_target {
                remove_lines.push(idx);
                debug!(line = line_num, "R8: removing orphaned #[derive(...)]");
            }
        }
    }

    if remove_lines.is_empty() {
        return source.to_string();
    }

    let result_lines: Vec<String> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !remove_lines.contains(i))
        .map(|(_, line)| line.to_string())
        .collect();

    result_lines.join("\n")
}

// ---------------------------------------------------------------------------
// R9: Conflicting trait implementations
// ---------------------------------------------------------------------------

/// R9: When `E0119` reports "conflicting implementations of trait X for type Y",
/// remove the second `impl` block entirely.
pub fn rule_conflicting_trait_impls(source: &str, errors: &[CompilerError]) -> String {
    let conflict_errors: Vec<&CompilerError> = errors
        .iter()
        .filter(|e| e.code == "E0119" && e.message.contains("conflicting implementations"))
        .collect();

    if conflict_errors.is_empty() {
        return source.to_string();
    }

    // Extract trait and type names from error messages
    // Message format: "conflicting implementations of trait `Debug` for type `Foo`"
    let impl_re = Regex::new(r"conflicting implementations of trait `(\w+)` for type `([^`]+)`")
        .expect("static regex is valid");

    let mut to_remove: Vec<(String, String, usize)> = Vec::new(); // (trait, type, error_line)
    for err in &conflict_errors {
        if let Some(caps) = impl_re.captures(&err.message) {
            to_remove.push((caps[1].to_string(), caps[2].to_string(), err.line));
        }
    }

    if to_remove.is_empty() {
        return source.to_string();
    }

    let lines: Vec<&str> = source.lines().collect();
    let mut removed_ranges: Vec<(usize, usize)> = Vec::new();

    for (trait_name, type_name, error_line) in &to_remove {
        // The error line points to the second impl. Build pattern to match it.
        // The error message uses unqualified names (e.g., `Debug`) but the source
        // may use qualified paths (e.g., `fmt::Debug`), so we match with optional
        // path prefix: `(path::)*TraitName`.
        let impl_pattern = format!(
            r"^\s*impl\s+(?:\w+::)*{}\s+for\s+(?:\w+::)*{}",
            regex::escape(trait_name),
            regex::escape(type_name)
        );
        let impl_match_re = Regex::new(&impl_pattern).expect("dynamic regex is valid");

        // Search near the error line for the impl declaration
        let search_start = error_line.saturating_sub(3); // error might be 1-2 lines after impl
        let search_end = (error_line + 2).min(lines.len());

        let mut found_first = false;
        for i in 0..lines.len() {
            if impl_match_re.is_match(lines[i].trim()) {
                if !found_first {
                    // Skip the first occurrence (keep it)
                    found_first = true;
                    continue;
                }
                // Check if this second occurrence is near the error line
                let line_num = i + 1;
                if line_num >= search_start && line_num <= search_end + 5 {
                    let end = skip_function_body(&lines, i);
                    removed_ranges.push((i, end));
                    debug!(
                        trait_name,
                        type_name,
                        start = i + 1,
                        end,
                        "R9: removing conflicting trait impl"
                    );
                    break;
                }
            }
        }
    }

    if removed_ranges.is_empty() {
        return source.to_string();
    }

    removed_ranges.sort_by_key(|&(start, _)| start);

    let mut result_lines: Vec<String> = Vec::new();
    let mut skip_until = 0usize;
    for (i, line) in lines.iter().enumerate() {
        if i < skip_until {
            continue;
        }
        if let Some(&(_, end)) = removed_ranges.iter().find(|&&(s, _)| s == i) {
            skip_until = end;
            continue;
        }
        result_lines.push(line.to_string());
    }

    result_lines.join("\n")
}

// ---------------------------------------------------------------------------
// R10: Unresolved external crate imports
// ---------------------------------------------------------------------------

/// R10: When `E0432` or `E0433` reports unresolved imports for known external
/// crates (e.g., `miniz_oxide`, `crc32fast`), comment out the `use` line.
///
/// This is a conservative fix — only strips the import line, not code that
/// references the missing types.
pub fn rule_strip_external_crate_imports(source: &str, errors: &[CompilerError]) -> String {
    // Known external crates that won't be available in single-file compilation
    let external_crates = [
        "miniz_oxide",
        "crc32fast",
        "flate2",
        "libc",
        "nix",
        "winapi",
        "num_traits",
        "byteorder",
        "rand",
        "serde",
        "tokio",
    ];

    let unresolved_errors: Vec<&CompilerError> = errors
        .iter()
        .filter(|e| {
            (e.code == "E0432" || e.code == "E0433")
                && (e.message.contains("unresolved") || e.message.contains("could not find"))
        })
        .collect();

    let error_lines: Vec<usize> = unresolved_errors.iter().map(|e| e.line).collect();

    let lines: Vec<&str> = source.lines().collect();
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len());
    let mut changed = false;

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;
        let trimmed = line.trim();

        // Check if this is a use line for an external crate on an error line
        if !error_lines.is_empty()
            && error_lines.contains(&line_num)
            && (trimmed.starts_with("use ") || trimmed.starts_with("pub use "))
        {
            let is_external = external_crates
                .iter()
                .any(|crate_name| trimmed.contains(crate_name));

            if is_external {
                debug!(line = line_num, "R10: commenting out external crate import");
                result_lines.push(format!("// R10: {trimmed}"));
                changed = true;
                continue;
            }
        }

        // Always strip `extern crate` lines for known external crates
        // (these are never valid in single-file compilation)
        if trimmed.starts_with("extern crate ") {
            let is_external = external_crates
                .iter()
                .any(|crate_name| trimmed.contains(crate_name));
            if is_external {
                debug!(line = i + 1, "R10: commenting out extern crate");
                result_lines.push(format!("// R10: {trimmed}"));
                changed = true;
                continue;
            }
        }

        result_lines.push(line.to_string());
    }

    if !changed {
        return source.to_string();
    }

    result_lines.join("\n")
}

// ---------------------------------------------------------------------------
// R11: Windows-specific imports on non-Windows
// ---------------------------------------------------------------------------

/// R11: When `E0433` reports "could not find `windows` in `os`", strip
/// Strip Windows-specific code: `use std::os::windows::*` imports AND
/// `#[cfg(target_os = "windows")]` blocks (functions, impls, structs).
///
/// On non-Windows targets, these cause compilation failures.
pub fn rule_strip_windows_imports(source: &str, errors: &[CompilerError]) -> String {
    let windows_errors: Vec<&CompilerError> = errors
        .iter()
        .filter(|e| {
            (e.code == "E0432" || e.code == "E0433" || e.code == "SYNTAX")
                && (e.message.contains("windows") || e.message.contains("FILE_SHARE"))
        })
        .collect();

    if windows_errors.is_empty() {
        return source.to_string();
    }

    let lines: Vec<&str> = source.lines().collect();
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Strip Windows use imports
        if trimmed.starts_with("use std::os::windows")
            || trimmed.starts_with("pub use std::os::windows")
        {
            debug!(line = i + 1, "R11: commenting out Windows-specific import");
            result_lines.push(format!("// R11 (non-Windows): {trimmed}"));
            i += 1;
            continue;
        }

        // Strip #[cfg(target_os = "windows")] + the following item (block)
        if trimmed == "#[cfg(target_os = \"windows\")]"
            || trimmed == "#[cfg(windows)]"
        {
            debug!(line = i + 1, "R11: stripping Windows-specific cfg block");
            result_lines.push(format!("// R11 (non-Windows): {trimmed}"));
            i += 1;
            // Skip the following item (could be a function, struct, impl with braces)
            if i < lines.len() {
                let mut depth = 0;
                let mut found_brace = false;
                while i < lines.len() {
                    let l = lines[i];
                    for ch in l.chars() {
                        if ch == '{' { depth += 1; found_brace = true; }
                        if ch == '}' { depth -= 1; }
                    }
                    result_lines.push(format!("// R11: {}", l.trim()));
                    i += 1;
                    if found_brace && depth <= 0 {
                        break;
                    }
                }
            }
            continue;
        }

        result_lines.push(lines[i].to_string());
        i += 1;
    }

    result_lines.join("\n")
}

// ---------------------------------------------------------------------------
// R12: Fix truncated functions at module boundaries
// ---------------------------------------------------------------------------

/// Detect functions truncated at module boundaries (pattern: `pub fn name(\n...\n// --- Module:`)
/// and close them with a stub body.
///
/// This recurring issue happens when module mz_p7's last function gets cut off
/// at the `// --- Module: mz_p8 ---` boundary during assembly.
pub fn rule_fix_truncated_module_boundary(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Detect module boundary marker
        if trimmed.starts_with("// --- Module:") || trimmed.starts_with("// ---Module:") {
            // Look backwards for an incomplete function signature (has `(` but no `{`)
            let mut j = i.saturating_sub(1);
            let mut found_incomplete = false;
            while j > 0 && j > i.saturating_sub(10) {
                let prev = lines[j].trim();
                // Skip empty lines and comments
                if prev.is_empty() || prev.starts_with("//") {
                    j -= 1;
                    continue;
                }
                // Check if this is a function parameter or signature line without `{`
                if (prev.contains("pub fn ") || prev.contains("fn "))
                    && !prev.contains('{')
                {
                    found_incomplete = true;
                    break;
                }
                // If we hit a line that ends with `,` it's likely a parameter continuation
                if prev.ends_with(',') {
                    j -= 1;
                    continue;
                }
                // If we see a line with types but no braces, could be return type
                if !prev.contains('{') && !prev.contains('}') && !prev.ends_with(';') {
                    j -= 1;
                    continue;
                }
                break;
            }

            if found_incomplete {
                // Remove the incomplete function lines (from fn signature to here)
                // and replace with a stub
                let fn_line = lines[j].trim();
                // Extract function name for the stub
                let fn_name = fn_line
                    .split('(')
                    .next()
                    .unwrap_or(fn_line)
                    .trim();
                debug!(line = j + 1, "R12: fixing truncated function at module boundary");

                // Remove lines from j to i-1 (the incomplete function)
                while result_lines.len() > j {
                    result_lines.pop();
                }
                // Add stub
                result_lines.push(format!("// R12: truncated function stubbed at module boundary"));
                result_lines.push(format!("{fn_name}() -> bool {{ false }}"));
                result_lines.push(String::new());
            }
        }

        result_lines.push(lines[i].to_string());
        i += 1;
    }

    result_lines.join("\n")
}

// ---------------------------------------------------------------------------
// R13: Field name prefix normalization
// ---------------------------------------------------------------------------

/// When E0609 ("no field `m_xyz` on type") appears, try stripping the `m_` prefix
/// or adding it. Common pattern: contract defines `zip64` but module uses `m_zip64`.
pub fn rule_field_name_prefix(source: &str, errors: &[CompilerError]) -> String {
    let field_errors: Vec<&CompilerError> = errors
        .iter()
        .filter(|e| e.code == "E0609")
        .collect();

    if field_errors.is_empty() {
        return source.to_string();
    }

    let mut result = source.to_string();

    for err in &field_errors {
        // Extract field name from "no field `m_xyz` on type `Foo`"
        let field_re = regex::Regex::new(r"no field `(\w+)` on type").ok();
        if let Some(re) = field_re {
            if let Some(caps) = re.captures(&err.message) {
                let bad_field = &caps[1];

                // Try stripping m_ prefix
                if let Some(stripped) = bad_field.strip_prefix("m_") {
                    // Only fix on the error line (±2 lines for safety)
                    let lines: Vec<&str> = result.lines().collect();
                    if err.line > 0 && (err.line as usize) <= lines.len() {
                        let line_idx = err.line as usize - 1;
                        let old_pattern = format!(".{bad_field}");
                        let new_pattern = format!(".{stripped}");

                        let start = line_idx.saturating_sub(1);
                        let end = (line_idx + 2).min(lines.len());

                        let mut new_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
                        for i in start..end {
                            if new_lines[i].contains(&old_pattern) {
                                new_lines[i] = new_lines[i].replace(&old_pattern, &new_pattern);
                                debug!(line = i + 1, from = %bad_field, to = %stripped, "R13: stripped m_ prefix");
                            }
                        }
                        result = new_lines.join("\n");
                    }
                }
                // Try adding m_ prefix
                else if !bad_field.starts_with("m_") {
                    let with_prefix = format!("m_{bad_field}");
                    let lines: Vec<&str> = result.lines().collect();
                    if err.line > 0 && (err.line as usize) <= lines.len() {
                        let line_idx = err.line as usize - 1;
                        let old_pattern = format!(".{bad_field}");
                        let new_pattern = format!(".{with_prefix}");

                        let start = line_idx.saturating_sub(1);
                        let end = (line_idx + 2).min(lines.len());

                        let mut new_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
                        for i in start..end {
                            if new_lines[i].contains(&old_pattern) {
                                new_lines[i] = new_lines[i].replace(&old_pattern, &new_pattern);
                                debug!(line = i + 1, from = %bad_field, to = %with_prefix, "R13: added m_ prefix");
                            }
                        }
                        result = new_lines.join("\n");
                    }
                }
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// R14: Free function vs method call fix
// ---------------------------------------------------------------------------

/// When E0599 ("no method named `xyz` found for struct `Foo`") appears and a free
/// function `xyz` exists at module scope, rewrite `self.xyz(args)` → `xyz(self, args)`
/// or `Self::xyz(args)` → `xyz(args)`.
pub fn rule_method_to_free_fn(source: &str, errors: &[CompilerError]) -> String {
    let method_errors: Vec<&CompilerError> = errors
        .iter()
        .filter(|e| e.code == "E0599" && e.message.contains("no method named"))
        .collect();

    if method_errors.is_empty() {
        return source.to_string();
    }

    // Collect all free function names at module scope
    let free_fns: std::collections::HashSet<String> = source
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if (trimmed.starts_with("pub fn ") || trimmed.starts_with("fn "))
                && !trimmed.starts_with("pub fn new")
                && trimmed.contains('(')
            {
                let name = trimmed
                    .split('(')
                    .next()?
                    .trim()
                    .rsplit(' ')
                    .next()?
                    .to_string();
                Some(name)
            } else {
                None
            }
        })
        .collect();

    let mut result = source.to_string();

    for err in &method_errors {
        // Extract method name: "no method named `xyz` found"
        let method_re = regex::Regex::new(r"no method named `(\w+)` found").ok();
        if let Some(re) = method_re {
            if let Some(caps) = re.captures(&err.message) {
                let method_name = &caps[1];

                // Check if there's a free function with this name
                if free_fns.contains(method_name) {
                    let lines: Vec<&str> = result.lines().collect();
                    if err.line > 0 && (err.line as usize) <= lines.len() {
                        let line_idx = err.line as usize - 1;
                        let mut new_lines: Vec<String> =
                            lines.iter().map(|l| l.to_string()).collect();

                        let line = &new_lines[line_idx];

                        // Pattern: self.method_name(args) → method_name(self, args)
                        let self_call = format!("self.{method_name}(");
                        if line.contains(&self_call) {
                            new_lines[line_idx] =
                                line.replace(&self_call, &format!("{method_name}(self, "));
                            debug!(line = line_idx + 1, method = %method_name, "R14: self.method → free_fn(self)");
                        }

                        // Pattern: Self::method_name(args) → method_name(args)
                        let assoc_call = format!("Self::{method_name}(");
                        if new_lines[line_idx].contains(&assoc_call) {
                            new_lines[line_idx] = new_lines[line_idx]
                                .replace(&assoc_call, &format!("{method_name}("));
                            debug!(line = line_idx + 1, method = %method_name, "R14: Self::method → free_fn");
                        }

                        result = new_lines.join("\n");
                    }
                }
            }
        }
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
    // R12 tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_r12_fixes_truncated_function_at_module_boundary() {
        let input = r#"pub fn good_fn() -> bool {
    true
}

/// Adds a C file to a zip archive
pub fn mz_zip_writer_add_cfile(
    p_zip: &mut MzZipArchive,
    p_archive_name: &str,
    p_src_file: Option<std::fs::File>,

// --- Module: mz_p8 ---
pub fn next_fn() -> bool {
    false
}"#;
        let result = rule_fix_truncated_module_boundary(input);
        assert!(result.contains("R12: truncated function"));
        assert!(result.contains("// --- Module: mz_p8 ---"));
        assert!(result.contains("pub fn next_fn"));
        // The truncated function should be replaced with a stub
        assert!(!result.contains("p_archive_name"));
    }

    #[test]
    fn test_r12_no_false_positive() {
        let input = r#"pub fn complete_fn(x: i32) -> bool {
    x > 0
}

// --- Module: mz_p2 ---
pub fn other_fn() -> bool {
    true
}"#;
        let result = rule_fix_truncated_module_boundary(input);
        // Complete function before boundary should not be modified
        assert!(result.contains("pub fn complete_fn(x: i32) -> bool {"));
    }

    // R13 tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_r13_strips_m_prefix() {
        let source = "fn foo(zip: &Zip) {\n    let x = zip.m_zip64;\n}\n";
        let errors = vec![CompilerError {
            code: "E0609".to_string(),
            message: "no field `m_zip64` on type `Zip`".to_string(),
            line: 2,
        }];
        let result = rule_field_name_prefix(source, &errors);
        assert!(result.contains("zip.zip64"), "m_ prefix should be stripped");
        assert!(!result.contains("zip.m_zip64"), "original should be gone");
    }

    #[test]
    fn test_r13_adds_m_prefix() {
        let source = "fn foo(zip: &Zip) {\n    let x = zip.archive_size;\n}\n";
        let errors = vec![CompilerError {
            code: "E0609".to_string(),
            message: "no field `archive_size` on type `Zip`".to_string(),
            line: 2,
        }];
        let result = rule_field_name_prefix(source, &errors);
        assert!(result.contains("zip.m_archive_size"), "m_ prefix should be added");
    }

    #[test]
    fn test_r13_no_false_positive() {
        let source = "fn foo(zip: &Zip) {\n    let x = zip.valid_field;\n}\n";
        let result = rule_field_name_prefix(source, &[]);
        assert_eq!(result, source, "no errors = no changes");
    }

    // R14 tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_r14_self_method_to_free_fn() {
        let source = "fn mz_zip_set_error(zip: &mut Zip, e: i32) {}\n\nimpl Zip {\n    fn foo(&mut self) {\n        self.mz_zip_set_error(42);\n    }\n}\n";
        let errors = vec![CompilerError {
            code: "E0599".to_string(),
            message: "no method named `mz_zip_set_error` found for struct `Zip`".to_string(),
            line: 5,
        }];
        let result = rule_method_to_free_fn(source, &errors);
        assert!(result.contains("mz_zip_set_error(self, 42)"), "should rewrite to free fn call");
        assert!(!result.contains("self.mz_zip_set_error"), "method call should be gone");
    }

    #[test]
    fn test_r14_no_false_positive() {
        let source = "impl Zip {\n    fn foo(&self) {\n        self.real_method();\n    }\n}\n";
        let result = rule_method_to_free_fn(source, &[]);
        assert_eq!(result, source, "no errors = no changes");
    }

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
    fn test_apply_all_rules_fixes_braces() {
        // Two fns, second truncated — R4 should truncate the incomplete one
        let source = "fn foo() {\n    1\n}\n\nfn bar() {\n    let x = 1;";
        let result = apply_all_rules(source, &[]);
        assert_eq!(
            check_brace_balance(&result),
            0,
            "apply_all_rules should fix braces via R4"
        );
        assert!(result.contains("fn foo()"), "complete fn preserved");
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

    // -----------------------------------------------------------------------
    // check_brace_balance tests
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // auto_close_braces tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_auto_close_braces_balanced() {
        let source = "fn foo() {\n    1\n}";
        let result = auto_close_braces(source);
        assert_eq!(result, source, "balanced source should be unchanged");
    }

    #[test]
    fn test_auto_close_braces_truncates_at_last_balanced() {
        // Two functions, second one is truncated
        let source = "fn foo() {\n    1\n}\n\nfn bar() {\n    if true {\n        let x = 1;";
        let result = auto_close_braces(source);
        // Should keep foo() and truncate the incomplete bar()
        assert!(result.contains("fn foo()"), "complete fn preserved: {result}");
        assert!(!result.contains("fn bar()"), "incomplete fn truncated: {result}");
        assert!(result.contains("P32: truncated"), "truncation marker: {result}");
        assert_eq!(check_brace_balance(&result), 0, "result should be balanced");
    }

    #[test]
    fn test_auto_close_braces_fallback_when_first_fn_truncated() {
        // Only one function and it's truncated — no balanced point to cut at
        let source = "fn foo() {\n    let x = 1;";
        let result = auto_close_braces(source);
        // Falls back to appending }
        assert!(result.ends_with("} // auto-closed: truncated output"), "got: {result}");
        assert_eq!(check_brace_balance(&result), 0, "should be balanced after auto-close");
    }

    #[test]
    fn test_auto_close_braces_multiple_complete_then_truncated() {
        let source = "use std::io;\n\nfn a() {\n    1\n}\n\nfn b() {\n    2\n}\n\nfn c() {\n    if true {";
        let result = auto_close_braces(source);
        assert!(result.contains("fn a()"), "a preserved");
        assert!(result.contains("fn b()"), "b preserved");
        assert!(!result.contains("fn c()"), "incomplete c truncated");
        assert_eq!(check_brace_balance(&result), 0, "balanced");
    }

    #[test]
    fn test_auto_close_braces_extra_close() {
        let source = "fn foo() {\n    1\n}\n}";
        let result = auto_close_braces(source);
        assert_eq!(result, source, "extra closes should not be modified");
    }

    // -----------------------------------------------------------------------
    // R5: rule_inner_attribute_to_outer tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_r5_inner_attr_to_outer() {
        // Inner attribute on line 10 (> 5) should become outer
        let source = "use std::io;\n\
                       use std::fmt;\n\
                       use std::collections::HashMap;\n\
                       \n\
                       fn foo() { }\n\
                       \n\
                       #![allow(dead_code)]\n\
                       fn bar() { }";

        let errors = vec![CompilerError {
            code: "SYNTAX".to_string(),
            line: 7,
            message: "an inner attribute is not permitted in this context".to_string(),
        }];

        let result = rule_inner_attribute_to_outer(source, &errors);
        assert!(
            result.contains("#[allow(dead_code)]"),
            "inner attr should become outer: {result}"
        );
        assert!(
            !result.contains("#![allow(dead_code)]"),
            "inner attr should be gone: {result}"
        );
    }

    #[test]
    fn test_r5_inner_attr_at_crate_root_preserved() {
        // Inner attribute on line 1 (≤ 5) should NOT be changed
        let source = "#![allow(dead_code)]\n\nfn foo() { }";

        let errors = vec![CompilerError {
            code: "SYNTAX".to_string(),
            line: 1,
            message: "an inner attribute is not permitted in this context".to_string(),
        }];

        let result = rule_inner_attribute_to_outer(source, &errors);
        assert!(
            result.contains("#![allow(dead_code)]"),
            "crate-root inner attr should be preserved: {result}"
        );
    }

    #[test]
    fn test_r5_no_matching_errors() {
        let source = "#![allow(dead_code)]\nfn foo() { }";
        let result = rule_inner_attribute_to_outer(source, &[]);
        assert_eq!(result, source, "no errors = no changes");
    }

    // -----------------------------------------------------------------------
    // R6: rule_inner_doc_to_comment tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_r6_inner_doc_to_comment() {
        let source = "use std::io;\n\
                       use std::fmt;\n\
                       \n\
                       fn foo() { }\n\
                       \n\
                       //! This is a module doc\n\
                       fn bar() { }";

        let errors = vec![CompilerError {
            code: "E0753".to_string(),
            line: 6,
            message: "expected outer doc comment".to_string(),
        }];

        let result = rule_inner_doc_to_comment(source, &errors);
        assert!(
            result.contains("// This is a module doc"),
            "inner doc should become regular comment: {result}"
        );
        assert!(
            !result.contains("//!"),
            "inner doc marker should be gone: {result}"
        );
    }

    #[test]
    fn test_r6_at_crate_root_preserved() {
        let source = "//! Crate documentation\n\nfn foo() { }";

        let errors = vec![CompilerError {
            code: "E0753".to_string(),
            line: 1,
            message: "expected outer doc comment".to_string(),
        }];

        let result = rule_inner_doc_to_comment(source, &errors);
        assert!(
            result.contains("//! Crate documentation"),
            "crate-root inner doc should be preserved: {result}"
        );
    }

    // -----------------------------------------------------------------------
    // R7: rule_dedup_use_imports tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_r7_dedup_use_imports() {
        let source = "use std::io;\n\
                       use std::fmt;\n\
                       use std::io;";

        let errors = vec![CompilerError {
            code: "E0252".to_string(),
            line: 3,
            message: "the name `io` is defined multiple times".to_string(),
        }];

        let result = rule_dedup_use_imports(source, &errors);
        let io_count = result.lines().filter(|l| l.trim() == "use std::io;").count();
        assert_eq!(io_count, 1, "should have only one use std::io: {result}");
        assert!(
            result.contains("use std::fmt;"),
            "unrelated import preserved: {result}"
        );
    }

    #[test]
    fn test_r7_dedup_self_import() {
        let source = "use std::io::{self, Read, Write};\n\
                       use std::io;";

        let errors = vec![CompilerError {
            code: "E0252".to_string(),
            line: 2,
            message: "the name `io` is defined multiple times".to_string(),
        }];

        let result = rule_dedup_use_imports(source, &errors);
        assert!(
            result.contains("use std::io::{self, Read, Write};"),
            "more specific import kept: {result}"
        );
        let simple_io = result
            .lines()
            .filter(|l| l.trim() == "use std::io;")
            .count();
        assert_eq!(simple_io, 0, "duplicate simple import removed: {result}");
    }

    #[test]
    fn test_r7_no_matching_errors() {
        let source = "use std::io;\nuse std::fmt;";
        let result = rule_dedup_use_imports(source, &[]);
        assert_eq!(result, source, "no errors = no changes");
    }

    // -----------------------------------------------------------------------
    // R8: rule_orphaned_derive tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_r8_orphaned_derive_on_fn() {
        let source = "use std::io;\n\
                       \n\
                       fn foo() { }\n\
                       \n\
                       #[derive(Debug, Clone)]\n\
                       fn bar() -> i32 { 42 }\n\
                       \n\
                       #[derive(Debug)]\n\
                       struct Baz { x: i32 }";

        let errors = vec![CompilerError {
            code: "E0774".to_string(),
            line: 5,
            message: "derive may only be applied to structs, enums and unions".to_string(),
        }];

        let result = rule_orphaned_derive(source, &errors);
        // The derive on fn bar should be removed
        assert!(
            !result.contains("#[derive(Debug, Clone)]"),
            "orphaned derive should be removed: {result}"
        );
        // The derive on struct Baz should be kept
        assert!(
            result.contains("#[derive(Debug)]"),
            "valid derive should be kept: {result}"
        );
        assert!(result.contains("fn bar()"), "function itself preserved: {result}");
    }

    #[test]
    fn test_r8_orphaned_derive_on_const() {
        let source = "use std::io;\n\
                       \n\
                       fn placeholder() { }\n\
                       \n\
                       #[derive(Debug)]\n\
                       const X: i32 = 42;";

        let errors = vec![CompilerError {
            code: "E0774".to_string(),
            line: 5,
            message: "derive may only be applied to structs, enums and unions".to_string(),
        }];

        let result = rule_orphaned_derive(source, &errors);
        assert!(
            !result.contains("#[derive(Debug)]"),
            "derive on const should be removed: {result}"
        );
        assert!(result.contains("const X: i32 = 42;"), "const preserved: {result}");
    }

    #[test]
    fn test_r8_valid_derive_not_removed() {
        let source = "#[derive(Debug, Clone)]\nstruct Foo { x: i32 }";
        let result = rule_orphaned_derive(source, &[]);
        assert_eq!(result, source, "valid derive on struct should be unchanged");
    }

    // -----------------------------------------------------------------------
    // R9: rule_conflicting_trait_impls tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_r9_conflicting_trait_impls() {
        let source = r#"use std::fmt;

struct Foo { x: i32 }

impl fmt::Debug for Foo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Foo({})", self.x)
    }
}

impl fmt::Debug for Foo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Foo[{}]", self.x)
    }
}"#;

        let errors = vec![CompilerError {
            code: "E0119".to_string(),
            line: 11,
            message: "conflicting implementations of trait `Debug` for type `Foo`".to_string(),
        }];

        let result = rule_conflicting_trait_impls(source, &errors);
        let impl_count = result.matches("impl fmt::Debug for Foo").count();
        assert_eq!(impl_count, 1, "should have only one impl: {result}");
        // First definition should be kept
        assert!(
            result.contains("Foo({})"),
            "first impl should be kept: {result}"
        );
        assert!(
            !result.contains("Foo[{}]"),
            "second impl should be removed: {result}"
        );
    }

    #[test]
    fn test_r9_no_matching_errors() {
        let source = "impl Debug for Foo { fn fmt(&self, f: &mut Formatter) -> Result { Ok(()) } }";
        let result = rule_conflicting_trait_impls(source, &[]);
        assert_eq!(result, source, "no errors = no changes");
    }

    // -----------------------------------------------------------------------
    // R10: rule_strip_external_crate_imports tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_r10_strip_external_crate_import() {
        let source = "use std::io;\n\
                       use miniz_oxide::inflate;\n\
                       use crc32fast::Hasher;\n\
                       use std::collections::HashMap;";

        let errors = vec![
            CompilerError {
                code: "E0432".to_string(),
                line: 2,
                message: "unresolved import `miniz_oxide`".to_string(),
            },
            CompilerError {
                code: "E0432".to_string(),
                line: 3,
                message: "unresolved import `crc32fast`".to_string(),
            },
        ];

        let result = rule_strip_external_crate_imports(source, &errors);
        assert!(
            result.contains("// R10: use miniz_oxide::inflate;"),
            "miniz_oxide should be commented out: {result}"
        );
        assert!(
            result.contains("// R10: use crc32fast::Hasher;"),
            "crc32fast should be commented out: {result}"
        );
        assert!(
            result.contains("use std::io;"),
            "std imports preserved: {result}"
        );
        assert!(
            result.contains("use std::collections::HashMap;"),
            "std imports preserved: {result}"
        );
    }

    #[test]
    fn test_r10_extern_crate_stripped() {
        let source = "extern crate miniz_oxide;\nuse std::io;";
        let result = rule_strip_external_crate_imports(source, &[]);
        // extern crate lines for known crates are always stripped
        assert!(
            result.contains("// R10: extern crate miniz_oxide;"),
            "extern crate should be commented: {result}"
        );
    }

    #[test]
    fn test_r10_no_matching_errors() {
        let source = "use std::io;\nuse std::fmt;";
        let result = rule_strip_external_crate_imports(source, &[]);
        assert_eq!(result, source, "no external crates = no changes");
    }

    // -----------------------------------------------------------------------
    // R11: rule_strip_windows_imports tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_r11_strip_windows_imports() {
        let source = "use std::io;\n\
                       use std::os::windows::io::AsRawHandle;\n\
                       use std::collections::HashMap;";

        let errors = vec![CompilerError {
            code: "E0433".to_string(),
            line: 2,
            message: "could not find `windows` in `os`".to_string(),
        }];

        let result = rule_strip_windows_imports(source, &errors);
        assert!(
            result.contains("// R11 (non-Windows): use std::os::windows::io::AsRawHandle;"),
            "windows import should be commented: {result}"
        );
        assert!(
            result.contains("use std::io;"),
            "std imports preserved: {result}"
        );
        assert!(
            result.contains("use std::collections::HashMap;"),
            "std imports preserved: {result}"
        );
    }

    #[test]
    fn test_r11_no_matching_errors() {
        let source = "use std::os::windows::io::AsRawHandle;";
        let result = rule_strip_windows_imports(source, &[]);
        assert_eq!(result, source, "no windows errors = no changes");
    }

    #[test]
    fn test_r11_strips_cfg_windows_block() {
        let source = r#"fn good_fn() -> bool { true }

#[cfg(target_os = "windows")]
pub fn mz_fopen(p_filename: &str) -> Result<std::fs::File, std::io::Error> {
    let share_mode = FILE_SHARE_READ | FILE_SHARE_WRITE;
    options.open(&path)
}

#[cfg(not(target_os = "windows"))]
pub fn mz_fopen(p_filename: &str) -> Result<std::fs::File, std::io::Error> {
    std::fs::File::open(p_filename)
}"#;
        let errors = vec![CompilerError {
            code: "E0433".to_string(),
            message: "cannot find windows in os".to_string(),
            line: 4,
        }];
        let result = rule_strip_windows_imports(source, &errors);
        assert!(result.contains("fn good_fn"), "non-windows fn preserved");
        assert!(result.contains("R11: pub fn mz_fopen"), "windows fn commented out");
        assert!(result.contains("#[cfg(not(target_os"), "non-windows cfg preserved");
        // Windows code should be commented out (prefixed with // R11:)
        assert!(result.contains("// R11: let share_mode"), "windows code commented via R11 prefix");
    }

    // -----------------------------------------------------------------------
    // R5-R11 integration via apply_all_rules
    // -----------------------------------------------------------------------

    #[test]
    fn test_apply_all_rules_r5_r8_combined() {
        // Source with both an orphaned derive (R8) and an inner attribute (R5)
        let source = "use std::io;\n\
                       use std::fmt;\n\
                       \n\
                       fn placeholder() { }\n\
                       \n\
                       #![allow(unused)]\n\
                       #[derive(Debug)]\n\
                       fn broken() -> i32 { 42 }";

        let errors = vec![
            CompilerError {
                code: "SYNTAX".to_string(),
                line: 6,
                message: "an inner attribute is not permitted in this context".to_string(),
            },
            CompilerError {
                code: "E0774".to_string(),
                line: 7,
                message: "derive may only be applied to structs, enums and unions".to_string(),
            },
        ];

        let result = apply_all_rules(source, &errors);
        // R5: inner attr → outer
        assert!(
            result.contains("#[allow(unused)]"),
            "R5 should convert inner attr: {result}"
        );
        assert!(
            !result.contains("#![allow(unused)]"),
            "R5 should remove inner attr: {result}"
        );
        // R8: orphaned derive removed
        assert!(
            !result.contains("#[derive(Debug)]"),
            "R8 should remove orphaned derive: {result}"
        );
        // Function preserved
        assert!(result.contains("fn broken()"), "function preserved: {result}");
    }
}
