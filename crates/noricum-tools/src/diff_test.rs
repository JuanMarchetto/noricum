/// Differential testing: compile C and Rust code, run both, compare outputs.
///
/// This is the core verification mechanism for migration correctness.
/// Given a C program and its Rust translation (both with `main()`),
/// we compile both, run them, and compare stdout byte-by-byte.
use std::path::Path;
use std::process::Command;

use tracing::{debug, info};

use crate::ToolError;

/// Result of a differential test between C and Rust programs.
#[derive(Debug)]
pub struct DiffTestResult {
    /// Whether the test passed (both compiled and outputs match).
    pub passed: bool,
    /// Stdout captured from the C program.
    pub c_output: String,
    /// Stdout captured from the Rust program.
    pub rust_output: String,
    /// Whether the C source compiled successfully.
    pub c_compiled: bool,
    /// Whether the Rust source compiled successfully.
    pub rust_compiled: bool,
}

/// Run a differential test: compile C and Rust, run both, compare outputs.
///
/// Takes the C source (with `main()`) and the Rust source (with `main()`).
/// The Rust source should be a complete program that can be compiled standalone.
///
/// # Errors
///
/// Returns `ToolError` if temporary file operations or command execution fails
/// at the OS level (not for compilation failures, which are reported in the result).
pub fn run_diff_test(c_source: &str, rust_source: &str) -> Result<DiffTestResult, ToolError> {
    let tmp = tempfile::tempdir()?;
    let c_file = tmp.path().join("test.c");
    let c_exe = tmp.path().join("test_c");
    let rs_file = tmp.path().join("test.rs");
    let rs_exe = tmp.path().join("test_rs");

    std::fs::write(&c_file, c_source)?;
    std::fs::write(&rs_file, rust_source)?;

    // Compile C
    let c_compiled = compile_c_exe(&c_file, &c_exe)?;

    // Compile Rust
    let rust_compiled = compile_rust_exe(&rs_file, &rs_exe)?;

    // If either failed to compile, return early
    if !c_compiled || !rust_compiled {
        info!(
            c_compiled,
            rust_compiled, "differential test: compilation failure"
        );
        return Ok(DiffTestResult {
            passed: false,
            c_output: String::new(),
            rust_output: String::new(),
            c_compiled,
            rust_compiled,
        });
    }

    // Run both and capture stdout
    let c_output = run_exe(&c_exe)?;
    let rust_output = run_exe(&rs_exe)?;

    let passed = c_output == rust_output;

    info!(
        passed,
        c_len = c_output.len(),
        rust_len = rust_output.len(),
        "differential test complete"
    );

    if !passed {
        debug!(c_output = %c_output, rust_output = %rust_output, "output mismatch");
    }

    Ok(DiffTestResult {
        passed,
        c_output,
        rust_output,
        c_compiled,
        rust_compiled,
    })
}

/// Compile a C source file to an executable.
/// Returns `true` if compilation succeeded.
fn compile_c_exe(c_file: &Path, output_path: &Path) -> Result<bool, ToolError> {
    let output = Command::new("cc")
        .args(["-std=c11", "-o"])
        .arg(output_path)
        .arg(c_file)
        .output()
        .map_err(|_| ToolError::CommandNotFound("cc".to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!(stderr = %stderr, "C compilation failed");
    }

    Ok(output.status.success())
}

/// Compile a Rust source file to an executable.
/// Returns `true` if compilation succeeded.
fn compile_rust_exe(rs_file: &Path, output_path: &Path) -> Result<bool, ToolError> {
    let output = Command::new("rustc")
        .arg("--edition=2024")
        .arg("-o")
        .arg(output_path)
        .arg(rs_file)
        .output()
        .map_err(|_| ToolError::CommandNotFound("rustc".to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!(stderr = %stderr, "Rust compilation failed");
    }

    Ok(output.status.success())
}

/// Run an executable and capture its stdout.
fn run_exe(exe_path: &Path) -> Result<String, ToolError> {
    let output = Command::new(exe_path).output().map_err(ToolError::Io)?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matching_outputs() {
        let c_source = r#"
#include <stdio.h>
int main(void) {
    printf("%d\n", 2 + 3);
    return 0;
}
"#;
        let rust_source = r#"
fn main() {
    println!("{}", 2 + 3);
}
"#;
        let result = run_diff_test(c_source, rust_source).unwrap();
        assert!(result.c_compiled, "C should compile");
        assert!(result.rust_compiled, "Rust should compile");
        assert!(result.passed, "outputs should match");
        assert_eq!(result.c_output, "5\n");
        assert_eq!(result.rust_output, "5\n");
    }

    #[test]
    fn test_mismatching_outputs() {
        let c_source = r#"
#include <stdio.h>
int main(void) {
    printf("hello\n");
    return 0;
}
"#;
        let rust_source = r#"
fn main() {
    println!("world");
}
"#;
        let result = run_diff_test(c_source, rust_source).unwrap();
        assert!(result.c_compiled);
        assert!(result.rust_compiled);
        assert!(!result.passed, "outputs should NOT match");
        assert_eq!(result.c_output, "hello\n");
        assert_eq!(result.rust_output, "world\n");
    }

    #[test]
    fn test_c_compile_failure() {
        let c_source = "this is not valid C code !!!";
        let rust_source = r#"fn main() { println!("ok"); }"#;

        let result = run_diff_test(c_source, rust_source).unwrap();
        assert!(!result.c_compiled, "C should fail to compile");
        assert!(result.rust_compiled, "Rust should compile");
        assert!(!result.passed);
    }

    #[test]
    fn test_rust_compile_failure() {
        let c_source = r#"
#include <stdio.h>
int main(void) { printf("ok\n"); return 0; }
"#;
        let rust_source = "fn main( { invalid syntax }";

        let result = run_diff_test(c_source, rust_source).unwrap();
        assert!(result.c_compiled, "C should compile");
        assert!(!result.rust_compiled, "Rust should fail to compile");
        assert!(!result.passed);
    }

    #[test]
    fn test_both_compile_failure() {
        let c_source = "not valid C";
        let rust_source = "not valid Rust";

        let result = run_diff_test(c_source, rust_source).unwrap();
        assert!(!result.c_compiled);
        assert!(!result.rust_compiled);
        assert!(!result.passed);
    }

    #[test]
    fn test_add_function_diff() {
        let c_source = r#"
#include <stdio.h>
int add(int a, int b) { return a + b; }
int main(void) {
    printf("%d\n", add(2, 3));
    printf("%d\n", add(-1, 1));
    printf("%d\n", add(0, 0));
    return 0;
}
"#;
        let rust_source = r#"
fn add(a: i32, b: i32) -> i32 { a + b }
fn main() {
    println!("{}", add(2, 3));
    println!("{}", add(-1, 1));
    println!("{}", add(0, 0));
}
"#;
        let result = run_diff_test(c_source, rust_source).unwrap();
        assert!(result.c_compiled);
        assert!(result.rust_compiled);
        assert!(result.passed, "add function outputs should match");
        assert_eq!(result.c_output, "5\n0\n0\n");
    }
}
