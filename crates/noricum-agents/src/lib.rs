pub mod analysis;
pub mod providers;
pub mod repair;
pub mod test_gen;
pub mod translation;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("LLM provider error: {0}")]
    Provider(String),

    #[error("no provider configured for role: {0}")]
    NoProvider(String),

    #[error("failed to parse LLM response: {0}")]
    Parse(String),

    #[error("max retries exceeded")]
    MaxRetries,
}
