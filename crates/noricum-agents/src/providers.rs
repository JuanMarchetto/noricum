/// LLM provider configuration.
///
/// Manages connections to Anthropic (Claude API), DeepSeek, and Ollama (local models).
/// Model routing selects the appropriate provider/model based on task difficulty.
use noricum_ir::Difficulty;
use rig::client::{CompletionClient, Nothing};
use rig::completion::{AssistantContent, Completion, Prompt};
use rig::providers::{anthropic, deepseek, ollama};
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
    /// Which provider to prefer ("anthropic", "deepseek", "ollama").
    pub primary_provider: String,
    /// Anthropic API key (from ANTHROPIC_API_KEY env var)
    pub anthropic_api_key: Option<String>,
    /// DeepSeek API key (from DEEPSEEK_API_KEY env var)
    pub deepseek_api_key: Option<String>,
    /// Ollama base URL (default: http://localhost:11434)
    pub ollama_url: String,
    /// Ollama model name
    pub ollama_model: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            primary_provider: "anthropic".to_string(),
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            deepseek_api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
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

/// Identifies which LLM provider backend is in use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderKind {
    /// Anthropic (Claude API).
    Anthropic,
    /// DeepSeek API.
    DeepSeek,
    /// Ollama (local models).
    Ollama,
}

/// Unified LLM client wrapping Anthropic, DeepSeek, or Ollama.
///
/// All providers share the same `run_prompt` interface via rig-rs's
/// `CompletionClient` trait. Adding a new provider requires only a new
/// enum variant and extending the match in `run_prompt`.
#[derive(Clone)]
pub enum LlmClient {
    /// Anthropic (Claude) backend.
    Anthropic(anthropic::Client),
    /// DeepSeek backend.
    DeepSeek(deepseek::Client),
    /// Ollama (local) backend.
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
            LlmClient::DeepSeek(client) => {
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
            LlmClient::DeepSeek(client) => {
                build_and_complete!(client, model, preamble, temperature, max_tokens, message)
            }
            LlmClient::Ollama(client) => {
                build_and_complete!(client, model, preamble, temperature, max_tokens, message)
            }
        }
    }
}

