//! Surgical repair utilities for targeted function-level editing.
//!
//! Instead of re-translating an entire file when a compiler error occurs,
//! these utilities extract the failing function, gather its type context,
//! and splice a repaired version back into the source.

use regex::Regex;

/// Extract the function containing `target_line` (1-indexed).
///
/// Searches backwards from the target line for a `fn` declaration, then
/// forward for the balanced closing brace. Returns `(function_name, function_body)`
/// or `None` if the line is not inside any function.
pub fn extract_function_at_line(source: &str, target_line: usize) -> Option<(String, String)> {
    let lines: Vec<&str> = source.lines().collect();
    if target_line == 0 || target_line > lines.len() {
        return None;
    }

    let fn_re = Regex::new(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)").ok()?;

    // Search backwards from target_line (1-indexed) for a fn declaration.
    let target_idx = target_line - 1;
    let mut fn_start_idx = None;
    let mut fn_name = String::new();

    for i in (0..=target_idx).rev() {
        if let Some(caps) = fn_re.captures(lines[i]) {
            fn_name = caps[1].to_string();
            fn_start_idx = Some(i);
            break;
        }
    }

    let fn_start_idx = fn_start_idx?;

    // Search forward from fn_start for balanced closing brace.
    let mut brace_depth = 0i32;
    let mut found_open = false;
    let mut fn_end_idx = fn_start_idx;

    for i in fn_start_idx..lines.len() {
        for ch in lines[i].chars() {
            if ch == '{' {
                brace_depth += 1;
                found_open = true;
            } else if ch == '}' {
                brace_depth -= 1;
            }
        }
        if found_open && brace_depth == 0 {
            fn_end_idx = i;
            break;
        }
    }

    // The target line must be within the function body.
    if target_idx > fn_end_idx {
        return None;
    }

    let body: String = lines[fn_start_idx..=fn_end_idx].join("\n");
    Some((fn_name, body))
}

