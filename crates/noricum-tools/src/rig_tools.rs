//! rig-rs Tool implementations for Noricum LLM agents.
//!
//! Each tool wraps an underlying function from `crate::compiler` or std I/O,
//! exposing it as a [`rig::tool::Tool`] so agents can invoke it via tool-calling.

use std::path::Path;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::compiler;

// ---------------------------------------------------------------------------
// Shared error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum RigToolError {
    #[error("compilation error: {0}")]
    Compilation(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("tool error: {0}")]
    Tool(#[from] crate::ToolError),

    #[error("path security violation: {0}")]
    PathViolation(String),
}

/// Validate that a file path is safe for LLM agent access.
///
/// Rejects paths containing `..`, and absolute paths outside of `/tmp` or the
/// current working directory. Canonicalizes paths to resolve symlinks before
/// checking, preventing symlink-based bypass.
fn validate_file_path(path: &str) -> Result<(), RigToolError> {
    let p = Path::new(path);

    // Reject path traversal components
    for component in p.components() {
        if let std::path::Component::ParentDir = component {
            return Err(RigToolError::PathViolation(
                "path contains '..' traversal".to_string(),
            ));
        }
    }

    // If absolute, resolve the path to check against allowed directories.
    // If the file exists, canonicalize it to resolve symlinks.
    // If not, canonicalize the parent directory and append the file name.
    if p.is_absolute() {
        let canonical = if p.exists() {
            p.canonicalize().map_err(|e| {
                RigToolError::PathViolation(format!("cannot resolve path {path}: {e}"))
            })?
        } else if let Some(parent) = p.parent() {
            let canon_parent = parent.canonicalize().map_err(|e| {
                RigToolError::PathViolation(format!("cannot resolve parent of {path}: {e}"))
            })?;
            if let Some(file_name) = p.file_name() {
                canon_parent.join(file_name)
            } else {
                canon_parent
            }
        } else {
            return Err(RigToolError::PathViolation(format!(
                "cannot resolve path {path}: no parent directory"
            )));
        };
        let tmp = Path::new("/tmp")
            .canonicalize()
            .unwrap_or_else(|_| Path::new("/tmp").to_path_buf());
        let var_tmp = Path::new("/var/tmp")
            .canonicalize()
            .unwrap_or_else(|_| Path::new("/var/tmp").to_path_buf());
        let is_tmp = canonical.starts_with(&tmp) || canonical.starts_with(&var_tmp);
        let is_cwd = std::env::current_dir()
            .ok()
            .is_some_and(|cwd| canonical.starts_with(&cwd));
        if !is_tmp && !is_cwd {
            return Err(RigToolError::PathViolation(format!(
                "absolute path outside allowed directories: {path}"
            )));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 1. CompileCheckTool
// ---------------------------------------------------------------------------

/// Arguments for [`CompileCheckTool`].
#[derive(Debug, Deserialize, Serialize)]
pub struct CompileCheckArgs {
    /// The Rust source code to compile-check.
    pub source_code: String,
}

/// Result returned by [`CompileCheckTool`].
#[derive(Debug, Deserialize, Serialize)]
pub struct CompileCheckOutput {
    pub success: bool,
    pub errors: String,
}

/// Compile-check Rust source code using `rustc`.
///
/// Returns whether compilation succeeded and any error messages.
#[derive(Debug, Deserialize, Serialize)]
pub struct CompileCheckTool;

impl Tool for CompileCheckTool {
    const NAME: &'static str = "compile_check";

    type Error = RigToolError;
    type Args = CompileCheckArgs;
    type Output = CompileCheckOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "compile_check".to_string(),
            description: "Compile-check Rust source code with rustc. Returns success/failure and any compiler errors.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source_code": {
                        "type": "string",
                        "description": "The Rust source code to compile-check"
                    }
                },
                "required": ["source_code"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let result = compiler::check_rust_compiles(&args.source_code)?;
        Ok(CompileCheckOutput {
            success: result.success,
            errors: result.stderr,
        })
    }
}

// ---------------------------------------------------------------------------
// 2. ReadSourceTool
// ---------------------------------------------------------------------------

/// Arguments for [`ReadSourceTool`].
#[derive(Debug, Deserialize, Serialize)]
pub struct ReadSourceArgs {
    /// Absolute or relative path to the file to read.
    pub file_path: String,
}

/// Result returned by [`ReadSourceTool`].
#[derive(Debug, Deserialize, Serialize)]
pub struct ReadSourceOutput {
    pub content: String,
}

/// Read the contents of a source file from disk.
///
/// Used by analysis agents to inspect C/C++ or Rust source files.
#[derive(Debug, Deserialize, Serialize)]
pub struct ReadSourceTool;

impl Tool for ReadSourceTool {
    const NAME: &'static str = "read_source";

    type Error = RigToolError;
    type Args = ReadSourceArgs;
    type Output = ReadSourceOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "read_source".to_string(),
            description: "Read the contents of a source file from disk.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the source file to read"
                    }
                },
                "required": ["file_path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        validate_file_path(&args.file_path)?;
        let content = std::fs::read_to_string(&args.file_path)?;
        Ok(ReadSourceOutput { content })
    }
}

