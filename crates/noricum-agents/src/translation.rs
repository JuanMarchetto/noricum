/// Translation Agent: converts C source code to safe, idiomatic Rust.
///
/// Takes the original C source, optional C2Rust mechanical translation output,
/// and the analysis results to produce high-quality Rust code.
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::anthropic;
use tracing::{debug, info};

use crate::analysis::AnalysisResult;
use crate::AgentError;

/// System prompt for the translation agent, loaded from the prompts directory at compile time.
const TRANSLATION_PREAMBLE: &str = include_str!("../../../prompts/translation.md");

/// Translate a C function to safe, idiomatic Rust.
///
/// Uses Claude API with the original C source, optional C2Rust output, and prior
/// analysis to produce the best possible Rust translation.
///
/// # Arguments
/// * `client` - Anthropic API client
/// * `model` - Model identifier (e.g., "claude-sonnet-4-0")
/// * `c_source` - Original C source code
/// * `c2rust_output` - Optional C2Rust mechanical translation (unsafe Rust)
/// * `analysis` - Prior analysis of the C function
///
/// # Returns
/// The translated Rust source code as a `String`.
///
/// # Errors
/// Returns `AgentError::Provider` if the LLM call fails.
pub async fn translate_function(
    client: &anthropic::Client,
    model: &str,
    c_source: &str,
    c2rust_output: Option<&str>,
    analysis: &AnalysisResult,
) -> Result<String, AgentError> {
    info!(model, difficulty = %analysis.difficulty, "starting translation");

    let agent = client
        .agent(model)
        .preamble(TRANSLATION_PREAMBLE)
        .temperature(0.3)
        .max_tokens(8192)
        .build();

    let analysis_json = serde_json::to_string_pretty(analysis)
        .map_err(|e| AgentError::Provider(format!("failed to serialize analysis: {e}")))?;

    let mut user_message = format!(
        "## Original C source\n```c\n{c_source}\n```\n\n\
         ## Analysis\n```json\n{analysis_json}\n```\n"
    );

    if let Some(c2rust) = c2rust_output {
        user_message.push_str(&format!(
            "\n## C2Rust output (unsafe Rust)\n```rust\n{c2rust}\n```\n"
        ));
    }

    user_message.push_str(
        "\nTranslate the C function to safe, idiomatic Rust. Output ONLY the Rust code.",
    );

    debug!("sending translation prompt to LLM");

    let response = agent
        .prompt(&user_message)
        .await
        .map_err(|e| AgentError::Provider(format!("translation LLM call failed: {e}")))?;

    debug!(response_len = response.len(), "received translation response");

    Ok(extract_rust_code(&response))
}

/// Extract Rust code from the LLM response, stripping markdown fences if present.
fn extract_rust_code(response: &str) -> String {
    // Try to extract code from ```rust ... ``` fences
    if let Some(start) = response.find("```rust") {
        let after_fence = &response[start + 7..];
        if let Some(end) = after_fence.find("```") {
            return after_fence[..end].trim().to_string();
        }
    }

    // Try generic code fences
    if let Some(start) = response.find("```") {
        let after_fence = &response[start + 3..];
        // Skip the language tag line if any
        let code_start = after_fence.find('\n').map(|i| i + 1).unwrap_or(0);
        let after_lang = &after_fence[code_start..];
        if let Some(end) = after_lang.find("```") {
            return after_lang[..end].trim().to_string();
        }
    }

    // No fences found, return as-is
    response.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_code_with_fence() {
        let response = "Here is the code:\n```rust\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n```\n";
        let code = extract_rust_code(response);
        assert_eq!(code, "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}");
    }

    #[test]
    fn test_extract_rust_code_no_fence() {
        let response = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}";
        let code = extract_rust_code(response);
        assert_eq!(code, "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}");
    }

    #[test]
    fn test_extract_rust_code_generic_fence() {
        let response = "```\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n```";
        let code = extract_rust_code(response);
        assert_eq!(code, "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}");
    }
}