/// Create an LLM client based on provider config preferences.
///
/// Routes by `config.primary_provider`:
/// - `"deepseek"` + key exists -> DeepSeek client
/// - `"anthropic"` (or default) + key exists -> Anthropic client; if no anthropic key, try deepseek key
/// - Fallback: Ollama
pub fn create_llm_client(config: &ProviderConfig) -> Result<LlmClient, AgentError> {
    if config.primary_provider.as_str() == "deepseek" {
        if let Some(ref key) = config.deepseek_api_key {
            let client = create_deepseek_client_with_key(key)?;
            return Ok(client);
        }
        info!("DeepSeek preferred but no API key found, trying Anthropic fallback");
    }

    // Anthropic path (default)
    if let Some(ref key) = config.anthropic_api_key {
        let client = create_anthropic_client_with_key(key)?;
        return Ok(LlmClient::Anthropic(client));
    }

    // If no anthropic key, try deepseek as secondary
    if let Some(ref key) = config.deepseek_api_key {
        info!("No Anthropic API key, falling back to DeepSeek");
        let client = create_deepseek_client_with_key(key)?;
        return Ok(client);
    }

    // Final fallback: Ollama
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

/// Create a DeepSeek client from an explicit API key string.
pub fn create_deepseek_client_with_key(api_key: &str) -> Result<LlmClient, AgentError> {
    let client = deepseek::Client::new(api_key)
        .map_err(|e| AgentError::Provider(format!("failed to create DeepSeek client: {e}")))?;
    Ok(LlmClient::DeepSeek(client))
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

/// Model constants for supported providers.
pub mod models {
    pub use rig::providers::anthropic::completion::{
        CLAUDE_3_5_HAIKU, CLAUDE_3_5_SONNET, CLAUDE_3_7_SONNET, CLAUDE_4_OPUS, CLAUDE_4_SONNET,
    };

    /// Claude Opus 4.6 — latest and most capable model (not yet in rig-rs constants).
    pub const CLAUDE_4_6_OPUS: &str = "claude-opus-4-6";
    /// Claude Sonnet 4.6.
    pub const CLAUDE_4_6_SONNET: &str = "claude-sonnet-4-6";

    /// DeepSeek Chat — general-purpose model.
    pub const DEEPSEEK_CHAT: &str = "deepseek-chat";
    /// DeepSeek Reasoner — advanced reasoning model.
    pub const DEEPSEEK_REASONER: &str = "deepseek-reasoner";
}

/// Select a model based on task difficulty and available providers.
///
/// Checks `primary_provider` first:
/// - `"deepseek"` with key -> DeepSeek models (Hard -> reasoner, others -> chat)
/// - `"anthropic"` (default) with key -> Anthropic models
/// - Fallback: Ollama with the configured model name.
pub fn select_model(
    config: &ProviderConfig,
    difficulty: Difficulty,
    task: &str,
) -> Result<ModelSelection, crate::AgentError> {
    // DeepSeek preferred
    if config.primary_provider == "deepseek" && config.deepseek_api_key.is_some() {
        let model = match difficulty {
            Difficulty::Hard => models::DEEPSEEK_REASONER.to_string(),
            Difficulty::Medium | Difficulty::Easy => models::DEEPSEEK_CHAT.to_string(),
        };
        info!(provider = "deepseek", model = %model, ?difficulty, "selected model");
        return Ok(ModelSelection {
            provider: ProviderKind::DeepSeek,
            model,
        });
    }

    // Anthropic path
    if config.anthropic_api_key.is_some() {
        let model = match (difficulty, task) {
            (Difficulty::Hard, _) => models::CLAUDE_4_6_OPUS.to_string(),
            (Difficulty::Medium, _) | (_, "analysis") => models::CLAUDE_4_6_SONNET.to_string(),
            (Difficulty::Easy, _) => "claude-haiku-4-5-20251001".to_string(),
        };
        info!(provider = "anthropic", model = %model, ?difficulty, "selected model");
        return Ok(ModelSelection {
            provider: ProviderKind::Anthropic,
            model,
        });
    }

    // DeepSeek fallback (when not primary but key is available and no anthropic key)
    if config.deepseek_api_key.is_some() {
        let model = match difficulty {
            Difficulty::Hard => models::DEEPSEEK_REASONER.to_string(),
            Difficulty::Medium | Difficulty::Easy => models::DEEPSEEK_CHAT.to_string(),
        };
        info!(provider = "deepseek", model = %model, ?difficulty, "selected model (DeepSeek fallback)");
        return Ok(ModelSelection {
            provider: ProviderKind::DeepSeek,
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

/// Select model for repair calls — prefers faster models since repair
/// is about fixing compilation errors, not deep reasoning.
/// For DeepSeek: always uses deepseek-chat (fast, cheap).
/// For other providers: uses the normal selection logic.
pub fn select_repair_model(
    config: &ProviderConfig,
    difficulty: Difficulty,
) -> Result<ModelSelection, crate::AgentError> {
    if config.primary_provider == "deepseek" && config.deepseek_api_key.is_some() {
        info!(
            provider = "deepseek",
            model = models::DEEPSEEK_CHAT,
            "P16: using fast model for repair"
        );
        return Ok(ModelSelection {
            provider: ProviderKind::DeepSeek,
            model: models::DEEPSEEK_CHAT.to_string(),
        });
    }
    // For non-DeepSeek providers, use normal model selection
    select_model(config, difficulty, "repair")
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
        assert_eq!(selection.model, "claude-opus-4-6");

        let selection = select_model(&config, Difficulty::Easy, "translation").unwrap();
        assert_eq!(selection.provider, ProviderKind::Anthropic);
        assert_eq!(selection.model, "claude-haiku-4-5-20251001");
    }

    #[test]
    fn test_select_model_falls_through_to_ollama() {
        let config = ProviderConfig {
            anthropic_api_key: None,
            deepseek_api_key: None,
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
            deepseek_api_key: None,
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

    #[test]
    fn test_create_deepseek_client_ok() {
        let result = create_deepseek_client_with_key("test-key");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), LlmClient::DeepSeek(_)));
    }

    #[test]
    fn test_select_model_with_deepseek() {
        let config = ProviderConfig {
            primary_provider: "deepseek".to_string(),
            deepseek_api_key: Some("test-key".to_string()),
            anthropic_api_key: None,
            ..Default::default()
        };
        let sel = select_model(&config, Difficulty::Hard, "translation").unwrap();
        assert_eq!(sel.provider, ProviderKind::DeepSeek);
        assert_eq!(sel.model, "deepseek-reasoner");

        let sel = select_model(&config, Difficulty::Easy, "translation").unwrap();
        assert_eq!(sel.provider, ProviderKind::DeepSeek);
        assert_eq!(sel.model, "deepseek-chat");
    }

    #[test]
    fn test_select_repair_model_uses_fast_model() {
        let config = ProviderConfig {
            primary_provider: "deepseek".to_string(),
            deepseek_api_key: Some("test-key".to_string()),
            anthropic_api_key: None,
            ..Default::default()
        };
        // Repair should use deepseek-chat even for Hard difficulty
        let sel = select_repair_model(&config, Difficulty::Hard).unwrap();
        assert_eq!(sel.model, "deepseek-chat");
        assert_eq!(sel.provider, ProviderKind::DeepSeek);

        // For Anthropic, repair uses normal selection
        let anthropic_config = ProviderConfig {
            anthropic_api_key: Some("test-key".to_string()),
            ..Default::default()
        };
        let sel = select_repair_model(&anthropic_config, Difficulty::Hard).unwrap();
        assert_eq!(sel.provider, ProviderKind::Anthropic);
    }

    #[test]
    fn test_create_llm_client_deepseek_preferred() {
        let config = ProviderConfig {
            primary_provider: "deepseek".to_string(),
            deepseek_api_key: Some("test-key".to_string()),
            anthropic_api_key: None,
            ..Default::default()
        };
        let client = create_llm_client(&config);
        assert!(client.is_ok());
        assert!(matches!(client.unwrap(), LlmClient::DeepSeek(_)));
    }
}
