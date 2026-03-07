//! Rule-based C-to-Rust translator for simple functions.
//!
//! This is the zero-dependency fallback: no c2rust, no LLM needed.
//! Handles simple patterns: arithmetic, control flow (if/else, while, for),
//! variable declarations, printf, and function calls.
//! For anything complex (pointers, structs, malloc, goto), returns None
//! and the caller should escalate to LLM.

mod blocks;
mod expressions;
mod functions;
mod parsing;
mod statements;

use blocks::translate_main;
use functions::translate_function;
use parsing::{extract_braced_block, find_brace_line, try_parse_function_start};
use tracing::{debug, info};

/// Attempt to translate a C source file to Rust using rule-based patterns.
///
/// Returns `Some(rust_source)` if the function is simple enough to translate
/// deterministically, `None` if it requires LLM assistance.
pub fn try_translate(c_source: &str, function_name: &str) -> Option<String> {
    let functions = extract_functions(c_source);
    if functions.is_empty() {
        debug!(function = function_name, "no functions found in source");
        return None;
    }

    let mut rust_parts: Vec<String> = Vec::new();
    let mut main_body: Option<String> = None;

    for func in &functions {
        if func.name == "main" {
            main_body = translate_main(&func.body);
        } else if let Some(rust_fn) = translate_function(func) {
            rust_parts.push(rust_fn);
        } else {
            debug!(
                function = func.name,
                "function too complex for rule-based translation"
            );
            return None;
        }
    }

    if rust_parts.is_empty() && main_body.is_none() {
        return None;
    }

    let mut output = String::new();
    for part in &rust_parts {
        output.push_str(part);
        output.push_str("\n\n");
    }
    if let Some(main) = main_body {
        output.push_str(&main);
        output.push('\n');
    }

    info!(
        function = function_name,
        functions = functions.len(),
        "rule-based translation succeeded"
    );

    Some(output.trim().to_string())
}

// --- Data structures ---

#[derive(Debug)]
pub(super) struct CFunction {
    pub(super) name: String,
    pub(super) return_type: String,
    pub(super) params: Vec<(String, String)>, // (type, name)
    pub(super) body: String,
}

// --- Function extraction ---

fn extract_functions(source: &str) -> Vec<CFunction> {
    let mut functions = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        if line.starts_with('#') || line.starts_with("//") || line.is_empty() {
            i += 1;
            continue;
        }

        if let Some(func) = try_parse_function_start(line)
            && let Some(start) = find_brace_line(&lines, i)
            && let Some((body, end)) = extract_braced_block(&lines, start)
        {
            functions.push(CFunction {
                name: func.0,
                return_type: func.1,
                params: func.2,
                body,
            });
            i = end + 1;
            continue;
        }

        i += 1;
    }

    functions
}

#[cfg(test)]
mod tests {
    use super::*;
    // Re-export submodule items for testing
    use expressions::{clean_numeric_literal, find_operator, is_numeric_literal};
    use functions::c_type_to_rust;
    use statements::{try_translate_compound_assign, try_translate_inc_dec};

    #[test]
    fn test_translate_add() {
        let c_source = r#"
int add(int a, int b) {
    return a + b;
}

int main(void) {
    printf("%d\n", add(2, 3));
    printf("%d\n", add(-1, 1));
    printf("%d\n", add(0, 0));
    return 0;
}
"#;
        let result = try_translate(c_source, "add").unwrap();
        assert!(result.contains("pub fn add(a: i32, b: i32) -> i32"));
        assert!(result.contains("a + b"));
        assert!(result.contains("fn main()"));
        assert!(result.contains("println!"));
    }

    #[test]
    fn test_translate_simple_arithmetic() {
        let c_source = r#"
int multiply(int x, int y) {
    return x * y;
}
"#;
        let result = try_translate(c_source, "multiply").unwrap();
        assert!(result.contains("pub fn multiply(x: i32, y: i32) -> i32"));
        assert!(result.contains("x * y"));
    }

    #[test]
    fn test_translate_rejects_pointers() {
        let c_source = r#"
void copy(char *dst, const char *src) {
    while (*src) { *dst++ = *src++; }
}
"#;
        assert!(try_translate(c_source, "copy").is_none());
    }

