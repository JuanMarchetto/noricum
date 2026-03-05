/// Auto-generate `main()` test harnesses for C library functions without `main()`.
///
/// Parses C function signatures and generates calling code with representative
/// inputs so that library functions can be diff-tested.

/// A parsed C function signature.
struct CSignature {
    return_type: String,
    name: String,
    params: Vec<CParam>,
}

/// A parsed C function parameter.
struct CParam {
    type_name: String,
    param_name: String,
}

/// Generate a C test harness with `main()` for a library function.
///
/// Appends a `main()` function to the original C source that calls the target
/// function with representative inputs and prints the results.
///
/// Returns `None` if the signature can't be parsed or has unsupported types
/// (void*, struct pointers, function pointers, etc.).
pub fn generate_test_harness(c_source: &str, function_name: &str) -> Option<String> {
    let sig = parse_c_signature(c_source, function_name)?;

    // Reject unsupported parameter types
    for param in &sig.params {
        let t = param.type_name.trim();
        if t.contains("void*")
            || t.contains("void *")
            || t.contains("struct")
            || t.contains("(*")
            || t.contains("FILE")
        {
            return None;
        }
    }

    let mut harness = String::new();

    // Ensure stdio.h is included
    if !c_source.contains("#include <stdio.h>") {
        harness.push_str("#include <stdio.h>\n");
    }
    if !c_source.contains("#include <stdlib.h>") {
        harness.push_str("#include <stdlib.h>\n");
    }
    if !c_source.contains("#include <limits.h>") && has_int_params(&sig) {
        harness.push_str("#include <limits.h>\n");
    }
    harness.push('\n');

    // Include original source
    harness.push_str(c_source);
    harness.push_str("\n\n");

    // Generate main
    harness.push_str("int main(void) {\n");

    let test_calls = generate_test_calls(&sig);
    for call in &test_calls {
        harness.push_str(call);
    }

    harness.push_str("    return 0;\n}\n");

    Some(harness)
}

/// Parse a C function signature from source code.
fn parse_c_signature(c_source: &str, function_name: &str) -> Option<CSignature> {
    // Look for pattern: <return_type> <function_name>(<params>)
    // This handles common cases but not every C declaration style.
    for line in c_source.lines() {
        let trimmed = line.trim();
        // Skip comments, preprocessor directives
        if trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('#')
            || trimmed.is_empty()
        {
            continue;
        }

        // Find function_name followed by (
        if let Some(name_pos) = trimmed.find(function_name) {
            let after_name = &trimmed[name_pos + function_name.len()..];
            if !after_name.starts_with('(') {
                continue;
            }

            // Extract return type (everything before function name)
            let return_type = trimmed[..name_pos].trim().to_string();
            if return_type.is_empty() {
                continue;
            }

            // Extract parameter list
            let params_start = name_pos + function_name.len() + 1;
            // Find matching )
            let rest = &trimmed[params_start..];
            let paren_end = rest.find(')')?;
            let params_str = &rest[..paren_end];

            let params = parse_params(params_str);
            return Some(CSignature {
                return_type,
                name: function_name.to_string(),
                params,
            });
        }
    }

    None
}

/// Parse a C parameter list string into typed parameters.
fn parse_params(params_str: &str) -> Vec<CParam> {
    let trimmed = params_str.trim();
    if trimmed.is_empty() || trimmed == "void" {
        return Vec::new();
    }

    let mut params = Vec::new();
    for (i, part) in trimmed.split(',').enumerate() {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        // Split into type and name: last word is the name (unless it's a pointer)
        let tokens: Vec<&str> = part.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        let (type_name, param_name) = if tokens.len() == 1 {
            // Just a type, no name (e.g. "int")
            (tokens[0].to_string(), format!("arg{i}"))
        } else {
            // Last token is the name (possibly with * prefix)
            let last = tokens.last().unwrap();
            let name = last.trim_start_matches('*');
            let type_parts: Vec<&str> = tokens[..tokens.len() - 1].to_vec();
            let mut t = type_parts.join(" ");
            // If the name had leading *, the * belongs to the type
            let star_count = last.len() - name.len();
            for _ in 0..star_count {
                t.push('*');
            }
            let n = if name.is_empty() {
                format!("arg{i}")
            } else {
                name.to_string()
            };
            (t, n)
        };

        params.push(CParam {
            type_name,
            param_name,
        });
    }

    params
}

