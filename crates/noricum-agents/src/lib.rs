pub mod providers;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("LLM provider error: {0}")]
    Provider(String),

    #[error("no provider configured for role: {0}")]
    NoProvider(String),

    #[error("max retries exceeded")]
    MaxRetries,
}
