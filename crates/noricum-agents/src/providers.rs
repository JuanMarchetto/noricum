/// LLM provider configuration.
///
/// Manages connections to Anthropic (Claude API) and Ollama (local models).
/// Model routing selects the appropriate provider/model based on task difficulty.
use noricum_ir::Difficulty;
use rig::client::Nothing;
use rig::providers::{anthropic, ollama};
use tracing::info;

use crate::AgentError;

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
            ollama_model: "llama3.2".to_string(),
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
pub fn select_model(config: &ProviderConfig, difficulty: Difficulty, task: &str) -> ModelSelection {
    // If Anthropic is available, use it for medium/hard tasks
    if config.anthropic_api_key.is_some() {
        let model = match (difficulty, task) {
            (Difficulty::Hard, _) => models::CLAUDE_4_OPUS.to_string(),
            (Difficulty::Medium, _) | (_, "analysis") => models::CLAUDE_4_SONNET.to_string(),
            (Difficulty::Easy, _) => models::CLAUDE_3_5_HAIKU.to_string(),
        };
        info!(provider = "anthropic", model = %model, ?difficulty, "selected model");
        return ModelSelection {
            provider: ProviderKind::Anthropic,
            model,
        };
    }

    // Fallback to Ollama
    info!(provider = "ollama", model = %config.ollama_model, ?difficulty, "selected model (fallback)");
    ModelSelection {
        provider: ProviderKind::Ollama,
        model: config.ollama_model.clone(),
    }
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

        let selection = select_model(&config, Difficulty::Hard, "translation");
        assert_eq!(selection.provider, ProviderKind::Anthropic);
        assert_eq!(selection.model, "claude-opus-4-0");

        let selection = select_model(&config, Difficulty::Easy, "translation");
        assert_eq!(selection.provider, ProviderKind::Anthropic);
        assert_eq!(selection.model, models::CLAUDE_3_5_HAIKU);
    }

    #[test]
    fn test_select_model_ollama_fallback() {
        let config = ProviderConfig {
            anthropic_api_key: None,
            ..Default::default()
        };

        let selection = select_model(&config, Difficulty::Hard, "translation");
        assert_eq!(selection.provider, ProviderKind::Ollama);
        assert_eq!(selection.model, "llama3.2");
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
