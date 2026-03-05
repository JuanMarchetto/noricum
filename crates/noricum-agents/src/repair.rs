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

/// Attempt to repair Rust compilation errors in migrated code.
///
/// Uses Claude API with the failing Rust source, compiler errors, and original C code
/// as context to produce a corrected version.
///
/// # Arguments
/// * `client` - Anthropic API client
/// * `model` - Model identifier (e.g., "claude-sonnet-4-0")
/// * `rust_source` - Current Rust source that fails to compile
/// * `compiler_errors` - List of compiler error messages
/// * `c_source` - Original C source for semantic reference
///
/// # Returns
/// The corrected Rust source code as a `String`.
///
/// # Errors
/// Returns `AgentError::Provider` if the LLM call fails.
pub async fn repair_function(
    client: &anthropic::Client,
    model: &str,
    rust_source: &str,
    compiler_errors: &[String],
    c_source: &str,
) -> Result<String, AgentError> {
    let error_count = compiler_errors.len();
    info!(model, error_count, "starting repair");

    if compiler_errors.is_empty() {
        warn!("repair called with no compiler errors");
        return Ok(rust_source.to_string());
    }

    let agent = client
        .agent(model)
        .preamble(REPAIR_PREAMBLE)
        .temperature(0.2)
        .max_tokens(8192)
        .build();

    let errors_text = compiler_errors
        .iter()
        .enumerate()
        .map(|(i, e)| format!("Error {}: {e}", i + 1))
        .collect::<Vec<_>>()
        .join("\n");

    let user_message = format!(
        "## Current Rust source (failing)\n```rust\n{rust_source}\n```\n\n\
         ## Compiler errors\n```\n{errors_text}\n```\n\n\
         ## Original C source (for reference)\n```c\n{c_source}\n```\n\n\
         Fix the compiler errors. Output ONLY the complete corrected Rust source code."
    );

    debug!(error_count, "sending repair prompt to LLM");

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