// ---------------------------------------------------------------------------
// 3. WriteSourceTool
// ---------------------------------------------------------------------------

/// Arguments for [`WriteSourceTool`].
#[derive(Debug, Deserialize, Serialize)]
pub struct WriteSourceArgs {
    /// Path where the file should be written.
    pub file_path: String,
    /// Content to write to the file.
    pub content: String,
}

/// Result returned by [`WriteSourceTool`].
#[derive(Debug, Deserialize, Serialize)]
pub struct WriteSourceOutput {
    pub success: bool,
    pub bytes_written: usize,
}

/// Write content to a source file on disk.
///
/// Used by translation agents to persist generated Rust code.
#[derive(Debug, Deserialize, Serialize)]
pub struct WriteSourceTool;

impl Tool for WriteSourceTool {
    const NAME: &'static str = "write_source";

    type Error = RigToolError;
    type Args = WriteSourceArgs;
    type Output = WriteSourceOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "write_source".to_string(),
            description: "Write content to a source file on disk.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path where the file should be written"
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write to the file"
                    }
                },
                "required": ["file_path", "content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        validate_file_path(&args.file_path)?;
        let bytes = args.content.len();
        std::fs::write(&args.file_path, &args.content)?;
        Ok(WriteSourceOutput {
            success: true,
            bytes_written: bytes,
        })
    }
}

// ---------------------------------------------------------------------------
// 4. ClippyCheckTool
// ---------------------------------------------------------------------------

/// Arguments for [`ClippyCheckTool`].
#[derive(Debug, Deserialize, Serialize)]
pub struct ClippyCheckArgs {
    /// The Rust source code to lint with Clippy.
    pub source_code: String,
}

/// Result returned by [`ClippyCheckTool`].
#[derive(Debug, Deserialize, Serialize)]
pub struct ClippyCheckOutput {
    pub warnings: Vec<String>,
    pub warning_count: usize,
}

/// Run Clippy on Rust source code and return warnings.
///
/// Falls back gracefully if `clippy-driver` is not installed.
#[derive(Debug, Deserialize, Serialize)]
pub struct ClippyCheckTool;

impl Tool for ClippyCheckTool {
    const NAME: &'static str = "clippy_check";

    type Error = RigToolError;
    type Args = ClippyCheckArgs;
    type Output = ClippyCheckOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "clippy_check".to_string(),
            description: "Run Clippy on Rust source code and return a list of warnings."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source_code": {
                        "type": "string",
                        "description": "The Rust source code to lint with Clippy"
                    }
                },
                "required": ["source_code"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let warnings = compiler::run_clippy_on_source(&args.source_code)?;
        let count = warnings.len();
        Ok(ClippyCheckOutput {
            warnings,
            warning_count: count,
        })
    }
}

// ---------------------------------------------------------------------------
// 5. UnsafeCountTool
// ---------------------------------------------------------------------------

/// Arguments for [`UnsafeCountTool`].
#[derive(Debug, Deserialize, Serialize)]
pub struct UnsafeCountArgs {
    /// The Rust source code to analyze.
    pub source_code: String,
}

/// Result returned by [`UnsafeCountTool`].
#[derive(Debug, Deserialize, Serialize)]
pub struct UnsafeCountOutput {
    pub unsafe_count: u32,
}

/// Count the number of `unsafe` blocks and `unsafe fn` declarations in Rust source code.
///
/// This is a key metric for migration quality -- the goal is to minimize unsafe usage.
#[derive(Debug, Deserialize, Serialize)]
pub struct UnsafeCountTool;

impl Tool for UnsafeCountTool {
    const NAME: &'static str = "unsafe_count";

