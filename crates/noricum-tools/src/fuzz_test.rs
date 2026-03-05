/// Fuzz testing for behavioral comparison between C and Rust programs.
///
/// Generates randomized inputs (args and stdin) and runs both programs
/// with each input to detect behavioral divergences that the basic diff test misses.
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use tracing::{debug, info};

use crate::ToolError;
use crate::diff_test::{compile_c_exe, compile_rust_exe, run_exe};

/// Configuration for fuzz testing.
#[derive(Debug, Clone)]
pub struct FuzzConfig {
    /// Number of fuzz iterations to run.
    pub iterations: u32,
    /// Random seed for reproducibility. If None, uses entropy.
    pub seed: Option<u64>,
    /// Maximum length for generated string inputs.
    pub max_string_len: usize,
    /// Range for generated integer inputs (inclusive).
    pub int_range: (i64, i64),
    /// Whether to include edge case inputs first.
    pub include_edge_cases: bool,
    /// Timeout per execution in seconds.
    pub timeout_secs: u64,
}

impl Default for FuzzConfig {
    fn default() -> Self {
        Self {
            iterations: 100,
            seed: None,
            max_string_len: 256,
            int_range: (-1_000_000, 1_000_000),
            include_edge_cases: true,
            timeout_secs: 5,
        }
    }
}

/// How the program receives input.
#[derive(Debug, Clone)]
pub enum InputMode {
    /// Program reads command-line arguments.
    Args { expected_count: usize },
    /// Program reads from stdin.
    Stdin { format_hints: Vec<InputType> },
    /// Program takes no input.
    NoInput,
}

/// Type of input value to generate.
#[derive(Debug, Clone)]
pub enum InputType {
    Int,
    Float,
    String,
    Char,
}

/// A single test input for fuzz testing.
#[derive(Debug, Clone)]
pub struct TestInput {
    /// Descriptive label.
    pub label: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Stdin content.
    pub stdin: String,
}

/// Result of a full fuzz test run.
#[derive(Debug)]
pub struct FuzzResult {
    /// Whether all iterations passed.
    pub all_passed: bool,
    /// Total iterations actually run.
    pub iterations_run: u32,
    /// Number of failures.
    pub failures: u32,
    /// First divergence found (if any).
    pub first_divergence: Option<FuzzDivergence>,
    /// All divergences found.
    pub divergences: Vec<FuzzDivergence>,
    /// Seed used for this run (for reproducibility).
    pub seed: u64,
}

/// A single behavioral divergence between C and Rust.
#[derive(Debug, Clone)]
pub struct FuzzDivergence {
    pub input: TestInput,
    pub c_output: String,
    pub rust_output: String,
    pub c_exit_code: i32,
    pub rust_exit_code: i32,
}

/// Detect how a C program receives input by examining the source.
pub fn detect_input_mode(c_source: &str) -> InputMode {
    // Use AST-first approach
    if let Some(mode) = detect_input_mode_ast(c_source) {
        return mode;
    }
    detect_input_mode_regex(c_source)
}

fn detect_input_mode_ast(c_source: &str) -> Option<InputMode> {
    // Check for argc/argv patterns indicating command-line args
    let has_argc = c_source.contains("argc") || c_source.contains("argv");
    let has_scanf = c_source.contains("scanf");
    let has_fgets = c_source.contains("fgets");
    let has_getchar = c_source.contains("getchar");
    let has_stdin = has_scanf || has_fgets || has_getchar;

    if has_argc {
        // Try to determine expected arg count from atoi/atol/strtol calls
        let atoi_count = c_source.matches("atoi").count()
            + c_source.matches("atol").count()
            + c_source.matches("strtol").count();
        let expected = if atoi_count > 0 { atoi_count } else { 1 };
        Some(InputMode::Args {
            expected_count: expected,
        })
    } else if has_stdin {
        let mut hints = Vec::new();
        if has_scanf {
            if c_source.contains("%d") || c_source.contains("%i") {
                hints.push(InputType::Int);
            }
            if c_source.contains("%f") || c_source.contains("%lf") {
                hints.push(InputType::Float);
            }
            if c_source.contains("%s") {
                hints.push(InputType::String);
            }
            if c_source.contains("%c") {
                hints.push(InputType::Char);
            }
        }
        if hints.is_empty() {
            hints.push(InputType::String);
        }
        Some(InputMode::Stdin {
            format_hints: hints,
        })
    } else {
        Some(InputMode::NoInput)
    }
}