/// Gather type and sibling-function context relevant to `fn_name`.
///
/// Scans the named function for capitalized type references, then extracts
/// matching `struct` and `enum` definitions from the source. Also extracts
/// signatures (without bodies) of sibling functions that are called from
/// the target function.
pub fn gather_context(source: &str, fn_name: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();

    // Step 1: Find the target function body.
    let fn_re = Regex::new(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)").expect("valid regex");
    let mut fn_start = None;
    let mut fn_end = None;

    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = fn_re.captures(line) {
            if &caps[1] == fn_name {
                fn_start = Some(i);
                break;
            }
        }
    }

    let fn_start = match fn_start {
        Some(s) => s,
        None => return String::new(),
    };

    // Find closing brace for this function.
    let mut brace_depth = 0i32;
    let mut found_open = false;
    for i in fn_start..lines.len() {
        for ch in lines[i].chars() {
            if ch == '{' {
                brace_depth += 1;
                found_open = true;
            } else if ch == '}' {
                brace_depth -= 1;
            }
        }
        if found_open && brace_depth == 0 {
            fn_end = Some(i);
            break;
        }
    }

    let fn_end = match fn_end {
        Some(e) => e,
        None => return String::new(),
    };

    let fn_body: String = lines[fn_start..=fn_end].join("\n");

    // Step 2: Extract capitalized type names referenced in the function body.
    let type_re = Regex::new(r"\b([A-Z]\w+)\b").expect("valid regex");
    let mut referenced_types: Vec<String> = type_re
        .captures_iter(&fn_body)
        .map(|c| c[1].to_string())
        .collect();
    referenced_types.sort();
    referenced_types.dedup();

    // Filter out common Rust built-in types and keywords.
    let builtins = [
        "Some", "None", "Ok", "Err", "Self", "String", "Vec", "Box", "Option", "Result",
        "HashMap", "HashSet", "BTreeMap", "Arc", "Rc", "Mutex", "RefCell", "Cow", "Pin",
        "Future", "Iterator", "Display", "Debug", "Clone", "Copy", "Default", "From", "Into",
        "Send", "Sync", "Sized", "Unpin", "Drop", "Fn", "FnMut", "FnOnce", "Ord", "Eq",
        "PartialOrd", "PartialEq", "Hash", "Serialize", "Deserialize",
    ];
    referenced_types.retain(|t| !builtins.contains(&t.as_str()));

    // Step 3: Extract matching struct/enum definitions.
    let mut context_parts: Vec<String> = Vec::new();
    let struct_enum_re =
        Regex::new(r"^\s*(?:pub\s+)?(?:struct|enum)\s+(\w+)").expect("valid regex");

    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = struct_enum_re.captures(line) {
            let name = &caps[1];
            if referenced_types.contains(&name.to_string()) {
                // Extract the full definition (with braces).
                let mut depth = 0i32;
                let mut opened = false;
                let mut end = i;

                for j in i..lines.len() {
                    for ch in lines[j].chars() {
                        if ch == '{' {
                            depth += 1;
                            opened = true;
                        } else if ch == '}' {
                            depth -= 1;
                        }
                    }
                    if opened && depth == 0 {
                        end = j;
                        break;
                    }
                }

                let def: String = lines[i..=end].join("\n");
                context_parts.push(def);
            }
        }
    }

    // Step 4: Extract signatures of sibling functions called from the target function.
    // Find all function names called in the body: identifier followed by `(`.
    let call_re = Regex::new(r"\b(\w+)\s*\(").expect("valid regex");
    let mut called_fns: Vec<String> = call_re
        .captures_iter(&fn_body)
        .map(|c| c[1].to_string())
        .collect();
    called_fns.sort();
    called_fns.dedup();
    // Remove the target function itself and common keywords/macros.
    let keywords = [
        "if", "for", "while", "match", "loop", "return", "let", "mut", "fn", "pub", "use",
        "impl", "struct", "enum", "type", "where", "as", "in", "ref", "self", "super", "crate",
        "mod", "const", "static", "unsafe", "extern", "async", "await", "move", "println",
        "eprintln", "format", "write", "writeln", "vec", "todo", "unimplemented", "panic",
        "assert", "assert_eq", "assert_ne", "debug_assert", "Some", "None", "Ok", "Err",
    ];
    called_fns.retain(|f| f != fn_name && !keywords.contains(&f.as_str()));

    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = fn_re.captures(line) {
            let name = caps[1].to_string();
            if called_fns.contains(&name) {
                // Extract only the signature (up to and including the opening `{`), not the body.
                // If the signature spans multiple lines (e.g., params), collect until `{`.
                let mut sig = String::new();
                for j in i..lines.len() {
                    sig.push_str(lines[j]);
                    if lines[j].contains('{') {
                        break;
                    }
                    sig.push('\n');
                }
                // Trim the body part — keep up to `{` but replace it with `;`.
                if let Some(brace_pos) = sig.find('{') {
                    sig.truncate(brace_pos);
                    sig = sig.trim_end().to_string();
                    // Remove trailing where clause formatting issues.
                }
                context_parts.push(format!("{sig};"));
            }
        }
    }

    context_parts.join("\n\n")
}

