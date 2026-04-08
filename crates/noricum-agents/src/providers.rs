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
    /// Ollama context window size (num_ctx). Default: 131072 (128k).
    pub ollama_num_ctx: u64,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            primary_provider: "anthropic".to_string(),
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            deepseek_api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
            ollama_url: "http://localhost:11434".to_string(),
            ollama_model: "qwen2.5-coder:32b".to_string(),
            ollama_num_ctx: 131072,
        }
    }
}

/// P35: Per-task LLM configuration override.
///
/// Each field is optional — `None` means "use the default from ProviderConfig".
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TaskModelConfig {
    /// Provider: "anthropic", "deepseek", "ollama".
    pub provider: Option<String>,
    /// Model identifier (e.g., "claude-opus-4-6", "gemma4:26b-a4b-it-q8_0").
    pub model: Option<String>,
    /// Temperature override.
    pub temperature: Option<f64>,
    /// Ollama context window size override (num_ctx).
    pub num_ctx: Option<u64>,
}

/// P35: Pipeline task identifiers for per-task routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PipelineTask {
    Analysis,
    TypeContract,
    Translation,
    Repair,
    Completion,
    TestGen,
}

impl std::fmt::Display for PipelineTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineTask::Analysis => write!(f, "analysis"),
            PipelineTask::TypeContract => write!(f, "type_contract"),
            PipelineTask::Translation => write!(f, "translation"),
            PipelineTask::Repair => write!(f, "repair"),
            PipelineTask::Completion => write!(f, "completion"),
            PipelineTask::TestGen => write!(f, "test_gen"),
        }
    }
}

impl std::str::FromStr for PipelineTask {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "analysis" => Ok(PipelineTask::Analysis),
            "type_contract" | "type-contract" | "contract" => Ok(PipelineTask::TypeContract),
            "translation" | "translate" => Ok(PipelineTask::Translation),
            "repair" | "fix" => Ok(PipelineTask::Repair),
            "completion" | "complete" => Ok(PipelineTask::Completion),
            "test_gen" | "test-gen" | "testgen" | "test" => Ok(PipelineTask::TestGen),
            _ => Err(format!("unknown pipeline task: {s}")),
        }
    }
}

/// P35: Per-task model routing table.
///
/// Maps each pipeline task to an optional `TaskModelConfig`. When a task has
/// no override, the default `ProviderConfig` + difficulty-based routing is used.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TaskRouting {
    pub analysis: Option<TaskModelConfig>,
    pub type_contract: Option<TaskModelConfig>,
    pub translation: Option<TaskModelConfig>,
    pub repair: Option<TaskModelConfig>,
    pub completion: Option<TaskModelConfig>,
    pub test_gen: Option<TaskModelConfig>,
}

impl TaskRouting {
    /// Get the config for a specific task.
    pub fn get(&self, task: PipelineTask) -> Option<&TaskModelConfig> {
        match task {
            PipelineTask::Analysis => self.analysis.as_ref(),
            PipelineTask::TypeContract => self.type_contract.as_ref(),
            PipelineTask::Translation => self.translation.as_ref(),
            PipelineTask::Repair => self.repair.as_ref(),
            PipelineTask::Completion => self.completion.as_ref(),
            PipelineTask::TestGen => self.test_gen.as_ref(),
        }
    }

    /// Set the config for a specific task.
    pub fn set(&mut self, task: PipelineTask, config: TaskModelConfig) {
        match task {
            PipelineTask::Analysis => self.analysis = Some(config),
            PipelineTask::TypeContract => self.type_contract = Some(config),
            PipelineTask::Translation => self.translation = Some(config),
            PipelineTask::Repair => self.repair = Some(config),
            PipelineTask::Completion => self.completion = Some(config),
            PipelineTask::TestGen => self.test_gen = Some(config),
        }
    }