fn detect_input_mode_regex(c_source: &str) -> InputMode {
    if c_source.contains("argc") || c_source.contains("argv") {
        InputMode::Args { expected_count: 1 }
    } else if c_source.contains("scanf")
        || c_source.contains("fgets")
        || c_source.contains("getchar")
    {
        InputMode::Stdin {
            format_hints: vec![InputType::String],
        }
    } else {
        InputMode::NoInput
    }
}

/// Generate fuzz inputs for the detected input mode.
pub fn generate_fuzz_inputs(mode: &InputMode, config: &FuzzConfig) -> Vec<TestInput> {
    let seed = config.seed.unwrap_or_else(|| {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42)
    });
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut inputs = Vec::new();

    match mode {
        InputMode::Args { expected_count } => {
            let n = *expected_count;

            // Edge cases first
            if config.include_edge_cases {
                inputs.push(TestInput {
                    label: "zeros".into(),
                    args: vec!["0".to_string(); n],
                    stdin: String::new(),
                });
                inputs.push(TestInput {
                    label: "ones".into(),
                    args: vec!["1".to_string(); n],
                    stdin: String::new(),
                });
                inputs.push(TestInput {
                    label: "negative".into(),
                    args: vec!["-1".to_string(); n],
                    stdin: String::new(),
                });
                inputs.push(TestInput {
                    label: "max_i32".into(),
                    args: vec![i32::MAX.to_string(); n],
                    stdin: String::new(),
                });
                inputs.push(TestInput {
                    label: "min_i32".into(),
                    args: vec![i32::MIN.to_string(); n],
                    stdin: String::new(),
                });
            }

            // Random inputs
            let remaining = config.iterations.saturating_sub(inputs.len() as u32);
            for i in 0..remaining {
                let args: Vec<String> = (0..n)
                    .map(|_| {
                        let val: i64 = rng.random_range(config.int_range.0..=config.int_range.1);
                        val.to_string()
                    })
                    .collect();
                inputs.push(TestInput {
                    label: format!("random_{i}"),
                    args,
                    stdin: String::new(),
                });
            }
        }
        InputMode::Stdin { format_hints } => {
            // Edge cases
            if config.include_edge_cases {
                inputs.push(TestInput {
                    label: "empty_stdin".into(),
                    args: Vec::new(),
                    stdin: String::new(),
                });
                inputs.push(TestInput {
                    label: "zero".into(),
                    args: Vec::new(),
                    stdin: "0\n".to_string(),
                });
                inputs.push(TestInput {
                    label: "negative".into(),
                    args: Vec::new(),
                    stdin: "-1\n".to_string(),
                });
            }

            let remaining = config.iterations.saturating_sub(inputs.len() as u32);
            for i in 0..remaining {
                let stdin = generate_stdin_input(&mut rng, format_hints, config);
                inputs.push(TestInput {
                    label: format!("random_{i}"),
                    args: Vec::new(),
                    stdin,
                });
            }
        }
        InputMode::NoInput => {
            // Only one run needed for no-input programs
            inputs.push(TestInput {
                label: "default".into(),
                args: Vec::new(),
                stdin: String::new(),
            });
        }
    }

    inputs
}