/// Replace the function named `fn_name` in `source` with `new_fn`.
///
/// Locates the function by name, finds its extent via brace counting,
/// and splices `new_fn` in its place, preserving all surrounding code.
pub fn splice_function(source: &str, fn_name: &str, new_fn: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let fn_re = Regex::new(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)").expect("valid regex");

    let mut fn_start = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = fn_re.captures(line) {
            if &caps[1] == fn_name {
                fn_start = Some(i);
                break;
            }
        }
    }

    let fn_start = match fn_start {
        Some(s) => s,
        None => return source.to_string(),
    };

    // Find closing brace.
    let mut brace_depth = 0i32;
    let mut found_open = false;
    let mut fn_end = fn_start;

    for i in fn_start..lines.len() {
        for ch in lines[i].chars() {
            if ch == '{' {
                brace_depth += 1;
                found_open = true;
            } else if ch == '}' {
                brace_depth -= 1;
            }
        }
        if found_open && brace_depth == 0 {
            fn_end = i;
            break;
        }
    }

    let mut result = Vec::new();
    if fn_start > 0 {
        result.extend_from_slice(&lines[..fn_start]);
    }
    result.push(new_fn.trim_end());
    if fn_end + 1 < lines.len() {
        result.extend_from_slice(&lines[fn_end + 1..]);
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SOURCE: &str = r#"use std::collections::HashMap;

fn foo(x: i32) -> i32 {
    let y = x + 1;
    y * 2
}

fn bar(a: &str) -> String {
    let mut result = String::new();
    for ch in a.chars() {
        if ch.is_uppercase() {
            result.push(ch);
        }
    }
    result
}

fn baz() -> bool {
    true
}"#;

    #[test]
    fn test_extract_function_at_line() {
        // Line 9 is inside bar ("let mut result = String::new();")
        let result = extract_function_at_line(SAMPLE_SOURCE, 9);
        assert!(result.is_some(), "should find function at line 9");
        let (name, body) = result.unwrap();
        assert_eq!(name, "bar");
        assert!(body.contains("fn bar"));
        assert!(body.contains("result.push(ch)"));

        // Line 4 is inside foo ("let y = x + 1;")
        let result = extract_function_at_line(SAMPLE_SOURCE, 4);
        assert!(result.is_some(), "should find function at line 4");
        let (name, _body) = result.unwrap();
        assert_eq!(name, "foo");

        // Line 1 is a use statement — not inside any function.
        let result = extract_function_at_line(SAMPLE_SOURCE, 1);
        assert!(result.is_none(), "line 1 is not inside any function");
    }

    #[test]
    fn test_extract_out_of_bounds() {
        assert!(extract_function_at_line(SAMPLE_SOURCE, 0).is_none());
        assert!(extract_function_at_line(SAMPLE_SOURCE, 9999).is_none());
        assert!(extract_function_at_line("", 1).is_none());
    }

    #[test]
    fn test_gather_type_context() {
        let source = r#"use std::io;

pub struct ZipArchive {
    entries: Vec<ZipEntry>,
    path: String,
}

pub enum ZipError {
    NotFound(String),
    Corrupt,
}

struct ZipEntry {
    name: String,
}

fn helper(archive: &ZipArchive) -> usize {
    archive.entries.len()
}

fn broken_fn(archive: &ZipArchive) -> Result<(), ZipError> {
    let count = helper(archive);
    if count == 0 {
        return Err(ZipError::NotFound("empty".into()));
    }
    Ok(())
}

fn unrelated_fn() -> i32 {
    42
}"#;

        let ctx = gather_context(source, "broken_fn");

        // Should include type definitions referenced by broken_fn.
        assert!(
            ctx.contains("struct ZipArchive"),
            "context should include ZipArchive struct"
        );
        assert!(
            ctx.contains("enum ZipError"),
            "context should include ZipError enum"
        );

        // Should include helper signature (called from broken_fn).
        assert!(
            ctx.contains("fn helper"),
            "context should include helper signature"
        );
        // Should NOT include the full helper body.
        assert!(
            !ctx.contains("archive.entries.len()"),
            "context should not include helper body"
        );

        // Should NOT include unrelated_fn.
        assert!(
            !ctx.contains("unrelated_fn"),
            "context should not include unrelated functions"
        );
    }

    #[test]
    fn test_gather_context_missing_fn() {
        let ctx = gather_context("fn foo() {}", "nonexistent");
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_splice_function() {
        let new_bar = r#"fn bar(a: &str) -> String {
    a.chars().filter(|c| c.is_uppercase()).collect()
}"#;

        let result = splice_function(SAMPLE_SOURCE, "bar", new_bar);

        // New body is present.
        assert!(
            result.contains("filter(|c| c.is_uppercase()).collect()"),
            "result should contain new bar body"
        );
        // Old body is gone.
        assert!(
            !result.contains("result.push(ch)"),
            "old bar body should be removed"
        );
        // foo and baz are unchanged.
        assert!(result.contains("fn foo(x: i32) -> i32"), "foo is preserved");
        assert!(result.contains("fn baz() -> bool"), "baz is preserved");
        assert!(result.contains("y * 2"), "foo body is preserved");
        assert!(result.contains("true"), "baz body is preserved");
    }

    #[test]
    fn test_splice_nonexistent_function() {
        let result = splice_function(SAMPLE_SOURCE, "nonexistent", "fn x() {}");
        assert_eq!(result, SAMPLE_SOURCE, "source should be unchanged");
    }

    #[test]
    fn test_splice_preserves_use_statements() {
        let result = splice_function(SAMPLE_SOURCE, "foo", "fn foo(x: i32) -> i32 {\n    x\n}");
        assert!(
            result.contains("use std::collections::HashMap"),
            "use statement should be preserved"
        );
    }
}
