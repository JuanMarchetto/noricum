/// Analysis Agent: examines C source code and produces a structured assessment
/// for migration to Rust.
///
/// The agent classifies difficulty, identifies C patterns, suggests Rust equivalents,
/// lists dependencies, flags risks, and recommends a migration strategy.
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::anthropic;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::AgentError;

/// System prompt for the analysis agent, loaded from the prompts directory at compile time.
const ANALYSIS_PREAMBLE: &str = include_str!("../../../prompts/analysis.md");

/// Structured result from the analysis agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    /// Migration difficulty: "easy", "medium", or "hard"
    pub difficulty: String,
    /// C patterns detected (e.g., "ptr_arithmetic", "error_codes", "malloc_free")
    pub patterns: Vec<String>,
    /// Map of C pattern to suggested Rust approach
    pub rust_equivalents: std::collections::HashMap<String, String>,
    /// Function names this function depends on
    pub dependencies: Vec<String>,
    /// Potential migration issues
    pub risks: Vec<String>,
    /// Brief migration strategy recommendation
    pub strategy: String,
}

/// Analyze a C function and return structured migration assessment.
///
/// Uses Claude API to examine the C source and produce an `AnalysisResult` with
/// difficulty classification, pattern detection, risk assessment, and strategy.
///
/// # Arguments
/// * `client` - Anthropic API client
/// * `model` - Model identifier (e.g., "claude-sonnet-4-0")
/// * `c_source` - The C source code to analyze
/// * `function_name` - Name of the function being analyzed
///
/// # Errors
/// Returns `AgentError::Provider` if the LLM call fails, or `AgentError::Parse`
/// if the response cannot be parsed as JSON.
pub async fn analyze_function(
    client: &anthropic::Client,
    model: &str,
    c_source: &str,
    function_name: &str,
) -> Result<AnalysisResult, AgentError> {
    analyze_function_with_temperature(client, model, c_source, function_name, None).await
}

/// Analyze with an optional temperature override.
pub async fn analyze_function_with_temperature(
    client: &anthropic::Client,
    model: &str,
    c_source: &str,
    function_name: &str,
    temperature: Option<f64>,
) -> Result<AnalysisResult, AgentError> {
    let temp = temperature.unwrap_or(0.2);
    info!(
        function = function_name,
        model,
        temperature = temp,
        "starting analysis"
    );

    let agent = client
        .agent(model)
        .preamble(ANALYSIS_PREAMBLE)
        .temperature(temp)
        .max_tokens(4096)
        .build();

    let user_message = format!(
        "Analyze the following C function named `{function_name}` for migration to Rust.\n\
         Respond ONLY with the JSON object as specified in the output format.\n\n\
         <c_source>\n{c_source}\n</c_source>"
    );

    debug!(function = function_name, "sending analysis prompt to LLM");

    let response = agent
        .prompt(&user_message)
        .await
        .map_err(|e| AgentError::Provider(format!("analysis LLM call failed: {e}")))?;

    debug!(
        function = function_name,
        response_len = response.len(),
        "received analysis response"
    );

    parse_analysis_response(&response)
}

/// Extract JSON from the LLM response, handling markdown code fences.
pub fn parse_analysis_response(response: &str) -> Result<AnalysisResult, AgentError> {
    // Try to extract JSON from markdown code fences first
    let json_str = if let Some(start) = response.find("```json") {
        let after_fence = &response[start + 7..];
        if let Some(end) = after_fence.find("```") {
            after_fence[..end].trim()
        } else {
            response.trim()
        }
    } else if let Some(start) = response.find("```") {
        let after_fence = &response[start + 3..];
        if let Some(end) = after_fence.find("```") {
            after_fence[..end].trim()
        } else {
            response.trim()
        }
    } else {
        response.trim()
    };

    serde_json::from_str(json_str).map_err(|e| {
        AgentError::Parse(format!(
            "failed to parse analysis JSON: {e}\nRaw response:\n{response}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_analysis_response_raw_json() {
        let json = r#"{
            "difficulty": "easy",
            "patterns": ["pure_function"],
            "rust_equivalents": {"pure_function": "direct translation"},
            "dependencies": [],
            "risks": [],
            "strategy": "Direct translation"
        }"#;

        let result = parse_analysis_response(json).unwrap();
        assert_eq!(result.difficulty, "easy");
        assert_eq!(result.patterns, vec!["pure_function"]);
        assert!(result.risks.is_empty());
    }

    #[test]
    fn test_parse_analysis_response_with_code_fence() {
        let response = "Here is the analysis:\n```json\n{\
            \"difficulty\": \"medium\",\
            \"patterns\": [\"ptr_arithmetic\"],\
            \"rust_equivalents\": {\"ptr_arithmetic\": \"slice indexing\"},\
            \"dependencies\": [\"helper_func\"],\
            \"risks\": [\"buffer overflow\"],\
            \"strategy\": \"Use slices\"\
        }\n```\n";

        let result = parse_analysis_response(response).unwrap();
        assert_eq!(result.difficulty, "medium");
        assert_eq!(result.dependencies, vec!["helper_func"]);
    }

    #[test]
    fn test_parse_analysis_response_invalid_json() {
        let result = parse_analysis_response("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_analysis_complex() {
        let result = parse_analysis_response(crate::mock_responses::MOCK_ANALYSIS_COMPLEX).unwrap();
        assert_eq!(result.difficulty, "hard");
        assert!(result.patterns.contains(&"malloc_free".to_string()));
        assert!(result.patterns.contains(&"ptr_arithmetic".to_string()));
        assert!(!result.risks.is_empty());
        assert!(!result.strategy.is_empty());
        assert!(result.dependencies.contains(&"helper_alloc".to_string()));
    }

    #[test]
    fn test_parse_analysis_malformed() {
        let result = parse_analysis_response(crate::mock_responses::MOCK_ANALYSIS_MALFORMED);
        assert!(
            result.is_err(),
            "malformed (non-JSON) response should return error"
        );
        let err = result.unwrap_err();
        assert!(
            matches!(err, crate::AgentError::Parse(_)),
            "should be a Parse error"
        );
    }

    #[test]
    fn test_parse_analysis_simple_mock() {
        let result = parse_analysis_response(crate::mock_responses::MOCK_ANALYSIS_SIMPLE).unwrap();
        assert_eq!(result.difficulty, "easy");
        assert_eq!(result.patterns, vec!["pure_function"]);
        assert!(result.risks.is_empty());
    }

    #[test]
    fn test_parse_analysis_empty_fields() {
        let result =
            parse_analysis_response(crate::mock_responses::MOCK_ANALYSIS_EMPTY_FIELDS).unwrap();
        assert_eq!(result.difficulty, "easy");
        assert!(result.patterns.is_empty());
        assert!(result.strategy.is_empty());
    }

    #[test]
    fn test_parse_analysis_missing_field() {
        let result = parse_analysis_response(crate::mock_responses::MOCK_ANALYSIS_MISSING_FIELD);
        assert!(
            result.is_err(),
            "missing required fields should fail parsing"
        );
    }
}
