/// LLM provider configuration.
///
/// Manages connections to Anthropic (Claude API) and Ollama (local models).
/// Model routing selects the appropriate provider/model based on task difficulty.
use noricum_ir::Difficulty;
use tracing::info;

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

/// Select a model based on task difficulty and available providers.
pub fn select_model(config: &ProviderConfig, difficulty: Difficulty, task: &str) -> ModelSelection {
    // If Anthropic is available, use it for medium/hard tasks
    if config.anthropic_api_key.is_some() {
        let model = match (difficulty, task) {
            (Difficulty::Hard, _) => "claude-opus-4-6".to_string(),
            (Difficulty::Medium, _) | (_, "analysis") => "claude-sonnet-4-6".to_string(),
            (Difficulty::Easy, _) => "claude-sonnet-4-6".to_string(),
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
        assert_eq!(selection.model, "claude-opus-4-6");

        let selection = select_model(&config, Difficulty::Easy, "translation");
        assert_eq!(selection.provider, ProviderKind::Anthropic);
        assert_eq!(selection.model, "claude-sonnet-4-6");
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
}