    /// Returns true if any task has a routing override.
    pub fn has_overrides(&self) -> bool {
        self.analysis.is_some()
            || self.type_contract.is_some()
            || self.translation.is_some()
            || self.repair.is_some()
            || self.completion.is_some()
            || self.test_gen.is_some()
    }
}

/// Which model to use for a given task.
#[derive(Debug, Clone)]
pub struct ModelSelection {
    /// The provider backend (Anthropic, DeepSeek, or Ollama).
    pub provider: ProviderKind,
    /// The model identifier string (e.g., "claude-opus-4-6", "deepseek-chat").
    pub model: String,
}

/// Identifies which LLM provider backend is in use.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// Ollama (local) backend with context window size.
    Ollama(ollama::Client, u64),
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
    ($client:expr, $model:expr, $preamble:expr, $temp:expr, $max:expr, $msg:expr, $params:expr) => {
        $client
            .agent($model)
            .preamble($preamble)
            .temperature($temp)
            .max_tokens($max)
            .additional_params($params)
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
    ($client:expr, $model:expr, $preamble:expr, $temp:expr, $max:expr, $msg:expr, $params:expr) => {{
        let agent = $client
            .agent($model)
            .preamble($preamble)
            .temperature($temp)
            .max_tokens($max)
            .additional_params($params)
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
            LlmClient::Ollama(client, num_ctx) => {
                let params = serde_json::json!({ "num_ctx": num_ctx });
                build_and_prompt!(client, model, preamble, temperature, max_tokens, message, params)
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
            LlmClient::Ollama(client, num_ctx) => {
                let params = serde_json::json!({ "num_ctx": num_ctx });
                build_and_complete!(client, model, preamble, temperature, max_tokens, message, params)
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
    // Ollama explicitly requested — skip all API providers
    if config.primary_provider.as_str() == "ollama" {
        info!(model = %config.ollama_model, num_ctx = config.ollama_num_ctx, "Ollama explicitly selected");
        let client = create_ollama_client_with_url(&config.ollama_url)?;
        return Ok(LlmClient::Ollama(client, config.ollama_num_ctx));
    }

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
    Ok(LlmClient::Ollama(client, config.ollama_num_ctx))
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

/// P35: Pool of LLM clients — holds one client per active provider.
///
/// Created from a `ProviderConfig` by initializing all available providers.
/// The orchestrator calls `resolve_task()` to get the right client + model
/// for each pipeline stage, enabling mixed-provider migrations (e.g., Ollama
/// for analysis, Claude for translation).
#[derive(Clone)]
pub struct LlmClientPool {
    anthropic: Option<LlmClient>,
    deepseek: Option<LlmClient>,
    ollama: Option<LlmClient>,
    config: ProviderConfig,
}

impl LlmClientPool {
    /// Build a pool from provider config, creating all available clients.
    pub fn from_config(config: &ProviderConfig) -> Result<Self, AgentError> {
        let anthropic = config
            .anthropic_api_key
            .as_ref()
            .and_then(|key| create_anthropic_client_with_key(key).ok().map(LlmClient::Anthropic));

        let deepseek = config
            .deepseek_api_key
            .as_ref()
            .and_then(|key| create_deepseek_client_with_key(key).ok());

        let ollama = create_ollama_client_with_url(&config.ollama_url)
            .ok()
            .map(|c| LlmClient::Ollama(c, config.ollama_num_ctx));

        if anthropic.is_none() && deepseek.is_none() && ollama.is_none() {
            return Err(AgentError::Provider(
                "no LLM providers available (no API keys, Ollama unreachable)".into(),
            ));
        }

        Ok(Self {
            anthropic,
            deepseek,
            ollama,
            config: config.clone(),
        })
    }

    /// Get a client by provider name. Falls back to the default client.
    pub fn get_by_provider(&self, provider: &str) -> Option<&LlmClient> {
        match provider {
            "anthropic" => self.anthropic.as_ref(),
            "deepseek" => self.deepseek.as_ref(),
            "ollama" => self.ollama.as_ref(),
            _ => None,
        }
    }

    /// Get the default client (same logic as `create_llm_client`).
    pub fn default_client(&self) -> Option<&LlmClient> {
        match self.config.primary_provider.as_str() {
            "ollama" => self.ollama.as_ref(),
            "deepseek" => self
                .deepseek
                .as_ref()
                .or(self.anthropic.as_ref())
                .or(self.ollama.as_ref()),
            _ => self
                .anthropic
                .as_ref()
                .or(self.deepseek.as_ref())
                .or(self.ollama.as_ref()),
        }
    }

    /// Get the underlying `ProviderConfig`.
    pub fn config(&self) -> &ProviderConfig {
        &self.config
    }
}

/// P35: Resolved configuration for a single pipeline task.
///
/// Contains everything needed to make an LLM call: the client, model name,
/// temperature, and provider kind. Produced by `resolve_task()`.
#[derive(Clone)]
pub struct ResolvedTask {
    /// The LLM client to use.
    pub client: LlmClient,
    /// Model identifier.
    pub model: String,
    /// Temperature for this call.
    pub temperature: f64,
    /// Which provider this resolves to.
    pub provider: ProviderKind,
}

/// P35: Resolve the LLM client + model + temperature for a pipeline task.
///
/// Priority order:
/// 1. Per-task override from `TaskRouting` (if set)
/// 2. Default `select_model()` logic based on difficulty + provider config
///
/// This is the single entry point for all model selection in the pipeline.
pub fn resolve_task(
    pool: &LlmClientPool,
    routing: &TaskRouting,
    task: PipelineTask,
    difficulty: Difficulty,
    default_temperature: f64,
) -> Result<ResolvedTask, AgentError> {
    // Check for per-task override
    if let Some(task_config) = routing.get(task) {
        let temperature = task_config.temperature.unwrap_or(default_temperature);

        // Resolve provider + model from task config
        if let Some(ref provider_name) = task_config.provider {
            let client = pool
                .get_by_provider(provider_name)
                .ok_or_else(|| {
                    AgentError::Provider(format!(
                        "P35: task {task} requests provider '{provider_name}' but it is not available"
                    ))
                })?
                .clone();

            // If task specifies a different num_ctx for Ollama, patch it
            let client = if provider_name == "ollama" {
                if let Some(num_ctx) = task_config.num_ctx {
                    if let LlmClient::Ollama(ollama_client, _) = &client {
                        LlmClient::Ollama(ollama_client.clone(), num_ctx)
                    } else {
                        client
                    }
                } else {
                    client
                }
            } else {
                client
            };

            let provider = match provider_name.as_str() {
                "anthropic" => ProviderKind::Anthropic,
                "deepseek" => ProviderKind::DeepSeek,
                _ => ProviderKind::Ollama,
            };

            // Model: use task override, or provider's default for the difficulty
            let model = task_config.model.clone().unwrap_or_else(|| {
                let config = pool.config();
                match provider {
                    ProviderKind::Ollama => config.ollama_model.clone(),
                    ProviderKind::DeepSeek => match difficulty {
                        Difficulty::Hard => models::DEEPSEEK_REASONER.to_string(),
                        _ => models::DEEPSEEK_CHAT.to_string(),
                    },
                    ProviderKind::Anthropic => match difficulty {
                        Difficulty::Hard => models::CLAUDE_4_6_OPUS.to_string(),
                        Difficulty::Medium => models::CLAUDE_4_6_SONNET.to_string(),
                        Difficulty::Easy => "claude-haiku-4-5-20251001".to_string(),
                    },
                }
            });

            info!(
                task = %task,
                provider = provider_name,
                model = %model,
                temperature,
                "P35: resolved task (per-task override)"
            );

            return Ok(ResolvedTask {
                client,
                model,
                temperature,
                provider,
            });
        }

        // Only model/temperature override, use default provider
        let sel = select_model(pool.config(), difficulty, &task.to_string())?;
        let client = pool
            .get_by_provider(match sel.provider {
                ProviderKind::Anthropic => "anthropic",
                ProviderKind::DeepSeek => "deepseek",
                ProviderKind::Ollama => "ollama",
            })
            .ok_or_else(|| {
                AgentError::Provider(format!("provider {:?} not available in pool", sel.provider))
            })?
            .clone();

        let model = task_config.model.clone().unwrap_or(sel.model);

        info!(
            task = %task,
            provider = ?sel.provider,
            model = %model,
            temperature,
            "P35: resolved task (partial override)"
        );

        return Ok(ResolvedTask {
            client,
            model,
            temperature,
            provider: sel.provider,
        });
    }

    // No override — use default routing
    let sel = select_model(pool.config(), difficulty, &task.to_string())?;
    let client = pool
        .get_by_provider(match sel.provider {
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::DeepSeek => "deepseek",
            ProviderKind::Ollama => "ollama",
        })
        .ok_or_else(|| {
            AgentError::Provider(format!("provider {:?} not available in pool", sel.provider))
        })?
        .clone();

    Ok(ResolvedTask {
        client,
        model: sel.model,
        temperature: default_temperature,
        provider: sel.provider,
    })
}

/// Parse a task routing spec string: "task:provider/model/temp/num_ctx"
///
/// Examples:
///   "analysis:ollama/gemma4:26b/0.2/131072"
///   "translation:anthropic/claude-opus-4-6/0.3"
///   "repair:ollama/gemma4:26b"
///   "test_gen:deepseek"
pub fn parse_task_routing_spec(spec: &str) -> Result<(PipelineTask, TaskModelConfig), String> {
    let parts: Vec<&str> = spec.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(format!(
            "invalid task routing spec '{spec}': expected 'task:provider/model/temp/num_ctx'"
        ));
    }

    let task: PipelineTask = parts[0].parse()?;
    let config_parts: Vec<&str> = parts[1].split('/').collect();

    let provider = Some(config_parts[0].to_string());
    let model = config_parts.get(1).filter(|s| !s.is_empty()).map(|s| s.to_string());
    let temperature = config_parts
        .get(2)
        .and_then(|s| s.parse::<f64>().ok());
    let num_ctx = config_parts
        .get(3)
        .and_then(|s| s.parse::<u64>().ok());

    Ok((
        task,
        TaskModelConfig {
            provider,
            model,
            temperature,
            num_ctx,
        },
    ))
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
    // Ollama explicitly requested — skip all API providers
    if config.primary_provider == "ollama" {
        info!(provider = "ollama", model = %config.ollama_model, ?difficulty, "selected model (Ollama explicit)");
        return Ok(ModelSelection {
            provider: ProviderKind::Ollama,
            model: config.ollama_model.clone(),
        });
    }

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
    // Ollama explicitly requested — use same model for repair
    if config.primary_provider == "ollama" {
        info!(provider = "ollama", model = %config.ollama_model, "using Ollama for repair");
        return Ok(ModelSelection {
            provider: ProviderKind::Ollama,
            model: config.ollama_model.clone(),
        });
    }

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
        assert!(matches!(client.unwrap(), LlmClient::Ollama(..)));
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

    #[test]
    fn test_parse_task_routing_full() {
        let (task, config) =
            parse_task_routing_spec("analysis:ollama/gemma4:26b/0.2/131072").unwrap();
        assert_eq!(task, PipelineTask::Analysis);
        assert_eq!(config.provider.as_deref(), Some("ollama"));
        assert_eq!(config.model.as_deref(), Some("gemma4:26b"));
        assert_eq!(config.temperature, Some(0.2));
        assert_eq!(config.num_ctx, Some(131072));
    }

    #[test]
    fn test_parse_task_routing_minimal() {
        let (task, config) = parse_task_routing_spec("repair:deepseek").unwrap();
        assert_eq!(task, PipelineTask::Repair);
        assert_eq!(config.provider.as_deref(), Some("deepseek"));
        assert_eq!(config.model, None);
        assert_eq!(config.temperature, None);
    }

    #[test]
    fn test_parse_task_routing_with_model() {
        let (task, config) =
            parse_task_routing_spec("translation:anthropic/claude-opus-4-6/0.3").unwrap();
        assert_eq!(task, PipelineTask::Translation);
        assert_eq!(config.provider.as_deref(), Some("anthropic"));
        assert_eq!(config.model.as_deref(), Some("claude-opus-4-6"));
        assert_eq!(config.temperature, Some(0.3));
        assert_eq!(config.num_ctx, None);
    }

    #[test]
    fn test_parse_task_routing_invalid() {
        assert!(parse_task_routing_spec("bad").is_err());
        assert!(parse_task_routing_spec("unknown_task:ollama").is_err());
    }

    #[test]
    fn test_task_routing_set_get() {
        let mut routing = TaskRouting::default();
        assert!(!routing.has_overrides());

        routing.set(
            PipelineTask::Analysis,
            TaskModelConfig {
                provider: Some("ollama".to_string()),
                model: Some("gemma4:26b".to_string()),
                temperature: Some(0.2),
                num_ctx: Some(131072),
            },
        );
        assert!(routing.has_overrides());
        assert!(routing.get(PipelineTask::Analysis).is_some());
        assert!(routing.get(PipelineTask::Translation).is_none());
    }

    #[test]
    fn test_pipeline_task_from_str() {
        assert_eq!(
            "analysis".parse::<PipelineTask>().unwrap(),
            PipelineTask::Analysis
        );
        assert_eq!(
            "translate".parse::<PipelineTask>().unwrap(),
            PipelineTask::Translation
        );
        assert_eq!(
            "type-contract".parse::<PipelineTask>().unwrap(),
            PipelineTask::TypeContract
        );
        assert_eq!(
            "test-gen".parse::<PipelineTask>().unwrap(),
            PipelineTask::TestGen
        );
        assert!("garbage".parse::<PipelineTask>().is_err());
    }

    #[test]
    fn test_llm_client_pool_ollama_only() {
        let config = ProviderConfig {
            primary_provider: "ollama".to_string(),
            anthropic_api_key: None,
            deepseek_api_key: None,
            ..Default::default()
        };
        let pool = LlmClientPool::from_config(&config).unwrap();
        assert!(pool.get_by_provider("ollama").is_some());
        assert!(pool.get_by_provider("anthropic").is_none());
        assert!(pool.default_client().is_some());
    }

    #[test]
    fn test_resolve_task_with_override() {
        let config = ProviderConfig {
            primary_provider: "ollama".to_string(),
            anthropic_api_key: None,
            deepseek_api_key: None,
            ..Default::default()
        };
        let pool = LlmClientPool::from_config(&config).unwrap();

        let mut routing = TaskRouting::default();
        routing.set(
            PipelineTask::Analysis,
            TaskModelConfig {
                provider: Some("ollama".to_string()),
                model: Some("custom-model".to_string()),
                temperature: Some(0.1),
                num_ctx: Some(65536),
            },
        );

        let resolved =
            resolve_task(&pool, &routing, PipelineTask::Analysis, Difficulty::Easy, 0.3).unwrap();
        assert_eq!(resolved.model, "custom-model");
        assert_eq!(resolved.temperature, 0.1);
        assert_eq!(resolved.provider, ProviderKind::Ollama);

        // Non-overridden task uses default
        let resolved_default =
            resolve_task(&pool, &routing, PipelineTask::Translation, Difficulty::Easy, 0.3)
                .unwrap();
        assert_eq!(resolved_default.provider, ProviderKind::Ollama);
        assert_eq!(resolved_default.temperature, 0.3);
    }
}
