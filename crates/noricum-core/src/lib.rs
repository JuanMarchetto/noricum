pub mod orchestrator;
pub mod router;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("orchestration error: {0}")]
    Orchestration(String),

    #[error("no source files found in {0}")]
    NoSourceFiles(String),

    #[error("agent error: {0}")]
    Agent(#[from] noricum_agents::AgentError),

    #[error("tool error: {0}")]
    Tool(#[from] noricum_tools::ToolError),

    #[error("validation error: {0}")]
    Validation(#[from] noricum_validation::ValidationError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
