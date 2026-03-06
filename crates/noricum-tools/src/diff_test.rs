/// Differential testing: compile C and Rust code, run both, compare outputs.
///
/// This is the core verification mechanism for migration correctness.
/// Given a C program and its Rust translation (both with `main()`),
/// we compile both, run them, and compare stdout byte-by-byte.
/// Exit codes are also compared; stderr is captured but not compared by default.
use std::io::Read as _;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tracing::{debug, info};

use crate::ToolError;

/// Default timeout for running compiled executables (seconds).
const RUN_TIMEOUT_SECS: u64 = 10;

/// Output captured from running an executable.
#[derive(Debug)]
pub(crate) struct ExecOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Result of a differential test between C and Rust programs.
#[derive(Debug)]
pub struct DiffTestResult {
    /// Whether the test passed (both compiled, outputs match, exit codes match).
    pub passed: bool,
    /// Stdout captured from the C program.
    pub c_output: String,
    /// Stdout captured from the Rust program.
    pub rust_output: String,
    /// Whether the C source compiled successfully.
    pub c_compiled: bool,
    /// Whether the Rust source compiled successfully.
    pub rust_compiled: bool,
    /// Exit code from the C program (0 if not run).
    pub c_exit_code: i32,
    /// Exit code from the Rust program (0 if not run).
    pub rust_exit_code: i32,
    /// Stderr captured from the C program.
    pub c_stderr: String,
    /// Stderr captured from the Rust program.
    pub rust_stderr: String,
}

/// Options for differential testing.
#[derive(Debug, Default)]
pub struct DiffTestOptions {
    /// If set, use approximate float comparison with this epsilon.
    pub float_tolerance: Option<f64>,
}

/// Run a differential test: compile C and Rust, run both, compare outputs.
///
/// This is a convenience wrapper around [`run_diff_test_with_options`] using default options.
pub fn run_diff_test(c_source: &str, rust_source: &str) -> Result<DiffTestResult, ToolError> {
    run_diff_test_with_options(c_source, rust_source, &DiffTestOptions::default())
}

/// Run a differential test with configurable options (e.g. float tolerance).
///
/// Takes the C source (with `main()`) and the Rust source (with `main()`).
/// Compiles both, runs both, compares stdout and exit codes.
///
/// # Errors
///
/// Returns `ToolError` if temporary file operations or command execution fails
/// at the OS level (not for compilation failure, which is reported in the result).
pub fn run_diff_test_with_options(
    c_source: &str,
    rust_source: &str,
    options: &DiffTestOptions,
) -> Result<DiffTestResult, ToolError> {
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
            c_exit_code: 0,
            rust_exit_code: 0,
            c_stderr: String::new(),
            rust_stderr: String::new(),
        });
    }

    // Run both and capture stdout, stderr, exit code
    let c_result = run_exe(&c_exe, None, &[])?;
    let rust_result = run_exe(&rs_exe, None, &[])?;

    let stdout_match = if let Some(eps) = options.float_tolerance {
        approximate_match(&c_result.stdout, &rust_result.stdout, eps)
    } else {
        c_result.stdout == rust_result.stdout
    };

    let exit_code_match = c_result.exit_code == rust_result.exit_code;
    let passed = stdout_match && exit_code_match;

    info!(
        passed,
        c_len = c_result.stdout.len(),
        rust_len = rust_result.stdout.len(),
        c_exit = c_result.exit_code,
        rust_exit = rust_result.exit_code,
        "differential test complete"
    );

    if !passed {
        debug!(
            c_output = %c_result.stdout,
            rust_output = %rust_result.stdout,
            c_exit = c_result.exit_code,
            rust_exit = rust_result.exit_code,
            "output mismatch"
        );
    }

    Ok(DiffTestResult {
        passed,
        c_output: c_result.stdout,
        rust_output: rust_result.stdout,
        c_compiled,
        rust_compiled,
        c_exit_code: c_result.exit_code,
        rust_exit_code: rust_result.exit_code,
        c_stderr: c_result.stderr,
        rust_stderr: rust_result.stderr,
    })
}