    type Error = RigToolError;
    type Args = UnsafeCountArgs;
    type Output = UnsafeCountOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "unsafe_count".to_string(),
            description:
                "Count the number of unsafe blocks and unsafe fn declarations in Rust source code."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source_code": {
                        "type": "string",
                        "description": "The Rust source code to analyze for unsafe usage"
                    }
                },
                "required": ["source_code"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let count = compiler::count_unsafe_blocks(&args.source_code);
        Ok(UnsafeCountOutput {
            unsafe_count: count,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_compile_check_tool_definition() {
        let tool = CompileCheckTool;
        let def = tool.definition(String::new()).await;
        assert_eq!(def.name, "compile_check");
        assert!(!def.description.is_empty());
    }

    #[tokio::test]
    async fn test_compile_check_valid_code() {
        let tool = CompileCheckTool;
        let result = tool
            .call(CompileCheckArgs {
                source_code: "pub fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
            })
            .await
            .unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_compile_check_invalid_code() {
        let tool = CompileCheckTool;
        let result = tool
            .call(CompileCheckArgs {
                source_code: "fn bad( { }".to_string(),
            })
            .await
            .unwrap();
        assert!(!result.success);
        assert!(!result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_read_source_tool_nonexistent() {
        let tool = ReadSourceTool;
        let result = tool
            .call(ReadSourceArgs {
                file_path: "/tmp/noricum_nonexistent_file_12345.c".to_string(),
            })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_write_and_read_source_roundtrip() {
        let path = "/tmp/noricum_test_roundtrip.rs";
        let content = "pub fn hello() -> &'static str { \"hello\" }";

        let write_tool = WriteSourceTool;
        let write_result = write_tool
            .call(WriteSourceArgs {
                file_path: path.to_string(),
                content: content.to_string(),
            })
            .await
            .unwrap();
        assert!(write_result.success);
        assert_eq!(write_result.bytes_written, content.len());

        let read_tool = ReadSourceTool;
        let read_result = read_tool
            .call(ReadSourceArgs {
                file_path: path.to_string(),
            })
            .await
            .unwrap();
        assert_eq!(read_result.content, content);

        // Cleanup
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn test_clippy_check_tool_definition() {
        let tool = ClippyCheckTool;
        let def = tool.definition(String::new()).await;
        assert_eq!(def.name, "clippy_check");
    }

    #[tokio::test]
    async fn test_unsafe_count_tool() {
        let tool = UnsafeCountTool;
        let result = tool
            .call(UnsafeCountArgs {
                source_code: r#"
unsafe fn danger() {}
fn safe_fn() {
    unsafe {
        danger();
    }
}
"#
                .to_string(),
            })
            .await
            .unwrap();
        assert_eq!(result.unsafe_count, 2);
    }

    #[tokio::test]
    async fn test_unsafe_count_zero() {
        let tool = UnsafeCountTool;
        let result = tool
            .call(UnsafeCountArgs {
                source_code: "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(result.unsafe_count, 0);
    }

    #[test]
    fn test_validate_path_rejects_traversal() {
        assert!(validate_file_path("../../../etc/passwd").is_err());
        assert!(validate_file_path("foo/../bar/../../etc/shadow").is_err());
    }

    #[test]
    fn test_validate_path_rejects_absolute_outside_allowed() {
        assert!(validate_file_path("/etc/passwd").is_err());
        assert!(validate_file_path("/root/.ssh/id_rsa").is_err());
    }

    #[test]
    fn test_validate_path_allows_tmp() {
        assert!(validate_file_path("/tmp/noricum_test.rs").is_ok());
    }

    #[test]
    fn test_validate_path_allows_relative() {
        assert!(validate_file_path("output/test.rs").is_ok());
    }

    #[tokio::test]
    async fn test_read_source_rejects_path_traversal() {
        let tool = ReadSourceTool;
        let result = tool
            .call(ReadSourceArgs {
                file_path: "../../../etc/passwd".to_string(),
            })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_write_source_rejects_path_traversal() {
        let tool = WriteSourceTool;
        let result = tool
            .call(WriteSourceArgs {
                file_path: "../../../tmp/evil.rs".to_string(),
                content: "malicious".to_string(),
            })
            .await;
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Any path containing ".." must be rejected.
        #[test]
        fn paths_with_dotdot_always_rejected(
            prefix in "[a-z]{0,5}",
            suffix in "[a-z]{0,5}",
        ) {
            let path = format!("{prefix}/../{suffix}");
            prop_assert!(validate_file_path(&path).is_err(),
                "path with '..' should be rejected: {path}");
        }

        /// Simple relative paths (no ..) should be accepted.
        #[test]
        fn simple_relative_paths_accepted(
            segments in proptest::collection::vec("[a-z][a-z0-9_]{0,8}", 1..4),
        ) {
            let path = segments.join("/") + ".rs";
            prop_assert!(validate_file_path(&path).is_ok(),
                "simple relative path should be accepted: {path}");
        }

        /// Absolute paths outside /tmp and /var/tmp are rejected.
        #[test]
        fn absolute_paths_outside_tmp_rejected(
            dir in prop_oneof!["/etc", "/root", "/usr", "/home", "/opt", "/var/log"],
            file in "[a-z]{1,8}",
        ) {
            let path = format!("{dir}/{file}");
            prop_assert!(validate_file_path(&path).is_err(),
                "absolute path outside allowed dirs should be rejected: {path}");
        }

        /// Paths under /tmp are accepted.
        #[test]
        fn tmp_paths_accepted(
            file in "[a-z][a-z0-9_]{0,8}\\.rs",
        ) {
            let path = format!("/tmp/{file}");
            prop_assert!(validate_file_path(&path).is_ok(),
                "/tmp paths should be accepted: {path}");
        }
    }
}
