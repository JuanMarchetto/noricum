pub mod c2rust;
pub mod compiler;
pub mod diff_test;
pub mod rig_tools;
pub mod rule_translate;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("C2Rust transpilation failed: {0}")]
    C2RustFailed(String),

    #[error("compilation failed: {0}")]
    CompilationFailed(String),

    #[error("command not found: {0}")]
    CommandNotFound(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
