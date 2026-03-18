/// Analysis agent for examining C source code before migration.
pub mod analysis;
/// Behavioral equivalence review agent for deep LLM-based comparison.
pub mod behavioral_review;
/// Ensemble translation: multi-provider best-of-N candidate selection.
pub mod ensemble_translation;
/// Mock LLM responses for testing agent parsing logic without real API calls.
pub mod mock_responses;
/// LLM provider configuration and client abstraction (Anthropic, DeepSeek, Ollama).
pub mod providers;
/// Repair agent for fixing Rust compilation errors in migrated code.
pub mod repair;
/// Test generation agent for creating behavioral equivalence tests.
pub mod test_gen;
/// Translation agent for converting C source code to idiomatic Rust.
pub mod translation;

pub use providers::{LlmClient, TokenUsage};

use thiserror::Error;

/// Errors that can occur during LLM agent operations.
#[derive(Debug, Error)]
pub enum AgentError {
    /// The LLM provider returned an error or failed to respond.
    #[error("LLM provider error: {0}")]
    Provider(String),

    /// No LLM provider is configured for the requested role.
    #[error("no provider configured for role: {0}")]
    NoProvider(String),

    /// The LLM response could not be parsed into the expected format.
    #[error("failed to parse LLM response: {0}")]
    Parse(String),

    /// The maximum number of retry attempts has been exceeded.
    #[error("max retries exceeded")]
    MaxRetries,
}

/// Estimate token count from text content using a word/symbol-aware heuristic.
///
/// For actual API token counts, use [`LlmClient::run_prompt_with_usage`] which
/// captures real `input_tokens`/`output_tokens` from the provider response.
/// This heuristic remains useful for pre-flight budget estimation where no API
/// call has been made yet. It is more accurate than a simple bytes/4 ratio because:
/// - Short identifiers and keywords tend to be single tokens
/// - Punctuation and operators are often individual tokens
/// - Whitespace is typically merged with adjacent tokens
///
/// Empirically calibrated against Claude tokenizer behavior on source code.
pub fn estimate_tokens(text: &str) -> u64 {
    if text.is_empty() {
        return 0;
    }

    let mut tokens: u64 = 0;
    let mut in_word = false;

    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            if !in_word {
                in_word = true;
                tokens += 1; // start of a new word/identifier
            }
        } else {
            in_word = false;
            if !ch.is_whitespace() {
                // Punctuation, operators, braces count as individual tokens
                tokens += 1;
            }
        }
    }

    // Long identifiers may be split into multiple sub-word tokens.
    // Add ~1 token per 8 chars of word content to account for this.
    let alpha_chars = text
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .count() as u64;
    tokens += alpha_chars / 8;

    // Newlines contribute fractionally (whitespace tokens)
    tokens += text.lines().count().saturating_sub(1) as u64 / 4;

    tokens.max(1)
}

/// Extract Rust code from an LLM response, stripping markdown fences if present.
///
/// Shared by translation and repair agents to avoid duplication.
/// Logs a warning if the extracted output looks like prose or empty stubs.
pub fn extract_rust_code(response: &str) -> String {
    // Try to extract code from ```rust ... ``` fences
    let code = if let Some(start) = response.find("```rust") {
        let after_fence = &response[start + 7..];
        if let Some(end) = after_fence.find("```") {
            after_fence[..end].trim().to_string()
        } else {
            extract_from_generic_fence(response)
        }
    } else {
        extract_from_generic_fence(response)
    };

    // P4: Validate extracted output has code substance
    if !code.is_empty() {
        let has_fn = code.contains("fn ");
        let has_struct = code.contains("struct ") || code.contains("enum ");
        let has_code_chars = code.contains('{') && code.contains('}');
        if !has_fn && !has_struct && !has_code_chars {
            tracing::warn!(
                len = code.len(),
                "extract_rust_code: output looks like prose, not Rust code"
            );
        }
    }

    code
}

/// Try to extract code from generic ``` fences, or return as-is.
fn extract_from_generic_fence(response: &str) -> String {
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
        let response =
            "Here is the code:\n```rust\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n```\n";
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

    #[test]
    fn test_extract_with_prose_and_code() {
        let response = "Here's the Rust translation:\n\n```rust\nfn add(a: i32, b: i32) -> i32 { a + b }\n```\n\nThis is a simple function.";
        let code = extract_rust_code(response);
        assert_eq!(code, "fn add(a: i32, b: i32) -> i32 { a + b }");
    }

    #[test]
    fn test_extract_nested_fences() {
        let response =
            "```rust\nfn fixed() -> i32 {\n    // uses ```backticks``` in comment\n    42\n}\n```";
        let code = extract_rust_code(response);
        assert!(
            code.contains("fn fixed()"),
            "should extract code despite nested backticks"
        );
    }

    #[test]
    fn test_extract_rust_code_empty() {
        let code = extract_rust_code(crate::mock_responses::MOCK_TRANSLATION_EMPTY);
        assert!(code.is_empty(), "empty input should produce empty output");
    }

    #[test]
    fn test_extract_rust_code_only_prose() {
        let code = extract_rust_code(crate::mock_responses::MOCK_TRANSLATION_ONLY_PROSE);
        assert!(
            !code.is_empty(),
            "prose-only input returns the text as-is (no fences found)"
        );
        assert!(code.contains("too complex"));
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_estimate_tokens_simple_code() {
        // "fn add(a: i32, b: i32) -> i32 { a + b }" has:
        // words: fn, add, a, i32, b, i32, i32, a, b = 9 words
        // symbols: (, :, ,, :, ), -, >, {, +, } = 10 symbols
        // Should be roughly 20-30 tokens
        let tokens = estimate_tokens("fn add(a: i32, b: i32) -> i32 { a + b }");
        assert!(tokens > 10, "expected >10 tokens, got {tokens}");
        assert!(tokens < 50, "expected <50 tokens, got {tokens}");
    }

    #[test]
    fn test_estimate_tokens_more_for_longer_text() {
        let short = estimate_tokens("fn a() {}");
        let long = estimate_tokens(
            "fn very_long_function_name(param_one: i32, param_two: String) -> Result<Vec<u8>, Box<dyn Error>> { todo!() }",
        );
        assert!(long > short, "longer text should estimate more tokens");
    }
}
