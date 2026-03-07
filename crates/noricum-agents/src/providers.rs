/// LLM provider configuration.
///
/// Manages connections to Anthropic (Claude API) and Ollama (local models).
/// Model routing selects the appropriate provider/model based on task difficulty.
use noricum_ir::Difficulty;
use rig::client::{CompletionClient, Nothing};
use rig::completion::{AssistantContent, Completion, Prompt};
use rig::providers::{anthropic, ollama};
use tracing::info;

use crate::AgentError;

/// Actual token usage from an LLM API response.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    /// Input (prompt) tokens reported by the API.
    pub input_tokens: u64,
    /// Output (completion) tokens reported by the API.
    pub output_tokens: u64,
}

/// Configuration for LLM providers.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Anthropic API key (from ANTHROPIC_API_KEY env var)
    pub anthropic_api_key: Option<String>,
    /// Ollama base URL (default: http://localhost:11434)
    pub ollama_url: String,
    /// Ollama model name
    pub ollama_model: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            ollama_url: "http://localhost:11434".to_string(),
            ollama_model: "qwen2.5-coder:32b".to_string(),
        }
    }
}

/// Which model to use for a given task.
#[derive(Debug, Clone)]
pub struct ModelSelection {
    pub provider: ProviderKind,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    Ollama,
}

/// Unified LLM client wrapping either Anthropic or Ollama.
///
/// Both providers share the same `run_prompt` interface via rig-rs's
/// `CompletionClient` trait. Adding a new provider (e.g., Gemini) requires
/// only a new enum variant and extending the match in `run_prompt`.
pub enum LlmClient {
    Anthropic(anthropic::Client),
    Ollama(ollama::Client),
}

/// Build an agent from a rig-rs `CompletionClient` and prompt it.
macro_rules! build_and_prompt {
    ($client:expr, $model:expr, $preamble:expr, $temp:expr, $max:expr, $msg:expr) => {
        $client
            .agent($model)
            .preamble($preamble)
            .temperature($temp)
            .max_tokens($max)
            .build()
            .prompt($msg)
            .await
            .map_err(|e| AgentError::Provider(format!("LLM call failed: {e}")))
    };
}

/// Build an agent, call `completion()` + `send()`, and return text + usage.
macro_rules! build_and_complete {
    ($client:expr, $model:expr, $preamble:expr, $temp:expr, $max:expr, $msg:expr) => {{
        let agent = $client
            .agent($model)
            .preamble($preamble)
            .temperature($temp)
            .max_tokens($max)
            .build();
        let builder = agent
            .completion($msg, vec![])
            .await
            .map_err(|e| AgentError::Provider(format!("LLM completion build failed: {e}")))?;
        let response = builder
            .send()
            .await
            .map_err(|e| AgentError::Provider(format!("LLM call failed: {e}")))?;
        let text = match response.choice.first() {
            AssistantContent::Text(t) => t.text().to_string(),
            _ => String::new(),
        };
        let usage = TokenUsage {
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
        };
        Ok((text, usage))
    }};
}

impl LlmClient {
    /// Build an agent with the given config and run a prompt.
    pub async fn run_prompt(
        &self,
        model: &str,
        preamble: &str,
        temperature: f64,
        max_tokens: u64,
        message: &str,
    ) -> Result<String, AgentError> {
        match self {
            LlmClient::Anthropic(client) => {
                build_and_prompt!(client, model, preamble, temperature, max_tokens, message)
            }
            LlmClient::Ollama(client) => {
                build_and_prompt!(client, model, preamble, temperature, max_tokens, message)
            }
        }
    }

    /// Build an agent, run a prompt, and return actual token usage from the API.
    ///
    /// Unlike `run_prompt`, this uses the lower-level `completion()` API to
    /// capture the real `input_tokens` and `output_tokens` reported by the
    /// provider, rather than relying on the `estimate_tokens()` heuristic.
    pub async fn run_prompt_with_usage(
        &self,
        model: &str,
        preamble: &str,
        temperature: f64,
        max_tokens: u64,
        message: &str,
    ) -> Result<(String, TokenUsage), AgentError> {
        match self {
            LlmClient::Anthropic(client) => {
                build_and_complete!(client, model, preamble, temperature, max_tokens, message)
            }
            LlmClient::Ollama(client) => {
                build_and_complete!(client, model, preamble, temperature, max_tokens, message)
            }
        }
    }
}

/// Create an LLM client, preferring Anthropic if a key is available, falling back to Ollama.
pub fn create_llm_client(config: &ProviderConfig) -> Result<LlmClient, AgentError> {
    if let Some(ref key) = config.anthropic_api_key {
        let client = create_anthropic_client_with_key(key)?;
        return Ok(LlmClient::Anthropic(client));
    }
    let client = create_ollama_client_with_url(&config.ollama_url)?;
    Ok(LlmClient::Ollama(client))
}