    #[test]
    fn test_translate_rejects_malloc() {
        let c_source = r#"
int* create(int n) {
    int *arr = (int *)malloc(n * sizeof(int));
    return arr;
}
"#;
        assert!(try_translate(c_source, "create").is_none());
    }

    #[test]
    fn test_c_type_to_rust() {
        assert_eq!(c_type_to_rust("int"), Some("i32"));
        assert_eq!(c_type_to_rust("double"), Some("f64"));
        assert_eq!(c_type_to_rust("void"), Some("()"));
        assert_eq!(c_type_to_rust("bool"), Some("bool"));
        assert_eq!(c_type_to_rust("long long"), Some("i64"));
        assert_eq!(c_type_to_rust("const char *"), None);
    }

    #[test]
    fn test_translate_printf() {
        let stmt = r#"printf("%d\n", add(2, 3));"#;
        let result = blocks::translate_printf(stmt).unwrap();
        assert_eq!(result, "println!(\"{}\", add(2, 3));");
    }

    #[test]
    fn test_translate_if_else() {
        let c_source = r#"
int abs_val(int x) {
    if (x < 0) {
        return -x;
    } else {
        return x;
    }
}
"#;
        let result = try_translate(c_source, "abs_val").unwrap();
        assert!(result.contains("if x < 0"));
        assert!(result.contains("-x"));
        assert!(result.contains("else"));
    }

    #[test]
    fn test_translate_while_loop() {
        let c_source = r#"
int sum_to(int n) {
    int total = 0;
    int i = 1;
    while (i <= n) {
        total += i;
        i++;
    }
    return total;
}
"#;
        let result = try_translate(c_source, "sum_to").unwrap();
        assert!(result.contains("while i <= n"));
        assert!(result.contains("total += i;"));
        assert!(result.contains("i += 1;"));
    }

    #[test]
    fn test_translate_for_loop() {
        let c_source = r#"
int factorial(int n) {
    int result = 1;
    for (int i = 2; i <= n; i++) {
        result *= i;
    }
    return result;
}
"#;
        let result = try_translate(c_source, "factorial").unwrap();
        assert!(result.contains("let mut i: i32 = 2;"));
        assert!(result.contains("while i <= n"));
        assert!(result.contains("result *= i;"));
        assert!(result.contains("i += 1;"));
    }

    #[test]
    fn test_translate_ternary() {
        let c_source = r#"
int max(int a, int b) {
    return a > b ? a : b;
}
"#;
        let result = try_translate(c_source, "max").unwrap();
        assert!(result.contains("if a > b { a } else { b }"));
    }

    #[test]
    fn test_translate_compound_assign() {
        let result = try_translate_compound_assign("x += 5;").unwrap();
        assert_eq!(result, "x += 5;");
        let result = try_translate_compound_assign("y *= 2;").unwrap();
        assert_eq!(result, "y *= 2;");
    }

    #[test]
    fn test_translate_inc_dec() {
        assert_eq!(try_translate_inc_dec("i++;"), Some("i += 1;".to_string()));
        assert_eq!(try_translate_inc_dec("--j;"), Some("j -= 1;".to_string()));
    }

    #[test]
    fn test_find_operator_respects_parens() {
        // `(a + b) * c`: the ` * ` starts at index 7
        assert_eq!(find_operator("(a + b) * c", " * "), Some(7));
        assert_eq!(find_operator("a + (b * c)", " + "), Some(1));
        // Should NOT find the + inside parens
        assert_eq!(find_operator("(a + b)", " + "), None);
    }

    #[test]
    fn test_numeric_literals() {
        assert!(is_numeric_literal("42"));
        assert!(is_numeric_literal("42L"));
        assert!(is_numeric_literal("3.14f"));
        assert!(is_numeric_literal("0xFF"));
        assert_eq!(clean_numeric_literal("42L"), "42");
        assert_eq!(clean_numeric_literal("3.14f"), "3.14");
    }

    #[test]
    fn test_translate_else_if_chain() {
        let c_source = r#"
int classify(int x) {
    if (x > 0) {
        return 1;
    } else if (x < 0) {
        return -1;
    } else {
        return 0;
    }
}
"#;
        let result = try_translate(c_source, "classify").unwrap();
        assert!(result.contains("if x > 0"));
        assert!(result.contains("else if x < 0"));
        assert!(result.contains("else"));
    }
}
