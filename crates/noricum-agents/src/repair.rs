/// Repair Agent: fixes Rust compilation errors in migrated code.
///
/// Takes the current Rust source, compiler error messages, and the original C source
/// for reference. Returns corrected Rust source code. Max iterations are handled
/// by the caller.
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::anthropic;
use tracing::{debug, info, warn};

use crate::AgentError;

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
pub async fn repair_function(
    client: &anthropic::Client,
    model: &str,
    rust_source: &str,
    compiler_errors: &[String],
    diff_feedback: &[String],
    c_source: &str,
    iteration: u32,
    max_iterations: u32,
) -> Result<String, AgentError> {
    let error_count = compiler_errors.len();
    let diff_count = diff_feedback.len();
    info!(model, error_count, diff_count, iteration, "starting repair");

    if compiler_errors.is_empty() && diff_feedback.is_empty() {
        warn!("repair called with no errors or feedback");
        return Ok(rust_source.to_string());
    }

    // Increase temperature on later iterations to try different approaches
    let temperature = match iteration {
        1 => 0.2,
        2 => 0.4,
        3 => 0.6,
        _ => 0.8,
    };

    let agent = client
        .agent(model)
        .preamble(REPAIR_PREAMBLE)
        .temperature(temperature)
        .max_tokens(8192)
        .build();

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
        user_message.push_str(&format!(
            "## Compiler errors\n```\n{errors_text}\n```\n\n"
        ));

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

    user_message.push_str(&format!(
        "## Original C source (for reference)\n```c\n{c_source}\n```\n\n\
         Fix all issues. The Rust output must match the C output exactly byte-for-byte. \
         Output ONLY the complete corrected Rust source code."
    ));

    debug!(error_count, diff_count, "sending repair prompt to LLM");

    let response = agent
        .prompt(&user_message)
        .await
        .map_err(|e| AgentError::Provider(format!("repair LLM call failed: {e}")))?;

    debug!(response_len = response.len(), "received repair response");

    Ok(extract_rust_code(&response))
}

/// Extract Rust code from the LLM response, stripping markdown fences if present.
fn extract_rust_code(response: &str) -> String {
    if let Some(start) = response.find("```rust") {
        let after_fence = &response[start + 7..];
        if let Some(end) = after_fence.find("```") {
            return after_fence[..end].trim().to_string();
        }
    }

    if let Some(start) = response.find("```") {
        let after_fence = &response[start + 3..];
        let code_start = after_fence.find('\n').map(|i| i + 1).unwrap_or(0);
        let after_lang = &after_fence[code_start..];
        if let Some(end) = after_lang.find("```") {
            return after_lang[..end].trim().to_string();
        }
    }

    response.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_code_with_fence() {
        let response = "```rust\nfn fixed() -> i32 { 42 }\n```";
        assert_eq!(extract_rust_code(response), "fn fixed() -> i32 { 42 }");
    }

    #[test]
    fn test_extract_rust_code_no_fence() {
        let response = "fn fixed() -> i32 { 42 }";
        assert_eq!(extract_rust_code(response), "fn fixed() -> i32 { 42 }");
    }
}
