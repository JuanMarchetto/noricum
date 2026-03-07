//! Behavioral equivalence review agent.
//!
//! Uses an LLM to perform deep behavioral analysis comparing original C source
//! with migrated Rust output. Goes beyond diff testing to reason about ALL
//! possible inputs, not just those covered by `main()`.

use tracing::info;

use crate::{AgentError, LlmClient};

const PREAMBLE: &str = include_str!("../../../prompts/behavioral_review.md");

/// Structured result from a behavioral equivalence review.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BehavioralReviewResult {
    /// Overall verdict: EQUIVALENT, LIKELY EQUIVALENT, DIVERGENT, or INSUFFICIENT DATA.
    pub verdict: String,
    /// Confidence percentage (0-100).
    pub confidence: u32,
    /// One-paragraph summary of the review.
    pub summary: String,
    /// The full raw review text from the LLM.
    pub full_review: String,
}

/// Run a behavioral equivalence review comparing C source with Rust output.
///
/// Requires an LLM. Returns structured review with verdict and confidence.
pub async fn review_behavioral_equivalence(
    client: &LlmClient,
    model: &str,
    c_source: &str,
    rust_source: &str,
    diff_test_result: Option<&str>,
) -> Result<BehavioralReviewResult, AgentError> {
    review_behavioral_equivalence_with_temperature(
        client,
        model,
        c_source,
        rust_source,
        diff_test_result,
        None,
    )
    .await
}

/// Run a behavioral equivalence review with configurable temperature.
pub async fn review_behavioral_equivalence_with_temperature(
    client: &LlmClient,
    model: &str,
    c_source: &str,
    rust_source: &str,
    diff_test_result: Option<&str>,
    temperature: Option<f64>,
) -> Result<BehavioralReviewResult, AgentError> {
    let c_lines = c_source.lines().count();
    let rust_lines = rust_source.lines().count();
    info!(
        c_lines,
        rust_lines, model, "starting behavioral equivalence review"
    );

    let mut message = format!(
        "<c_source>\n{c_source}\n</c_source>\n\n<rust_source>\n{rust_source}\n</rust_source>"
    );

    if let Some(diff) = diff_test_result {
        message.push_str(&format!(
            "\n\n<diff_test_result>\n{diff}\n</diff_test_result>"
        ));
    }

    let temp = temperature.unwrap_or(0.2);
    let max_tokens =
        ((c_source.len() as u64 + rust_source.len() as u64) / 4 * 2).clamp(4096, 16384);

    let response = client
        .run_prompt(model, PREAMBLE, temp, max_tokens, &message)
        .await?;

    let result = parse_review_response(&response);
    info!(verdict = %result.verdict, confidence = result.confidence, "behavioral review complete");

    Ok(result)
}

/// Parse the LLM review response into a structured result.
///
/// Extracts verdict, confidence, and summary from the markdown-formatted review.
/// Falls back to raw text if structured fields can't be parsed.
pub fn parse_review_response(response: &str) -> BehavioralReviewResult {
    let verdict = extract_field(response, "### Verdict:")
        .or_else(|| extract_field(response, "**Verdict:**"))
        .or_else(|| extract_field(response, "Verdict:"))
        .unwrap_or_else(|| classify_verdict_from_text(response));

    let confidence = extract_field(response, "### Confidence:")
        .or_else(|| extract_field(response, "**Confidence:**"))
        .or_else(|| extract_field(response, "Confidence:"))
        .and_then(|s| s.trim_end_matches('%').trim().parse::<u32>().ok())
        .unwrap_or(50);

    let summary = extract_field(response, "### Summary")
        .or_else(|| extract_field(response, "**Summary:**"))
        .unwrap_or_else(|| {
            response
                .lines()
                .find(|l| !l.trim().is_empty() && !l.starts_with('#'))
                .unwrap_or("No summary available.")
                .to_string()
        });

    BehavioralReviewResult {
        verdict,
        confidence: confidence.min(100),
        summary,
        full_review: response.to_string(),
    }
}

/// Extract the value after a field label in the response text.
fn extract_field(text: &str, label: &str) -> Option<String> {
    let idx = text.find(label)?;
    let after = &text[idx + label.len()..];
    let value = after
        .lines()
        .next()?
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_matches('*')
        .trim();
    if value.is_empty() {
        // Try the next non-empty line
        after
            .lines()
            .skip(1)
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
    } else {
        Some(value.to_string())
    }
}

/// Classify verdict from response text when structured extraction fails.
fn classify_verdict_from_text(text: &str) -> String {
    let upper = text.to_uppercase();
    if upper.contains("DIVERGENT") {
        "DIVERGENT".to_string()
    } else if upper.contains("EQUIVALENT") && upper.contains("LIKELY") {
        "LIKELY EQUIVALENT".to_string()
    } else if upper.contains("EQUIVALENT") {
        "EQUIVALENT".to_string()
    } else {
        "INSUFFICIENT DATA".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_review_structured() {
        let response = r#"## Behavioral Equivalence Review

### Verdict: EQUIVALENT

### Confidence: 95%

### Summary
The Rust translation faithfully reproduces all observable behavior of the C program.

### Findings
#### Informational
- Minor style differences in formatting."#;

        let result = parse_review_response(response);
        assert_eq!(result.verdict, "EQUIVALENT");
        assert_eq!(result.confidence, 95);
        assert!(result.summary.contains("faithfully reproduces"));
    }

    #[test]
    fn test_parse_review_divergent() {
        let response = r#"## Behavioral Equivalence Review

### Verdict: DIVERGENT

### Confidence: 88%

### Summary
The Rust code uses bool where C uses int, causing output mismatch."#;

        let result = parse_review_response(response);
        assert_eq!(result.verdict, "DIVERGENT");
        assert_eq!(result.confidence, 88);
    }

    #[test]
    fn test_parse_review_fallback() {
        let response = "This code looks equivalent to the original C implementation.";
        let result = parse_review_response(response);
        assert_eq!(result.verdict, "EQUIVALENT");
        assert_eq!(result.confidence, 50);
    }

    #[test]
    fn test_parse_review_likely_equivalent() {
        let response = r#"### Verdict: LIKELY EQUIVALENT
### Confidence: 75%
### Summary
Most paths are equivalent but edge cases around overflow are unverifiable."#;

        let result = parse_review_response(response);
        assert_eq!(result.verdict, "LIKELY EQUIVALENT");
        assert_eq!(result.confidence, 75);
    }

    #[test]
    fn test_extract_field_basic() {
        let text = "### Verdict: EQUIVALENT\n### Confidence: 90%\n";
        assert_eq!(
            extract_field(text, "### Verdict:"),
            Some("EQUIVALENT".to_string())
        );
        assert_eq!(
            extract_field(text, "### Confidence:"),
            Some("90%".to_string())
        );
    }

    #[test]
    fn test_extract_field_with_brackets() {
        let text = "### Verdict: [DIVERGENT]\n";
        assert_eq!(
            extract_field(text, "### Verdict:"),
            Some("DIVERGENT".to_string())
        );
    }

    #[test]
    fn test_classify_verdict_from_text() {
        assert_eq!(classify_verdict_from_text("This is divergent"), "DIVERGENT");
        assert_eq!(
            classify_verdict_from_text("Likely equivalent output"),
            "LIKELY EQUIVALENT"
        );
        assert_eq!(
            classify_verdict_from_text("The code is equivalent"),
            "EQUIVALENT"
        );
        assert_eq!(
            classify_verdict_from_text("Cannot determine"),
            "INSUFFICIENT DATA"
        );
    }
}
