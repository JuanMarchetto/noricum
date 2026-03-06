/// Translation Agent: converts C source code to safe, idiomatic Rust.
///
/// Takes the original C source, optional C2Rust mechanical translation output,
/// and the analysis results to produce high-quality Rust code.
/// When relevant patterns are available from the PatternStore, they are included
/// as few-shot examples in the prompt.
use noricum_ir::pattern_store::MigrationPattern;
use tracing::{debug, info};

use crate::AgentError;
use crate::analysis::AnalysisResult;
use crate::providers::LlmClient;

/// System prompt for the translation agent, loaded from the prompts directory at compile time.
const TRANSLATION_PREAMBLE: &str = include_str!("../../../prompts/translation.md");

/// Translate a C function to safe, idiomatic Rust.
///
/// Uses Claude API with the original C source, optional C2Rust output, prior
/// analysis, and relevant migration patterns (RAG) to produce the best possible
/// Rust translation.
pub async fn translate_function(
    client: &LlmClient,
    model: &str,
    c_source: &str,
    c2rust_output: Option<&str>,
    analysis: &AnalysisResult,
) -> Result<String, AgentError> {
    translate_function_with_patterns(client, model, c_source, c2rust_output, analysis, &[]).await
}

/// Translate with explicit pattern context (for testability and orchestrator integration).
pub async fn translate_function_with_patterns(
    client: &LlmClient,
    model: &str,
    c_source: &str,
    c2rust_output: Option<&str>,
    analysis: &AnalysisResult,
    patterns: &[&MigrationPattern],
) -> Result<String, AgentError> {
    translate_function_with_patterns_and_temperature(
        client,
        model,
        c_source,
        c2rust_output,
        analysis,
        patterns,
        None,
    )
    .await
}

/// Translate with pattern context and optional temperature override.
pub async fn translate_function_with_patterns_and_temperature(
    client: &LlmClient,
    model: &str,
    c_source: &str,
    c2rust_output: Option<&str>,
    analysis: &AnalysisResult,
    patterns: &[&MigrationPattern],
    temperature: Option<f64>,
) -> Result<String, AgentError> {
    let temp = temperature.unwrap_or(0.3);
    info!(
        model,
        difficulty = %analysis.difficulty,
        pattern_count = patterns.len(),
        temperature = temp,
        "starting translation"
    );

    let analysis_json = serde_json::to_string_pretty(analysis)
        .map_err(|e| AgentError::Provider(format!("failed to serialize analysis: {e}")))?;

    let mut user_message = format!(
        "## Original C source\n<c_source>\n{c_source}\n</c_source>\n\n\
         ## Analysis\n```json\n{analysis_json}\n```\n"
    );

    if let Some(c2rust) = c2rust_output {
        user_message.push_str(&format!(
            "\n## C2Rust output (unsafe Rust)\n```rust\n{c2rust}\n```\n"
        ));
    }

    if !patterns.is_empty() {
        user_message
            .push_str("\n## Relevant migration patterns (examples from past translations)\n");
        for pattern in patterns {
            user_message.push_str(&format!(
                "\n### Pattern: {}\nC:\n```c\n{}\n```\nRust:\n```rust\n{}\n```\n",
                pattern.name, pattern.c_pattern, pattern.rust_pattern
            ));
        }
    }

    user_message
        .push_str("\nTranslate the C function to safe, idiomatic Rust. Output ONLY the Rust code.");

    debug!("sending translation prompt to LLM");

    let response = client
        .run_prompt(model, TRANSLATION_PREAMBLE, temp, 8192, &user_message)
        .await?;

    debug!(
        response_len = response.len(),
        "received translation response"
    );

    Ok(crate::extract_rust_code(&response))
}
