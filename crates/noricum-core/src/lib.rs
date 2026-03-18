//! `noricum-core` -- orchestrator and state machine for C-to-Rust migration.
//!
//! This crate provides the top-level migration pipeline that drives a C source
//! file through extraction, analysis, translation, validation, and repair.
//! It coordinates the agents (`noricum-agents`), tools (`noricum-tools`), and
//! validation (`noricum-validation`) crates.

pub mod artifacts;
pub mod audit;
pub mod dependency;
pub mod ensemble;
pub mod incremental;
pub mod orchestrator;
pub mod router;
pub mod surgical_repair;
pub mod type_contract;
pub mod budget;
pub(crate) mod warmstart;
pub(crate) mod assembly;
pub(crate) mod module_migration;

pub use orchestrator::MigrationConfig;

use thiserror::Error;

/// Error severity classification for alerting and monitoring.
///
/// Used by [`CoreError::severity`] to route errors to the appropriate
/// notification channel (e.g., log-only for `Low`, page-on-call for `High`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// Transient or expected failure (e.g., compilation failure during repair loop).
    Low,
    /// Actionable failure that may require investigation (e.g., missing source files, agent failures).
    Medium,
    /// Critical failure that should trigger an alert (e.g., budget exceeded, I/O errors).
    High,
}

/// Typed error for the `noricum-core` crate.
///
/// Wraps errors from downstream crates (`noricum-agents`, `noricum-tools`,
/// `noricum-validation`) and adds orchestration-specific variants such as
/// budget exceeded and missing source files.
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