/// Compile a C source file to an executable.
/// Returns `true` if compilation succeeded.
pub(crate) fn compile_c_exe(c_file: &Path, output_path: &Path) -> Result<bool, ToolError> {
    let output = Command::new("cc")
        .args(["-std=gnu11", "-o"])
        .arg(output_path)
        .arg(c_file)
        .arg("-lm")
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
pub(crate) fn compile_rust_exe(rs_file: &Path, output_path: &Path) -> Result<bool, ToolError> {
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

/// Maximum virtual memory for diff_test executables (256 MB).
const MAX_VIRTUAL_MEMORY: u64 = 256 * 1024 * 1024;
/// Maximum file size that diff_test executables can create (10 MB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;
/// Maximum number of child processes diff_test executables can spawn.
const MAX_NPROC: u64 = 0;

/// Run an executable and capture its stdout, stderr, and exit code, with a timeout.
///
/// # Security
///
/// The executable is run with:
/// - **Cleared environment**: no PATH, HOME, API keys, etc.
/// - **Timeout**: killed after `RUN_TIMEOUT_SECS` seconds.
/// - **Resource limits** (Unix only): memory cap (256 MB), file size cap (10 MB),
///   and no child process spawning via `setrlimit`.
///
/// **Trust boundary**: This is designed for *developer-supplied* C source code.
/// It does NOT provide full container/namespace isolation. Do NOT expose this to
/// untrusted third-party input without an additional sandbox layer (e.g., Docker,
/// bubblewrap, or seccomp).
///
/// Optionally accepts stdin input and command-line arguments.
pub(crate) fn run_exe(
    exe_path: &Path,
    stdin_input: Option<&str>,
    args: &[String],
) -> Result<ExecOutput, ToolError> {
    let stdin_mode = if stdin_input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    };

    let mut cmd = Command::new(exe_path);
    cmd.args(args)
        .env_clear()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(stdin_mode);

    // On Unix, set resource limits via pre_exec to constrain the child process.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: pre_exec runs between fork and exec. The closure only calls
        // async-signal-safe libc::setrlimit, which is permitted in this context.
        unsafe {
            cmd.pre_exec(|| {
                let mem_limit = libc::rlimit {
                    rlim_cur: MAX_VIRTUAL_MEMORY,
                    rlim_max: MAX_VIRTUAL_MEMORY,
                };
                libc::setrlimit(libc::RLIMIT_AS, &mem_limit);

                let file_limit = libc::rlimit {
                    rlim_cur: MAX_FILE_SIZE,
                    rlim_max: MAX_FILE_SIZE,
                };
                libc::setrlimit(libc::RLIMIT_FSIZE, &file_limit);

                let nproc_limit = libc::rlimit {
                    rlim_cur: MAX_NPROC,
                    rlim_max: MAX_NPROC,
                };
                libc::setrlimit(libc::RLIMIT_NPROC, &nproc_limit);

                Ok(())
            });
        }
    }

    let mut child = cmd.spawn().map_err(ToolError::Io)?;

    // Write stdin before reading stdout/stderr to avoid deadlock
    if let Some(input) = stdin_input
        && let Some(mut stdin) = child.stdin.take()
    {
        let _ = stdin.write_all(input.as_bytes());
        // Drop stdin to signal EOF
    }

    // Read stdout in a separate thread to avoid pipe buffer deadlock
    let stdout_handle = child.stdout.take();
    let stdout_thread = std::thread::spawn(move || {
        let mut output = String::new();
        if let Some(mut out) = stdout_handle {
            let _ = out.read_to_string(&mut output);
        }
        output
    });

    // Read stderr in a separate thread
    let stderr_handle = child.stderr.take();
    let stderr_thread = std::thread::spawn(move || {
        let mut output = String::new();
        if let Some(mut err) = stderr_handle {
            let _ = err.read_to_string(&mut output);
        }
        output
    });

    // Poll child with timeout
    let timeout = Duration::from_secs(RUN_TIMEOUT_SECS);
    let start = Instant::now();
    loop {
        match child.try_wait().map_err(ToolError::Io)? {
            Some(status) => {
                let stdout = stdout_thread.join().unwrap_or_default();
                let stderr = stderr_thread.join().unwrap_or_default();
                return Ok(ExecOutput {
                    stdout,
                    stderr,
                    exit_code: status.code().unwrap_or(-1),
                });
            }
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    debug!(timeout_secs = RUN_TIMEOUT_SECS, "executable timed out");
                    return Err(ToolError::Timeout(RUN_TIMEOUT_SECS));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// Approximate line-by-line comparison with float tolerance.
///
/// For each pair of lines: if both parse as f64, compare with epsilon;
/// otherwise require exact string match.
fn approximate_match(a: &str, b: &str, eps: f64) -> bool {
    let a_lines: Vec<&str> = a.lines().collect();
    let b_lines: Vec<&str> = b.lines().collect();

    if a_lines.len() != b_lines.len() {
        return false;
    }

    for (al, bl) in a_lines.iter().zip(b_lines.iter()) {
        if let (Ok(af), Ok(bf)) = (al.trim().parse::<f64>(), bl.trim().parse::<f64>()) {
            if (af - bf).abs() > eps {
                return false;
            }
        } else if al != bl {
            return false;
        }
    }

    true
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
    fn test_timeout_kills_hanging_process() {
        // A C program that would hang forever without timeout
        let c_source = r#"
#include <stdio.h>
int main(void) {
    printf("start\n");
    fflush(stdout);
    while(1) {}
    return 0;
}
"#;
        let rust_source = r#"fn main() { println!("start"); }"#;

        let result = run_diff_test(c_source, rust_source);
        // The test should complete (not hang) due to timeout.
        // C might timeout or produce partial output. Either way, it should not block forever.
        match result {
            Ok(r) => {
                // If C compiled, it either timed out (no output) or was killed
                assert!(!r.passed || !r.c_compiled);
            }
            Err(e) => {
                // Timeout error is expected
                assert!(
                    e.to_string().contains("timed out"),
                    "expected timeout error, got: {e}"
                );
            }
        }
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

    // --- Phase 1A: Exit code + stderr tests ---

    #[test]
    fn test_exit_code_mismatch() {
        let c_source = r#"
#include <stdio.h>
int main(void) {
    printf("hello\n");
    return 0;
}
"#;
        let rust_source = r#"
fn main() {
    print!("hello\n");
    std::process::exit(1);
}
"#;
        let result = run_diff_test(c_source, rust_source).unwrap();
        assert!(result.c_compiled);
        assert!(result.rust_compiled);
        assert!(!result.passed, "exit code mismatch should cause failure");
        assert_eq!(result.c_exit_code, 0);
        assert_eq!(result.rust_exit_code, 1);
    }

    #[test]
    fn test_exit_code_both_nonzero_match() {
        let c_source = r#"
#include <stdio.h>
int main(void) {
    printf("result\n");
    return 42;
}
"#;
        let rust_source = r#"
fn main() {
    print!("result\n");
    std::process::exit(42);
}
"#;
        let result = run_diff_test(c_source, rust_source).unwrap();
        assert!(result.c_compiled);
        assert!(result.rust_compiled);
        assert!(result.passed, "matching nonzero exit codes should pass");
        assert_eq!(result.c_exit_code, 42);
        assert_eq!(result.rust_exit_code, 42);
    }

    #[test]
    fn test_stderr_captured() {
        let c_source = r#"
#include <stdio.h>
int main(void) {
    fprintf(stderr, "c_error\n");
    printf("output\n");
    return 0;
}
"#;
        let rust_source = r#"
fn main() {
    eprintln!("rust_error");
    println!("output");
}
"#;
        let result = run_diff_test(c_source, rust_source).unwrap();
        assert!(
            result.passed,
            "stderr differs but should not affect pass/fail"
        );
        assert!(
            result.c_stderr.contains("c_error"),
            "C stderr should be captured"
        );
        assert!(
            result.rust_stderr.contains("rust_error"),
            "Rust stderr should be captured"
        );
    }

    #[test]
    fn test_exit_code_with_matching_stdout() {
        let c_source = r#"
#include <stdio.h>
int main(void) {
    printf("same\n");
    return 0;
}
"#;
        let rust_source = r#"
fn main() {
    print!("same\n");
    std::process::exit(1);
}
"#;
        let result = run_diff_test(c_source, rust_source).unwrap();
        assert!(result.c_compiled);
        assert!(result.rust_compiled);
        assert!(
            !result.passed,
            "stdout matches but exit code differs, should fail"
        );
        assert_eq!(result.c_output, result.rust_output);
        assert_ne!(result.c_exit_code, result.rust_exit_code);
    }

    // --- Phase 1B: Float tolerance tests ---

    #[test]
    fn test_float_tolerance_pass() {
        let c_source = r#"
#include <stdio.h>
int main(void) {
    printf("3.141593\n");
    return 0;
}
"#;
        let rust_source = r#"
fn main() {
    println!("3.141592");
}
"#;
        let options = DiffTestOptions {
            float_tolerance: Some(0.001),
        };
        let result = run_diff_test_with_options(c_source, rust_source, &options).unwrap();
        assert!(result.passed, "should pass with float tolerance 0.001");
    }

    #[test]
    fn test_float_tolerance_fail() {
        let c_source = r#"
#include <stdio.h>
int main(void) {
    printf("3.14\n");
    return 0;
}
"#;
        let rust_source = r#"
fn main() {
    println!("3.20");
}
"#;
        let options = DiffTestOptions {
            float_tolerance: Some(0.01),
        };
        let result = run_diff_test_with_options(c_source, rust_source, &options).unwrap();
        assert!(!result.passed, "should fail: |3.14 - 3.20| = 0.06 > 0.01");
    }

    #[test]
    fn test_float_tolerance_mixed() {
        let c_source = r#"
#include <stdio.h>
int main(void) {
    printf("hello\n");
    printf("3.14159\n");
    printf("42\n");
    return 0;
}
"#;
        let rust_source = r#"
fn main() {
    println!("hello");
    println!("3.14160");
    println!("42");
}
"#;
        let options = DiffTestOptions {
            float_tolerance: Some(0.001),
        };
        let result = run_diff_test_with_options(c_source, rust_source, &options).unwrap();
        assert!(
            result.passed,
            "mixed integer/float/string lines should pass with tolerance"
        );
    }

    // --- approximate_match unit tests ---

    #[test]
    fn test_approximate_match_exact() {
        assert!(approximate_match("hello\n42\n", "hello\n42\n", 0.001));
    }

    #[test]
    fn test_approximate_match_float_within_eps() {
        assert!(approximate_match("3.14159\n", "3.14160\n", 0.001));
    }

    #[test]
    fn test_approximate_match_float_outside_eps() {
        assert!(!approximate_match("3.14\n", "3.20\n", 0.01));
    }

    #[test]
    fn test_approximate_match_different_line_count() {
        assert!(!approximate_match("a\nb\n", "a\n", 0.001));
    }
}