/// Check if any parameter is an integer type.
fn has_int_params(sig: &CSignature) -> bool {
    sig.params.iter().any(|p| {
        let t = p.type_name.trim();
        t == "int" || t == "long" || t == "short" || t == "unsigned int" || t == "unsigned"
    })
}

/// Generate test call lines for a function signature.
fn generate_test_calls(sig: &CSignature) -> Vec<String> {
    let mut calls = Vec::new();
    let test_values = generate_param_test_values(&sig.params);

    for (i, values) in test_values.iter().enumerate() {
        let args_str = values.join(", ");
        let call = format!("{}({})", sig.name, args_str);

        let line = match sig.return_type.trim() {
            "void" => {
                format!("    /* test {} */ {};\n", i + 1, call)
            }
            t if t.contains("double") || t.contains("float") => {
                format!("    printf(\"%f\\n\", (double){});\n", call)
            }
            t if t.contains("char*") || t.contains("char *") => {
                format!("    printf(\"%s\\n\", {});\n", call)
            }
            _ => {
                // int, long, short, unsigned, etc.
                format!("    printf(\"%d\\n\", (int){});\n", call)
            }
        };
        calls.push(line);
    }

    calls
}

/// Generate representative test value combinations for parameters.
fn generate_param_test_values(params: &[CParam]) -> Vec<Vec<String>> {
    if params.is_empty() {
        return vec![vec![]];
    }

    let per_param: Vec<Vec<String>> = params
        .iter()
        .map(|p| {
            let t = p.type_name.trim();
            if t.contains("char*") || t.contains("char *") || t.contains("const char") {
                vec!["\"hello\"".to_string(), "\"\"".to_string()]
            } else if t.contains("double") || t.contains("float") {
                vec![
                    "0.0".to_string(),
                    "1.0".to_string(),
                    "-1.5".to_string(),
                    "3.14".to_string(),
                ]
            } else if t.contains("int*") || t.contains("int *") {
                // Array parameter — provide a small test array
                vec!["(int[]){1, 2, 3, 4, 5}".to_string()]
            } else {
                // int, long, short, etc.
                vec![
                    "0".to_string(),
                    "1".to_string(),
                    "-1".to_string(),
                    "42".to_string(),
                ]
            }
        })
        .collect();

    // Generate combinations: for simplicity, zip the longest list and repeat shorter ones
    let max_len = per_param.iter().map(|v| v.len()).max().unwrap_or(1);
    let mut combos = Vec::new();
    for i in 0..max_len {
        let combo: Vec<String> = per_param
            .iter()
            .map(|values| values[i % values.len()].clone())
            .collect();
        combos.push(combo);
    }

    combos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_gen_int_function() {
        let c_source = "int add(int a, int b) { return a + b; }";
        let harness = generate_test_harness(c_source, "add").unwrap();
        assert!(harness.contains("int main(void)"), "should have main()");
        assert!(harness.contains("add("), "should call add()");
        assert!(harness.contains("printf"), "should print results");
        assert!(
            harness.contains("int add(int a, int b)"),
            "should include original source"
        );
    }

    #[test]
    fn test_harness_gen_string_function() {
        let c_source = "int my_strlen(const char* s) { int n = 0; while(s[n]) n++; return n; }";
        let harness = generate_test_harness(c_source, "my_strlen").unwrap();
        assert!(harness.contains("my_strlen("), "should call my_strlen");
        assert!(
            harness.contains("\"hello\""),
            "should test with string input"
        );
    }

    #[test]
    fn test_harness_gen_void_function() {
        let c_source = r#"
#include <stdio.h>
void print_hello(void) { printf("hello\n"); }
"#;
        let harness = generate_test_harness(c_source, "print_hello").unwrap();
        assert!(
            harness.contains("print_hello()"),
            "should call void function"
        );
    }

    #[test]
    fn test_harness_gen_unsupported() {
        let c_source = "void* complex(struct Foo* f) { return NULL; }";
        let result = generate_test_harness(c_source, "complex");
        assert!(result.is_none(), "struct/void* params should return None");
    }
}
