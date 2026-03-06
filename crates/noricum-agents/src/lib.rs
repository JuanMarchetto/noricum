pub mod analysis;
#[cfg(test)]
#[allow(dead_code)]
mod mock_responses;
pub mod providers;
pub mod repair;
pub mod test_gen;
pub mod translation;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("LLM provider error: {0}")]
    Provider(String),

    #[error("no provider configured for role: {0}")]
    NoProvider(String),

    #[error("failed to parse LLM response: {0}")]
    Parse(String),

    #[error("max retries exceeded")]
    MaxRetries,
}

/// Estimate token count from text length (chars / 4 heuristic).
///
/// rig-rs 0.31 does not expose usage metadata directly, so we estimate
/// based on the ~4 characters per token average for English/code.
pub fn estimate_tokens(text: &str) -> u64 {
    (text.len() as u64).div_ceil(4)
}

/// Extract Rust code from an LLM response, stripping markdown fences if present.
///
/// Shared by translation and repair agents to avoid duplication.
pub(crate) fn extract_rust_code(response: &str) -> String {
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
}
