/// Documentation generation for migrated Rust code.
///
/// Extracts comments from original C source and generates `///` doc comments
/// on the corresponding Rust functions.
use regex::Regex;

/// Add doc comments to migrated Rust source based on the original C source.
///
/// Extracts C-style comments (`/* */` and `//`) above function definitions,
/// converts them to Rust `///` doc comments, and prepends them to the
/// corresponding Rust function declarations.
pub fn add_docs_to_rust(rust_source: &str, c_source: &str, function_name: &str) -> String {
    let c_comments = extract_c_comments(c_source, function_name);
    let mut result = String::new();

    for line in rust_source.lines() {
        let trimmed = line.trim();
        // Insert doc comment before function declarations (that don't already have one)
        let is_fn_decl = trimmed.starts_with("pub fn ")
            || trimmed.starts_with("fn ")
            || trimmed.starts_with("pub unsafe fn ")
            || trimmed.starts_with("unsafe fn ");
        if is_fn_decl && !already_has_doc_comment(&result) {
            if !c_comments.is_empty() {
                for comment_line in &c_comments {
                    result.push_str(&format!("/// {comment_line}\n"));
                }
            } else {
                // Generate a basic doc comment from the function signature
                let brief = generate_brief_from_signature(trimmed, function_name);
                result.push_str(&format!("/// {brief}\n"));
            }

            // Add safety note if the function contains unsafe
            if rust_source.contains("unsafe ") {
                result.push_str("///\n/// # Safety\n/// This function contains unsafe code.\n");
            }
        }
        result.push_str(line);
        result.push('\n');
    }

    result
}

/// Check if the last non-empty line in the result is already a doc comment.
fn already_has_doc_comment(result: &str) -> bool {
    result
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .is_some_and(|l| l.trim().starts_with("///"))
}

/// Extract comments from C source that precede a function definition.
fn extract_c_comments(c_source: &str, function_name: &str) -> Vec<String> {
    let lines: Vec<&str> = c_source.lines().collect();

    // Find the line containing the function definition
    let func_line_idx = lines.iter().position(|line| {
        let trimmed = line.trim();
        trimmed.contains(function_name) && trimmed.contains('(') && !trimmed.starts_with("//")
    });

    let func_idx = match func_line_idx {
        Some(idx) => idx,
        None => return Vec::new(),
    };

    let mut comments = Vec::new();

    // Walk backwards from the function to collect comments
    let mut i = func_idx.saturating_sub(1);
    loop {
        let trimmed = lines[i].trim();

        if trimmed.starts_with("//") {
            let comment = trimmed.trim_start_matches("//").trim();
            comments.push(comment.to_string());
        } else if trimmed.ends_with("*/") {
            // Multi-line comment: collect until we find /*
            let block_comments = extract_block_comment(&lines, i);
            comments.extend(block_comments);
            break;
        } else if !trimmed.is_empty() {
            break;
        }

        if i == 0 {
            break;
        }
        i -= 1;
    }

    comments.reverse();
    comments
}

/// Extract a C block comment (`/* ... */`) ending at the given line index.
fn extract_block_comment(lines: &[&str], end_idx: usize) -> Vec<String> {
    let mut comments = Vec::new();
    let mut i = end_idx;

    loop {
        let trimmed = lines[i].trim();
        let cleaned = trimmed
            .trim_start_matches("/*")
            .trim_end_matches("*/")
            .trim_start_matches('*')
            .trim();

        if !cleaned.is_empty() {
            comments.push(cleaned.to_string());
        }

        if trimmed.contains("/*") || i == 0 {
            break;
        }
        i -= 1;
    }

    comments.reverse();
    comments
}

/// Generate a brief description from the Rust function signature.
fn generate_brief_from_signature(signature: &str, function_name: &str) -> String {
    let re = Regex::new(r"fn\s+\w+\(([^)]*)\)(?:\s*->\s*(\S+))?").expect("hardcoded regex pattern");

    if let Some(caps) = re.captures(signature) {
        let params = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let ret = caps.get(2).map(|m| m.as_str());

        let param_desc = if params.is_empty() {
            String::new()
        } else {
            let param_names: Vec<&str> = params
                .split(',')
                .filter_map(|p| p.split(':').next())
                .map(|p| p.trim())
                .collect();
            if param_names.is_empty() {
                String::new()
            } else {
                format!(" with {}", param_names.join(", "))
            }
        };

        let ret_desc = match ret {
            Some(t) if t != "()" => format!(", returning `{t}`"),
            _ => String::new(),
        };

        format!("Migrated from C function `{function_name}`{param_desc}{ret_desc}.")
    } else {
        format!("Migrated from C function `{function_name}`.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_docs_basic() {
        let rust = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let c = "// Add two integers\nint add(int a, int b) { return a + b; }";
        let result = add_docs_to_rust(rust, c, "add");
        assert!(result.contains("/// Add two integers"));
        assert!(result.contains("pub fn add"));
    }

    #[test]
    fn test_add_docs_block_comment() {
        let c = "/* Compute the sum of a and b */\nint add(int a, int b) { return a + b; }";
        let rust = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let result = add_docs_to_rust(rust, c, "add");
        assert!(result.contains("/// Compute the sum of a and b"));
    }

    #[test]
    fn test_add_docs_no_c_comments() {
        let c = "int add(int a, int b) { return a + b; }";
        let rust = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let result = add_docs_to_rust(rust, c, "add");
        assert!(result.contains("/// Migrated from C function `add`"));
    }

    #[test]
    fn test_add_docs_unsafe_safety_note() {
        let c = "void* alloc(int n) { return malloc(n); }";
        let rust = "pub fn alloc(n: i32) -> *mut u8 {\n    unsafe { std::alloc::alloc(n) }\n}\n";
        let result = add_docs_to_rust(rust, c, "alloc");
        assert!(result.contains("# Safety"));
    }

    #[test]
    fn test_does_not_double_doc() {
        let rust = "/// Already documented\npub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let c = "// Add two numbers\nint add(int a, int b) { return a + b; }";
        let result = add_docs_to_rust(rust, c, "add");
        // Should not add a second doc comment
        assert_eq!(
            result.matches("///").count(),
            1,
            "should not double-document"
        );
    }

    #[test]
    fn test_extract_c_comments_multiline() {
        let c = "/*\n * Compute power of x to n.\n * Returns x^n.\n */\nint power(int x, int n) { return 0; }";
        let comments = extract_c_comments(c, "power");
        assert!(!comments.is_empty());
        assert!(comments.iter().any(|c| c.contains("power")));
    }

    #[test]
    fn test_generate_brief() {
        let brief = generate_brief_from_signature("pub fn add(a: i32, b: i32) -> i32", "add");
        assert!(brief.contains("add"));
        assert!(brief.contains("i32"));
    }
}
