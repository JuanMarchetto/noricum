/// Repair Agent: fixes Rust compilation errors in migrated code.
///
/// Takes the current Rust source, compiler error messages, and the original C source
/// for reference. Returns corrected Rust source code. Max iterations are handled
/// by the caller.
use tracing::{debug, info, warn};

use crate::AgentError;
use crate::providers::LlmClient;

/// System prompt for the repair agent, loaded from the prompts directory at compile time.
const REPAIR_PREAMBLE: &str = include_str!("../../../prompts/repair.md");

/// Attempt to repair Rust code with compiler errors and/or behavioral mismatches.
///
/// Uses Claude API with the failing Rust source, compiler errors, optional diff test
/// feedback, and original C code as context to produce a corrected version.
///
/// The repair agent handles two kinds of problems:
/// - **Compilation errors**: The Rust code doesn't compile (type errors, syntax, etc.)
/// - **Behavioral mismatches**: The code compiles but produces different output than the C original
///   (e.g., using `bool` where C uses `int`, changing format specifiers, etc.)
#[allow(clippy::too_many_arguments)]
pub async fn repair_function(
    client: &LlmClient,
    model: &str,
    rust_source: &str,
    compiler_errors: &[String],
    diff_feedback: &[String],
    c_source: &str,
    iteration: u32,
    max_iterations: u32,
) -> Result<String, AgentError> {
    repair_function_with_temperature(
        client,
        model,
        rust_source,
        compiler_errors,
        diff_feedback,
        c_source,
        iteration,
        max_iterations,
        None,
    )
    .await
}

/// Repair with an optional base temperature override.
#[allow(clippy::too_many_arguments)]
pub async fn repair_function_with_temperature(
    client: &LlmClient,
    model: &str,
    rust_source: &str,
    compiler_errors: &[String],
    diff_feedback: &[String],
    c_source: &str,
    iteration: u32,
    max_iterations: u32,
    base_temperature: Option<f64>,
) -> Result<String, AgentError> {
    let error_count = compiler_errors.len();
    let diff_count = diff_feedback.len();
    info!(model, error_count, diff_count, iteration, "starting repair");

    if compiler_errors.is_empty() && diff_feedback.is_empty() {
        warn!("repair called with no errors or feedback");
        return Ok(rust_source.to_string());
    }

    // Increase temperature on later iterations to try different approaches.
    // Uses a gentler ramp (0.1 step) to avoid destabilizing repairs.
    let base = base_temperature.unwrap_or(0.2);
    let temperature = (base + (iteration.saturating_sub(1) as f64) * 0.1).min(1.0);

    let mut user_message = String::new();

    // Tell the LLM about iteration context
    if iteration > 1 {
        user_message.push_str(&format!(
            "## IMPORTANT: This is repair attempt {iteration} of {max_iterations}.\n\
             Previous attempts with the SAME errors have failed. You MUST try a fundamentally \
             different approach this time. Do not repeat the same fix.\n\n"
        ));
    }

    user_message.push_str(&format!(
        "## Current Rust source\n```rust\n{rust_source}\n```\n\n"
    ));

    if !compiler_errors.is_empty() {
        let errors_text = compiler_errors
            .iter()
            .enumerate()
            .map(|(i, e)| format!("Error {}: {e}", i + 1))
            .collect::<Vec<_>>()
            .join("\n");
        user_message.push_str(&format!("## Compiler errors\n```\n{errors_text}\n```\n\n"));

        // Add hints for common Rust gotchas on later iterations
        if iteration >= 2 {
            user_message.push_str(
                "## Hints for common Rust issues:\n\
                 - `vec![value; N]` requires `Clone`. Use `(0..N).map(|_| Default::default()).collect()` instead.\n\
                 - Recursive types need `Box<>`. Use `Option<Box<T>>` for optional recursive fields.\n\
                 - Mutable borrows: you can't hold multiple `&mut` to the same data. Restructure the logic.\n\
                 - Use `.to_string()` or `.clone()` to avoid move issues with `String`.\n\n"
            );
        }
    }

    if !diff_feedback.is_empty() {
        let diff_text = diff_feedback.join("\n");
        user_message.push_str(&format!(
            "## Behavioral mismatch (diff test failed)\n\
             The code compiles but produces different output than the original C program.\n\
             ```\n{diff_text}\n```\n\n"
        ));
    }

    // For large C sources, abbreviate to save context tokens.
    // The repair agent primarily needs the Rust code + errors; the C source is just reference.
    let c_context = abbreviate_c_source(c_source, 300);

    user_message.push_str(&format!(
        "## Original C source (for reference)\n<c_source>\n{c_context}\n</c_source>\n\n\
         Fix all issues. The Rust output must match the C output exactly byte-for-byte. \
         Output ONLY the complete corrected Rust source code."
    ));

    // Scale max_tokens based on current Rust source size + headroom for fixes.
    let max_tokens = ((rust_source.len() as u64 / 4) * 2).clamp(8192, 65536);

    debug!(
        error_count,
        diff_count, max_tokens, "sending repair prompt to LLM"
    );

    let response = client
        .run_prompt(
            model,
            REPAIR_PREAMBLE,
            temperature,
            max_tokens,
            &user_message,
        )
        .await?;

    debug!(response_len = response.len(), "received repair response");

    Ok(crate::extract_rust_code(&response))
}