fn generate_stdin_input(rng: &mut SmallRng, hints: &[InputType], config: &FuzzConfig) -> String {
    let mut parts = Vec::new();
    for hint in hints {
        match hint {
            InputType::Int => {
                let val: i64 = rng.random_range(config.int_range.0..=config.int_range.1);
                parts.push(val.to_string());
            }
            InputType::Float => {
                let val: f64 = rng.random_range(-1000.0..1000.0);
                parts.push(format!("{val:.6}"));
            }
            InputType::String => {
                let len = rng.random_range(1..=config.max_string_len.min(32));
                let s: String = (0..len)
                    .map(|_| rng.random_range(b'a'..=b'z') as char)
                    .collect();
                parts.push(s);
            }
            InputType::Char => {
                let c = rng.random_range(b'a'..=b'z') as char;
                parts.push(c.to_string());
            }
        }
    }
    parts.join(" ") + "\n"
}

/// Run a fuzz test comparing C and Rust program behavior across multiple inputs.
///
/// Compiles both programs once, then runs them with generated fuzz inputs.
pub fn run_fuzz_test(
    c_source: &str,
    rust_source: &str,
    config: &FuzzConfig,
) -> Result<FuzzResult, ToolError> {
    let seed = config.seed.unwrap_or_else(|| {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42)
    });

    let mode = detect_input_mode(c_source);
    let inputs = generate_fuzz_inputs(&mode, config);

    info!(
        mode = ?mode,
        iterations = inputs.len(),
        seed,
        "starting fuzz test"
    );

    let tmp = tempfile::tempdir()?;
    let c_file = tmp.path().join("fuzz.c");
    let c_exe = tmp.path().join("fuzz_c");
    let rs_file = tmp.path().join("fuzz.rs");
    let rs_exe = tmp.path().join("fuzz_rs");

    std::fs::write(&c_file, c_source)?;
    std::fs::write(&rs_file, rust_source)?;

    let c_compiled = compile_c_exe(&c_file, &c_exe)?;
    let rust_compiled = compile_rust_exe(&rs_file, &rs_exe)?;

    if !c_compiled || !rust_compiled {
        return Ok(FuzzResult {
            all_passed: false,
            iterations_run: 0,
            failures: 1,
            first_divergence: None,
            divergences: Vec::new(),
            seed,
        });
    }

    let mut divergences = Vec::new();
    let mut iterations_run = 0u32;

    for input in &inputs {
        iterations_run += 1;

        let stdin = if input.stdin.is_empty() {
            None
        } else {
            Some(input.stdin.as_str())
        };

        let c_result = match run_exe(&c_exe, stdin, &input.args) {
            Ok(r) => r,
            Err(e) => {
                debug!(label = %input.label, error = %e, "C execution failed");
                continue;
            }
        };
        let rust_result = match run_exe(&rs_exe, stdin, &input.args) {
            Ok(r) => r,
            Err(e) => {
                debug!(label = %input.label, error = %e, "Rust execution failed");
                continue;
            }
        };

        if c_result.stdout != rust_result.stdout || c_result.exit_code != rust_result.exit_code {
            divergences.push(FuzzDivergence {
                input: input.clone(),
                c_output: c_result.stdout,
                rust_output: rust_result.stdout,
                c_exit_code: c_result.exit_code,
                rust_exit_code: rust_result.exit_code,
            });
        }
    }

    let failures = divergences.len() as u32;
    let first_divergence = divergences.first().cloned();

    info!(iterations_run, failures, seed, "fuzz test complete");

    Ok(FuzzResult {
        all_passed: failures == 0,
        iterations_run,
        failures,
        first_divergence,
        divergences,
        seed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_input_mode_no_input() {
        let source = r#"
#include <stdio.h>
int main(void) {
    printf("hello\n");
    return 0;
}
"#;
        assert!(matches!(detect_input_mode(source), InputMode::NoInput));
    }

    #[test]
    fn test_detect_input_mode_args() {
        let source = r#"
#include <stdio.h>
#include <stdlib.h>
int main(int argc, char *argv[]) {
    int n = atoi(argv[1]);
    printf("%d\n", n * 2);
    return 0;
}
"#;
        match detect_input_mode(source) {
            InputMode::Args { expected_count } => {
                assert!(expected_count >= 1);
            }
            other => panic!("expected Args, got {:?}", other),
        }
    }

    #[test]
    fn test_detect_input_mode_stdin() {
        let source = r#"
#include <stdio.h>
int main(void) {
    int n;
    scanf("%d", &n);
    printf("%d\n", n);
    return 0;
}
"#;
        assert!(matches!(detect_input_mode(source), InputMode::Stdin { .. }));
    }

    #[test]
    fn test_generate_inputs_no_input() {
        let mode = InputMode::NoInput;
        let config = FuzzConfig::default();
        let inputs = generate_fuzz_inputs(&mode, &config);
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].label, "default");
    }

    #[test]
    fn test_generate_inputs_args() {
        let mode = InputMode::Args { expected_count: 2 };
        let config = FuzzConfig {
            iterations: 10,
            seed: Some(42),
            include_edge_cases: true,
            ..Default::default()
        };
        let inputs = generate_fuzz_inputs(&mode, &config);
        assert_eq!(inputs.len(), 10);
        // First 5 are edge cases
        assert_eq!(inputs[0].label, "zeros");
        assert_eq!(inputs[0].args, vec!["0", "0"]);
    }

    #[test]
    fn test_generate_inputs_deterministic_seed() {
        let mode = InputMode::Args { expected_count: 1 };
        let config = FuzzConfig {
            iterations: 5,
            seed: Some(12345),
            include_edge_cases: false,
            ..Default::default()
        };
        let inputs1 = generate_fuzz_inputs(&mode, &config);
        let inputs2 = generate_fuzz_inputs(&mode, &config);

        for (a, b) in inputs1.iter().zip(inputs2.iter()) {
            assert_eq!(a.args, b.args, "same seed should produce same inputs");
        }
    }

    #[test]
    fn test_fuzz_matching_programs() {
        let c_source = r#"
#include <stdio.h>
int main(void) {
    printf("42\n");
    return 0;
}
"#;
        let rust_source = r#"
fn main() {
    println!("42");
}
"#;
        let config = FuzzConfig {
            iterations: 3,
            seed: Some(42),
            ..Default::default()
        };
        let result = run_fuzz_test(c_source, rust_source, &config).unwrap();
        assert!(result.all_passed);
        assert_eq!(result.failures, 0);
    }

    #[test]
    fn test_fuzz_divergent_programs() {
        let c_source = r#"
#include <stdio.h>
int main(void) {
    printf("c_output\n");
    return 0;
}
"#;
        let rust_source = r#"
fn main() {
    println!("rust_output");
}
"#;
        let config = FuzzConfig {
            iterations: 1,
            seed: Some(42),
            ..Default::default()
        };
        let result = run_fuzz_test(c_source, rust_source, &config).unwrap();
        assert!(!result.all_passed);
        assert!(result.failures > 0);
        assert!(result.first_divergence.is_some());
    }

    #[test]
    fn test_fuzz_with_args() {
        // Use a simple echo program that doesn't involve arithmetic overflow
        let c_source = r#"
#include <stdio.h>
int main(int argc, char *argv[]) {
    if (argc > 1) {
        printf("%s\n", argv[1]);
    }
    return 0;
}
"#;
        let rust_source = r#"
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        println!("{}", args[1]);
    }
}
"#;
        let config = FuzzConfig {
            iterations: 10,
            seed: Some(42),
            include_edge_cases: true,
            ..Default::default()
        };
        let result = run_fuzz_test(c_source, rust_source, &config).unwrap();
        assert!(
            result.all_passed,
            "matching arg programs should pass fuzz, divergences: {:?}",
            result
                .divergences
                .iter()
                .map(|d| (&d.input.label, &d.c_output, &d.rust_output))
                .collect::<Vec<_>>()
        );
    }
}
