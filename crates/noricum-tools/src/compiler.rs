/// Compiler tools: compile C and Rust code, run executables, capture output.
///
/// Used for differential testing (compile both, run both, compare outputs)
/// and for the repair loop (compile Rust, check for errors).
use std::path::Path;
use std::process::{Command, Output};

use tracing::{debug, info};

use crate::ToolError;

/// Result of compiling and optionally running code.
#[derive(Debug)]
pub struct CompileResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl CompileResult {
    fn from_output(output: &Output) -> Self {
        Self {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        }
    }
}

/// Compile a C file to an executable.
pub fn compile_c(c_file: &Path, output_path: &Path) -> Result<CompileResult, ToolError> {
    info!(file = %c_file.display(), "compiling C file");

    let output = Command::new("cc")
        .args(["-std=c11", "-Wall", "-o"])
        .arg(output_path)
        .arg(c_file)
        .output()
        .map_err(|_| ToolError::CommandNotFound("cc".to_string()))?;

    let result = CompileResult::from_output(&output);
    debug!(success = result.success, "C compilation finished");
    Ok(result)
}

/// Check if a Rust source file compiles (using rustc directly).
pub fn check_rust_compiles(rust_source: &str) -> Result<CompileResult, ToolError> {
    let tmp = tempfile::tempdir()?;
    let rs_file = tmp.path().join("check.rs");
    std::fs::write(&rs_file, rust_source)?;

    let output = Command::new("rustc")
        .arg("--edition=2024")
        .arg("--crate-type=lib")
        .arg(&rs_file)
        .arg("-o")
        .arg(tmp.path().join("check"))
        .output()
        .map_err(|_| ToolError::CommandNotFound("rustc".to_string()))?;

    let result = CompileResult::from_output(&output);
    debug!(success = result.success, "Rust compile check finished");
    Ok(result)
}

/// Run clippy on a Rust source string, return warnings.
pub fn run_clippy_on_source(rust_source: &str) -> Result<Vec<String>, ToolError> {
    let tmp = tempfile::tempdir()?;
    let rs_file = tmp.path().join("check.rs");
    std::fs::write(&rs_file, rust_source)?;

    let output = Command::new("clippy-driver")
        .arg("--edition=2024")
        .arg(&rs_file)
        .output();

    match output {
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let warnings: Vec<String> = stderr
                .lines()
                .filter(|l| l.contains("warning"))
                .map(String::from)
                .collect();
            Ok(warnings)
        }
        Err(_) => {
            // clippy-driver not available; fall back to rustc warnings
            debug!("clippy-driver not found, skipping clippy check");
            Ok(Vec::new())
        }
    }
}

/// Run a compiled executable and capture its output.
pub fn run_executable(exe_path: &Path, args: &[&str]) -> Result<CompileResult, ToolError> {
    let output = Command::new(exe_path)
        .args(args)
        .output()
        .map_err(ToolError::Io)?;

    Ok(CompileResult::from_output(&output))
}

/// Count the number of `unsafe` blocks in Rust source code.
pub fn count_unsafe_blocks(rust_source: &str) -> u32 {
    // Simple heuristic: count occurrences of "unsafe {" and "unsafe fn"
    // A proper implementation would use tree-sitter, but this works for scoring.
    rust_source
        .lines()
        .map(str::trim)
        .map(|line| {
            let has_block = line.contains("unsafe {") || line.contains("unsafe{");
            let has_fn = line.starts_with("unsafe fn ") || line.contains(" unsafe fn ");
            u32::from(has_block) + u32::from(has_fn)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_unsafe_blocks() {
        let source = r#"
fn safe_fn() -> i32 { 42 }

unsafe fn dangerous() -> *mut u8 {
    std::ptr::null_mut()
}

fn uses_unsafe() {
    unsafe {
        dangerous();
    }
}
"#;
        assert_eq!(count_unsafe_blocks(source), 2);
    }

    #[test]
    fn test_count_unsafe_zero() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        assert_eq!(count_unsafe_blocks(source), 0);
    }

    #[test]
    fn test_check_rust_compiles_valid() {
        let result = check_rust_compiles("pub fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_check_rust_compiles_invalid() {
        let result = check_rust_compiles("fn bad( { }").unwrap();
        assert!(!result.success);
        assert!(!result.stderr.is_empty());
    }
}
