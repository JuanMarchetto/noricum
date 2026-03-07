use anyhow::{Context, Result};
use tracing::info;

/// Parameters for the `review` subcommand.
pub struct ReviewParams {
    pub c_path: std::path::PathBuf,
    pub rust_path: std::path::PathBuf,
    pub json: bool,
    pub diff_test: bool,
}

pub async fn cmd_review(params: ReviewParams) -> Result<()> {
    let c_source = std::fs::read_to_string(&params.c_path)
        .with_context(|| format!("cannot read C source: {}", params.c_path.display()))?;
    let rust_source = std::fs::read_to_string(&params.rust_path)
        .with_context(|| format!("cannot read Rust source: {}", params.rust_path.display()))?;

    info!(
        c_path = %params.c_path.display(),
        rust_path = %params.rust_path.display(),
        "starting behavioral equivalence review"
    );

    // Optionally run diff test first to provide context
    let diff_test_result = if params.diff_test {
        match noricum_tools::diff_test::run_diff_test(&c_source, &rust_source) {
            Ok(result) => {
                let status = if result.passed { "PASS" } else { "FAIL" };
                Some(format!(
                    "Status: {status}\nC output: {:?}\nRust output: {:?}\nC exit: {}\nRust exit: {}",
                    result.c_output, result.rust_output, result.c_exit_code, result.rust_exit_code
                ))
            }
            Err(e) => Some(format!("Diff test error: {e}")),
        }
    } else {
        None
    };

    // Set up LLM client
    let provider_config = noricum_agents::providers::ProviderConfig {
        anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
        ollama_url: std::env::var("OLLAMA_URL")
            .unwrap_or_else(|_| "http://localhost:11434".to_string()),
        ollama_model: std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".to_string()),
    };

    let client = noricum_agents::providers::create_llm_client(&provider_config)
        .map_err(|e| anyhow::anyhow!("failed to create LLM client: {e}"))?;

    let difficulty = noricum_core::router::classify_difficulty(&c_source);
    let selection =
        noricum_agents::providers::select_model(&provider_config, difficulty, "behavioral_review")
            .map_err(|e| anyhow::anyhow!("failed to select model: {e}"))?;

    let result = noricum_agents::behavioral_review::review_behavioral_equivalence(
        &client,
        &selection.model,
        &c_source,
        &rust_source,
        diff_test_result.as_deref(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("behavioral review failed: {e}"))?;

    if params.json {
        let json_str = serde_json::to_string_pretty(&result)?;
        println!("{json_str}");
    } else {
        println!("Behavioral Equivalence Review");
        println!("=============================");
        println!("  C source:    {}", params.c_path.display());
        println!("  Rust source: {}", params.rust_path.display());
        println!();
        println!("  Verdict:    {}", result.verdict);
        println!("  Confidence: {}%", result.confidence);
        println!();
        println!("Summary: {}", result.summary);
        println!();
        println!("--- Full Review ---");
        println!("{}", result.full_review);
    }

    Ok(())
}
