/// Test Generation Agent: creates Rust `#[test]` functions that verify
/// behavioral equivalence between original C code and the migrated Rust code.
///
/// The generated tests compare inputs/outputs to ensure semantic correctness.
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::anthropic;
use tracing::{debug, info};

use crate::AgentError;

/// System prompt for the test generation agent.
const TEST_GEN_PREAMBLE: &str = "\
You are a Rust test generation agent for the Noricum C-to-Rust migration tool.

## Task
Given the original C source and its Rust translation, generate Rust #[test] functions
that verify behavioral equivalence.

## Requirements
1. Each test should exercise a meaningful input/output pair
2. Cover edge cases: zero, negative, boundary values, empty inputs
3. Test error conditions if applicable
4. Tests must be self-contained (no external dependencies beyond std)
5. Use assert_eq!, assert!, or assert_ne! macros
6. Include at least 3 test cases per function

## Output
Provide ONLY the Rust test code (with #[cfg(test)] module and #[test] functions).
No explanations.";

/// Generate Rust test functions that verify behavioral equivalence.
///
/// Uses Claude API to examine both the original C source and its Rust translation,
/// then produces `#[test]` functions that validate they behave identically.
///
/// # Arguments
/// * `client` - Anthropic API client
/// * `model` - Model identifier (e.g., "claude-sonnet-4-0")
/// * `c_source` - Original C source code
/// * `rust_source` - Translated Rust source code
/// * `function_name` - Name of the function under test
///
/// # Returns
/// The generated Rust test code as a `String`.
///
/// # Errors
/// Returns `AgentError::Provider` if the LLM call fails.
pub async fn generate_tests(
    client: &anthropic::Client,
    model: &str,
    c_source: &str,
    rust_source: &str,
    function_name: &str,
) -> Result<String, AgentError> {
    info!(function = function_name, model, "starting test generation");

    let agent = client
        .agent(model)
        .preamble(TEST_GEN_PREAMBLE)
        .temperature(0.4)
        .max_tokens(8192)
        .build();

    let user_message = format!(
        "Generate Rust #[test] functions for the function `{function_name}`.\n\n\
         ## Original C source\n```c\n{c_source}\n```\n\n\
         ## Rust translation\n```rust\n{rust_source}\n```\n\n\
         Generate comprehensive tests that verify the Rust code behaves identically \
         to the C code. Output ONLY the test code."
    );

    debug!(function = function_name, "sending test generation prompt to LLM");

    let response = agent
        .prompt(&user_message)
        .await
        .map_err(|e| AgentError::Provider(format!("test generation LLM call failed: {e}")))?;

    debug!(
        function = function_name,
        response_len = response.len(),
        "received test generation response"
    );

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
        let response = "```rust\n#[test]\nfn test_add() {\n    assert_eq!(add(1, 2), 3);\n}\n```";
        let code = extract_rust_code(response);
        assert!(code.contains("#[test]"));
        assert!(code.contains("assert_eq!"));
    }

    #[test]
    fn test_extract_rust_code_no_fence() {
        let response = "#[test]\nfn test_add() {\n    assert_eq!(add(1, 2), 3);\n}";
        let code = extract_rust_code(response);
        assert!(code.contains("#[test]"));
    }
}
