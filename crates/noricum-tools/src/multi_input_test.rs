/// Multi-input differential testing: compile once, run with multiple inputs.
///
/// Extends the basic diff test to test C and Rust programs with different
/// stdin inputs and command-line arguments, catching edge-case mismatches.
use std::path::Path;
use tracing::info;

use crate::ToolError;
use crate::diff_test::{DiffTestResult, compile_c_exe, compile_rust_exe, run_exe};

/// A single test input configuration.
pub struct TestInput {
    /// Descriptive name for this test case.
    pub name: String,
    /// Stdin to feed to the program.
    pub stdin: String,
    /// Command-line arguments.
    pub args: Vec<String>,
}

/// Result of running multiple inputs through a diff test.
pub struct MultiInputResult {
    /// Whether all individual tests passed.
    pub all_passed: bool,
    /// Per-input results: (input_name, diff_test_result).
    pub results: Vec<(String, DiffTestResult)>,
}

/// Run a differential test with multiple inputs.
///
/// Compiles C and Rust once, then runs each with the provided inputs.
/// If `inputs` is empty, runs once with no stdin (backward-compatible).
pub fn run_multi_input_diff_test(
    c_source: &str,
    rust_source: &str,
    inputs: &[TestInput],
) -> Result<MultiInputResult, ToolError> {
    let tmp = tempfile::tempdir()?;
    let c_file = tmp.path().join("test.c");
    let c_exe = tmp.path().join("test_c");
    let rs_file = tmp.path().join("test.rs");
    let rs_exe = tmp.path().join("test_rs");

    std::fs::write(&c_file, c_source)?;
    std::fs::write(&rs_file, rust_source)?;

    let c_compiled = compile_c_exe(&c_file, &c_exe)?;
    let rust_compiled = compile_rust_exe(&rs_file, &rs_exe)?;

    if !c_compiled || !rust_compiled {
        info!(
            c_compiled,
            rust_compiled, "multi-input: compilation failure"
        );
        let result = DiffTestResult {
            passed: false,
            c_output: String::new(),
            rust_output: String::new(),
            c_compiled,
            rust_compiled,
            c_exit_code: 0,
            rust_exit_code: 0,
            c_stderr: String::new(),
            rust_stderr: String::new(),
        };
        return Ok(MultiInputResult {
            all_passed: false,
            results: vec![("compilation".to_string(), result)],
        });
    }

    let mut results = Vec::new();
    let mut all_passed = true;

    let run_one = |name: &str,
                   stdin: Option<&str>,
                   args: &[String],
                   c_exe: &Path,
                   rs_exe: &Path|
     -> Result<(String, DiffTestResult), ToolError> {
        let c_out = run_exe(c_exe, stdin, args)?;
        let r_out = run_exe(rs_exe, stdin, args)?;
        let passed = c_out.stdout == r_out.stdout && c_out.exit_code == r_out.exit_code;

        Ok((
            name.to_string(),
            DiffTestResult {
                passed,
                c_output: c_out.stdout,
                rust_output: r_out.stdout,
                c_compiled: true,
                rust_compiled: true,
                c_exit_code: c_out.exit_code,
                rust_exit_code: r_out.exit_code,
                c_stderr: c_out.stderr,
                rust_stderr: r_out.stderr,
            },
        ))
    };

    if inputs.is_empty() {
        let (name, result) = run_one("default", None, &[], &c_exe, &rs_exe)?;
        if !result.passed {
            all_passed = false;
        }
        results.push((name, result));
    } else {
        for input in inputs {
            let stdin = if input.stdin.is_empty() {
                None
            } else {
                Some(input.stdin.as_str())
            };
            let (name, result) = run_one(&input.name, stdin, &input.args, &c_exe, &rs_exe)?;
            if !result.passed {
                all_passed = false;
            }
            results.push((name, result));
        }
    }

    info!(
        all_passed,
        num_inputs = results.len(),
        "multi-input diff test complete"
    );

    Ok(MultiInputResult {
        all_passed,
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_input_all_pass() {
        // Program that echoes its arguments
        let c_source = r#"
#include <stdio.h>
int main(int argc, char *argv[]) {
    for (int i = 1; i < argc; i++) printf("%s\n", argv[i]);
    return 0;
}
"#;
        let rust_source = r#"
fn main() {
    for arg in std::env::args().skip(1) {
        println!("{}", arg);
    }
}
"#;
        let inputs = vec![
            TestInput {
                name: "hello".into(),
                stdin: String::new(),
                args: vec!["hello".into()],
            },
            TestInput {
                name: "world".into(),
                stdin: String::new(),
                args: vec!["world".into()],
            },
            TestInput {
                name: "multi".into(),
                stdin: String::new(),
                args: vec!["foo".into(), "bar".into()],
            },
        ];

        let result = run_multi_input_diff_test(c_source, rust_source, &inputs).unwrap();
        assert!(result.all_passed, "all echo inputs should match");
        assert_eq!(result.results.len(), 3);
    }

    #[test]
    fn test_multi_input_one_fails() {
        // C echoes args as-is, Rust adds "!" for the arg "bad"
        let c_source = r#"
#include <stdio.h>
int main(int argc, char *argv[]) {
    for (int i = 1; i < argc; i++) printf("%s\n", argv[i]);
    return 0;
}
"#;
        let rust_source = r#"
fn main() {
    for arg in std::env::args().skip(1) {
        if arg == "bad" {
            println!("{}!", arg);
        } else {
            println!("{}", arg);
        }
    }
}
"#;
        let inputs = vec![
            TestInput {
                name: "good1".into(),
                stdin: String::new(),
                args: vec!["hello".into()],
            },
            TestInput {
                name: "good2".into(),
                stdin: String::new(),
                args: vec!["world".into()],
            },
            TestInput {
                name: "bad_input".into(),
                stdin: String::new(),
                args: vec!["bad".into()],
            },
        ];

        let result = run_multi_input_diff_test(c_source, rust_source, &inputs).unwrap();
        assert!(!result.all_passed, "should fail when one input mismatches");
        assert_eq!(result.results.len(), 3);
        assert!(result.results[0].1.passed, "good1 should pass");
        assert!(result.results[1].1.passed, "good2 should pass");
        assert!(!result.results[2].1.passed, "bad_input should fail");
    }

    #[test]
    fn test_multi_input_empty() {
        let c_source = r#"
#include <stdio.h>
int main(void) { printf("hello\n"); return 0; }
"#;
        let rust_source = r#"fn main() { println!("hello"); }"#;

        let result = run_multi_input_diff_test(c_source, rust_source, &[]).unwrap();
        assert!(result.all_passed, "empty inputs should run once and pass");
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].0, "default");
    }
}
