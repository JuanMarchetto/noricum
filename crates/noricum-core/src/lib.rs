pub mod artifacts;
pub mod audit;
pub mod dependency;
pub mod incremental;
pub mod orchestrator;
pub mod router;
pub mod surgical_repair;
#[allow(dead_code)] // Helpers used by tests now; Task 3 wires up generate_type_contract()
pub mod type_contract;

pub use orchestrator::MigrationConfig;

use thiserror::Error;

/// Error severity classification for alerting and monitoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// Transient or expected failure (e.g., compilation failure during repair loop).
    Low,
    /// Actionable failure that may require investigation (e.g., missing source files, agent failures).
    Medium,
    /// Critical failure that should trigger an alert (e.g., budget exceeded, I/O errors).
    High,
}

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

    #[error("token budget exceeded: used {used} of {budget} allowed tokens")]
    BudgetExceeded { used: u64, budget: u64 },
}

impl CoreError {
    /// Classify error severity for alerting and monitoring systems.
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            CoreError::Validation(_) => ErrorSeverity::Low,
            CoreError::Tool(_) => ErrorSeverity::Low,
            CoreError::Agent(_) => ErrorSeverity::Medium,
            CoreError::NoSourceFiles(_) => ErrorSeverity::Medium,
            CoreError::Orchestration(_) => ErrorSeverity::Medium,
            CoreError::Io(_) => ErrorSeverity::High,
            CoreError::BudgetExceeded { .. } => ErrorSeverity::High,
        }
    }

    /// Return a machine-readable error code for monitoring dashboards.
    pub fn error_code(&self) -> &'static str {
        match self {
            CoreError::Orchestration(_) => "E_ORCHESTRATION",
            CoreError::NoSourceFiles(_) => "E_NO_SOURCE",
            CoreError::Agent(_) => "E_AGENT",
            CoreError::Tool(_) => "E_TOOL",
            CoreError::Validation(_) => "E_VALIDATION",
            CoreError::Io(_) => "E_IO",
            CoreError::BudgetExceeded { .. } => "E_BUDGET",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_severity_classification() {
        let io_err = CoreError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "test"));
        assert_eq!(io_err.severity(), ErrorSeverity::High);
        assert_eq!(io_err.error_code(), "E_IO");

        let budget_err = CoreError::BudgetExceeded {
            used: 100,
            budget: 50,
        };
        assert_eq!(budget_err.severity(), ErrorSeverity::High);
        assert_eq!(budget_err.error_code(), "E_BUDGET");

        let no_source = CoreError::NoSourceFiles("/tmp".to_string());
        assert_eq!(no_source.severity(), ErrorSeverity::Medium);
        assert_eq!(no_source.error_code(), "E_NO_SOURCE");
    }
}