/// Create an Anthropic client from an API key.
///
/// Returns `AgentError::Provider` if the key is missing or the client cannot be built.
pub fn create_anthropic_client() -> Result<anthropic::Client, AgentError> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        AgentError::Provider("ANTHROPIC_API_KEY environment variable not set".into())
    })?;

    anthropic::Client::new(api_key)
        .map_err(|e| AgentError::Provider(format!("failed to create Anthropic client: {e}")))
}

/// Create an Anthropic client from an explicit API key string.
pub fn create_anthropic_client_with_key(api_key: &str) -> Result<anthropic::Client, AgentError> {
    anthropic::Client::new(api_key)
        .map_err(|e| AgentError::Provider(format!("failed to create Anthropic client: {e}")))
}

/// Create an Ollama client.
///
/// Uses the default Ollama URL (http://localhost:11434). For custom URLs,
/// use `create_ollama_client_with_url`.
pub fn create_ollama_client() -> Result<ollama::Client, AgentError> {
    ollama::Client::new(Nothing)
        .map_err(|e| AgentError::Provider(format!("failed to create Ollama client: {e}")))
}

/// Create an Ollama client pointing to a custom URL.
pub fn create_ollama_client_with_url(url: &str) -> Result<ollama::Client, AgentError> {
    ollama::Client::builder()
        .api_key(Nothing)
        .base_url(url)
        .build()
        .map_err(|e| AgentError::Provider(format!("failed to create Ollama client at {url}: {e}")))
}

/// Model constants for Anthropic.
pub mod models {
    pub use rig::providers::anthropic::completion::{
        CLAUDE_3_5_HAIKU, CLAUDE_3_5_SONNET, CLAUDE_3_7_SONNET, CLAUDE_4_OPUS, CLAUDE_4_SONNET,
    };
}

/// Select a model based on task difficulty and available providers.
///
/// Prefers Anthropic if an API key is configured; otherwise falls through
/// to Ollama with the configured model name.
pub fn select_model(
    config: &ProviderConfig,
    difficulty: Difficulty,
    task: &str,
) -> Result<ModelSelection, crate::AgentError> {
    if config.anthropic_api_key.is_some() {
        let model = match (difficulty, task) {
            (Difficulty::Hard, _) => models::CLAUDE_4_OPUS.to_string(),
            (Difficulty::Medium, _) | (_, "analysis") => models::CLAUDE_4_SONNET.to_string(),
            (Difficulty::Easy, _) => "claude-haiku-4-5-20251001".to_string(),
        };
        info!(provider = "anthropic", model = %model, ?difficulty, "selected model");
        return Ok(ModelSelection {
            provider: ProviderKind::Anthropic,
            model,
        });
    }

    // Fall through to Ollama
    info!(provider = "ollama", model = %config.ollama_model, ?difficulty, "selected model (Ollama fallback)");
    Ok(ModelSelection {
        provider: ProviderKind::Ollama,
        model: config.ollama_model.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_model_with_anthropic() {
        let config = ProviderConfig {
            anthropic_api_key: Some("test-key".to_string()),
            ..Default::default()
        };

        let selection = select_model(&config, Difficulty::Hard, "translation").unwrap();
        assert_eq!(selection.provider, ProviderKind::Anthropic);
        assert_eq!(selection.model, "claude-opus-4-0");

        let selection = select_model(&config, Difficulty::Easy, "translation").unwrap();
        assert_eq!(selection.provider, ProviderKind::Anthropic);
        assert_eq!(selection.model, "claude-haiku-4-5-20251001");
    }

    #[test]
    fn test_select_model_falls_through_to_ollama() {
        let config = ProviderConfig {
            anthropic_api_key: None,
            ..Default::default()
        };

        let selection = select_model(&config, Difficulty::Hard, "translation").unwrap();
        assert_eq!(selection.provider, ProviderKind::Ollama);
        assert_eq!(selection.model, "qwen2.5-coder:32b");
    }

    #[test]
    fn test_create_llm_client_ollama_fallback() {
        let config = ProviderConfig {
            anthropic_api_key: None,
            ..Default::default()
        };
        let client = create_llm_client(&config);
        assert!(client.is_ok());
        assert!(matches!(client.unwrap(), LlmClient::Ollama(_)));
    }

    #[test]
    fn test_create_llm_client_anthropic_preferred() {
        let config = ProviderConfig {
            anthropic_api_key: Some("test-key".to_string()),
            ..Default::default()
        };
        let client = create_llm_client(&config);
        assert!(client.is_ok());
        assert!(matches!(client.unwrap(), LlmClient::Anthropic(_)));
    }

    #[test]
    fn test_create_anthropic_client_no_key() {
        // Test the error path by checking env var directly.
        // If ANTHROPIC_API_KEY is not set, create_anthropic_client() should error.
        // If it IS set (e.g., in CI or dev), we still validate the with_key path.
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            let result = create_anthropic_client();
            assert!(result.is_err());
        }
        // Validate that create_anthropic_client_with_key works with a test key
        let result = create_anthropic_client_with_key("test-key-for-unit-test");
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_ollama_client_ok() {
        let result = create_ollama_client();
        assert!(result.is_ok());
    }
}
