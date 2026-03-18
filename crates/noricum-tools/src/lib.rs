pub mod ast;
pub mod c2rust;
pub mod compiler;
pub mod crate_builder;
pub mod diff_test;
pub mod doc_gen;
pub mod ffi_bridge;
pub mod fuzz_test;
pub mod harness_gen;
pub mod mixed_build;
pub mod multi_input_test;
pub mod preprocessor;
pub mod repair_pattern_store;
pub mod repair_rules;
pub mod rig_tools;
pub mod rule_translate;
pub mod semantic_patterns;
pub mod spec_mining;
pub mod translation_memory;

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

    #[error("execution timed out after {0}s")]
    Timeout(u64),
}