/// Abbreviate large C source files to reduce context token usage.
///
/// For files under `max_lines`, returns the full source unchanged.
/// For larger files, keeps struct/typedef declarations, function signatures,
/// and the main() function body, replacing other function bodies with `// ...`.
fn abbreviate_c_source(c_source: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = c_source.lines().collect();
    if lines.len() <= max_lines {
        return c_source.to_string();
    }

    let mut result = Vec::new();
    let mut brace_depth: i32 = 0;
    let mut in_function_body = false;
    let mut body_omitted = false;

    for line in &lines {
        let trimmed = line.trim();

        // Always keep preprocessor directives, typedefs, struct/enum/union declarations
        let is_declaration = trimmed.starts_with("#include")
            || trimmed.starts_with("#define")
            || trimmed.starts_with("typedef")
            || trimmed.starts_with("struct ")
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("union ");

        // Always keep the main function fully
        let is_main = trimmed.starts_with("int main")
            || trimmed.starts_with("void main")
            || trimmed.starts_with("int main(");

        // Track brace depth
        let opens = trimmed.chars().filter(|&c| c == '{').count() as i32;
        let closes = trimmed.chars().filter(|&c| c == '}').count() as i32;

        if is_main {
            in_function_body = false; // don't abbreviate main
        }

        if brace_depth == 0 && opens > 0 && !is_declaration && !is_main {
            // Entering a function body — keep the signature line, abbreviate body
            result.push(*line);
            in_function_body = true;
            body_omitted = false;
            brace_depth += opens - closes;
            continue;
        }

        if in_function_body && brace_depth > 0 {
            if !body_omitted {
                result.push("    // ... (body abbreviated for context)");
                body_omitted = true;
            }
            brace_depth += opens - closes;
            if brace_depth <= 0 {
                result.push("}");
                in_function_body = false;
                brace_depth = 0;
            }
            continue;
        }

        brace_depth += opens - closes;
        if brace_depth < 0 {
            brace_depth = 0;
        }
        result.push(line);
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abbreviate_short_source() {
        let source = "int main() {\n    return 0;\n}\n";
        assert_eq!(abbreviate_c_source(source, 10), source);
    }

    #[test]
    fn test_abbreviate_keeps_main() {
        let source = "\
void helper(int x) {
    int a = 1;
    int b = 2;
    int c = 3;
    return;
}

int main() {
    helper(1);
    return 0;
}
";
        let result = abbreviate_c_source(source, 5);
        assert!(result.contains("int main()"), "main should be kept");
        assert!(result.contains("helper(1);"), "main body should be kept");
        assert!(
            result.contains("// ... (body abbreviated for context)"),
            "helper body should be abbreviated"
        );
    }

    #[test]
    fn test_abbreviate_keeps_declarations() {
        let source = "\
#include <stdio.h>
typedef int myint;
struct Foo { int x; };
void helper(int x) {
    int a = 1;
    int b = 2;
    int c = 3;
    return;
}
int main() {
    return 0;
}
";
        let result = abbreviate_c_source(source, 3);
        assert!(
            result.contains("#include <stdio.h>"),
            "includes should be preserved"
        );
        assert!(
            result.contains("typedef int myint;"),
            "typedefs should be preserved"
        );
    }

    #[test]
    fn test_abbreviate_nested_braces() {
        let source = "\
void outer(int x) {
    if (x > 0) {
        while (x > 0) {
            x--;
        }
    }
}
int main() { return 0; }
";
        let result = abbreviate_c_source(source, 3);
        // The function body (with nested braces) should be collapsed to a single comment
        let comment_count = result
            .matches("// ... (body abbreviated for context)")
            .count();
        assert_eq!(
            comment_count, 1,
            "should have exactly one abbreviation comment"
        );
    }

    #[test]
    fn test_abbreviate_empty() {
        assert_eq!(abbreviate_c_source("", 10), "");
    }
}
