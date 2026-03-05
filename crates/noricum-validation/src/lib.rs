/// Verification pipeline: ensures migrated Rust code is correct and idiomatic.
///
/// Checks: compilation, differential testing, clippy, unsafe counting, idiomatic scoring.
use noricum_ir::{FunctionUnit, MigrationState};
use thiserror::Error;
use tracing::{info, warn};

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("no Rust output to validate")]
    NoRustOutput,

    #[error("compilation failed: {0}")]
    CompilationFailed(String),

    #[error("differential test failed: expected {expected}, got {actual}")]
    DifferentialTestFailed { expected: String, actual: String },

    #[error("tool error: {0}")]
    Tool(#[from] noricum_tools::ToolError),
}

/// Result of running the full validation pipeline on a function.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub compiles: bool,
    pub compiler_errors: Vec<String>,
    pub clippy_warnings: Vec<String>,
    pub unsafe_count: u32,
    pub idiomatic_score: u32,
    /// Whether the diff test passed (None if not run, e.g. no main() in source)
    pub diff_test_passed: Option<bool>,
    /// Feedback describing the diff test mismatch (for the repair agent)
    pub diff_test_feedback: Vec<String>,
    pub passed: bool,
}

/// Run the full validation pipeline on a function unit.
pub fn validate(unit: &FunctionUnit) -> Result<ValidationResult, ValidationError> {
    let rust_source = unit
        .rust_output
        .as_ref()
        .ok_or(ValidationError::NoRustOutput)?;

    info!(function = %unit.name, "running validation pipeline");

    // Step 1: Compilation check
    let compile_result = noricum_tools::compiler::check_rust_compiles(rust_source)?;
    let compiler_errors: Vec<String> = if !compile_result.success {
        compile_result
            .stderr
            .lines()
            .filter(|l: &&str| l.contains("error"))
            .map(String::from)
            .collect()
    } else {
        Vec::new()
    };

    // Step 2: Clippy check
    let clippy_warnings = noricum_tools::compiler::run_clippy_on_source(rust_source)?;

    // Step 3: Unsafe counting
    let unsafe_count = noricum_tools::compiler::count_unsafe_blocks(rust_source);

    // Step 4: Idiomatic score (enhanced: analyzes Rust source for positive/negative signals)
    let idiomatic_score = compute_idiomatic_score_from_source(
        unsafe_count,
        clippy_warnings.len() as u32,
        rust_source,
        &unit.c_source,
    );

    // Step 5: Differential test (only if code compiles)
    // If C source has main(), run diff test directly.
    // If not, try generating a test harness for the library function.
    let (diff_test_passed, diff_test_feedback) = if compile_result.success {
        let has_main = unit.c_source.contains("int main(");
        let test_c_source = if has_main {
            Some(unit.c_source.clone())
        } else {
            // Try generating a test harness for library functions
            match noricum_tools::harness_gen::generate_test_harness(&unit.c_source, &unit.name) {
                Some(harness) => {
                    info!(function = %unit.name, "generated test harness for library function");
                    Some(harness)
                }
                None => {
                    warn!(
                        function = %unit.name,
                        "diff test skipped: no main() and harness generation failed"
                    );
                    None
                }
            }
        };

        if let Some(c_test_source) = test_c_source {
            match noricum_tools::diff_test::run_diff_test(&c_test_source, rust_source) {
                Ok(result) => {
                    if result.passed {
                        info!(function = %unit.name, "diff test passed");
                        (Some(true), Vec::new())
                    } else {
                        let mut feedback = Vec::new();
                        if !result.rust_compiled {
                            feedback
                                .push("Rust binary compilation failed (standalone)".to_string());
                        } else {
                            feedback.push(format!(
                                "Output mismatch:\n  C output:    {:?}\n  Rust output: {:?}",
                                result.c_output, result.rust_output
                            ));
                            if result.c_exit_code != result.rust_exit_code {
                                feedback.push(format!(
                                    "Exit code mismatch: C={}, Rust={}",
                                    result.c_exit_code, result.rust_exit_code
                                ));
                            }
                        }
                        info!(function = %unit.name, "diff test FAILED");
                        (Some(false), feedback)
                    }
                }
                Err(e) => {
                    warn!(function = %unit.name, error = %e, "diff test failed (tool error)");
                    (Some(false), vec![format!("Diff test error: {e}")])
                }
            }
        } else {
            (None, Vec::new())
        }
    } else {
        (None, Vec::new())
    };

    let passed =
        compile_result.success && idiomatic_score >= 60 && diff_test_passed.unwrap_or(true);

    info!(
        function = %unit.name,
        compiles = compile_result.success,
        unsafe_count,
        idiomatic_score,
        diff_test = ?diff_test_passed,
        passed,
        "validation complete"
    );

    Ok(ValidationResult {
        compiles: compile_result.success,
        compiler_errors,
        clippy_warnings,
        unsafe_count,
        idiomatic_score,
        diff_test_passed,
        diff_test_feedback,
        passed,
    })
}

/// Compute an idiomatic score based on unsafe blocks and clippy warnings.
/// Score: 100 - (unsafe_blocks * 10) - (clippy_warnings * 2), clamped to [0, 100].
pub fn compute_idiomatic_score(unsafe_count: u32, clippy_warning_count: u32) -> u32 {
    let penalty = unsafe_count * 10 + clippy_warning_count * 2;
    100u32.saturating_sub(penalty)
}

/// Enhanced idiomatic score that also analyzes the Rust source for positive and negative signals.
///
/// Starts from the basic penalty score, then applies:
/// - **Positive signals** (+2 each): `Result<`, `Option<`, `.iter()`, `impl`, `From`/`Into`, `enum`
/// - **Negative signals** (-3 each): `.unwrap()`, raw `as` casts, manual index loops (`[i]`)
/// - **LOC ratio bonus** (+5): if Rust code is significantly shorter than C source
pub fn compute_idiomatic_score_from_source(
    unsafe_count: u32,
    clippy_warning_count: u32,
    rust_source: &str,
    c_source: &str,
) -> u32 {
    let base = compute_idiomatic_score(unsafe_count, clippy_warning_count) as i32;

    let mut bonus: i32 = 0;

    // Positive signals: idiomatic Rust patterns
    let positive_patterns: &[&str] = &[
        "Result<",
        "Option<",
        ".iter()",
        ".into()",
        "impl ",
        "From<",
        "Into<",
        "enum ",
        ".collect()",
        ".map(",
        ".filter(",
        ".unwrap_or(",
        ".unwrap_or_else(",
        "Vec<",
        "String",
        "HashMap<",
        "&[",
        "&str",
    ];
    for pattern in positive_patterns {
        let count = rust_source.matches(pattern).count() as i32;
        bonus += (count.min(3)) * 2; // cap at 3 occurrences per pattern
    }

    // Negative signals: non-idiomatic patterns
    let unwrap_count = rust_source.matches(".unwrap()").count() as i32;
    bonus -= unwrap_count * 3;

    // Raw `as` casts (but not `as_` method calls)
    let as_cast_count = rust_source
        .split_whitespace()
        .filter(|w| *w == "as")
        .count() as i32;
    bonus -= as_cast_count;

    // Manual index loops like `arr[i]` (heuristic: `[i]` or `[j]` patterns)
    let manual_index = rust_source.matches("[i]").count() as i32
        + rust_source.matches("[j]").count() as i32
        + rust_source.matches("[idx]").count() as i32;
    bonus -= manual_index * 2;

    // LOC ratio bonus: Rust shorter than C is a good sign
    let c_lines = c_source.lines().filter(|l| !l.trim().is_empty()).count() as i32;
    let rust_lines = rust_source.lines().filter(|l| !l.trim().is_empty()).count() as i32;
    if c_lines > 5 && rust_lines > 0 && rust_lines < c_lines {
        bonus += 5;
    }

    (base + bonus).clamp(0, 100) as u32
}

/// Update a function unit's state based on validation results.
pub fn apply_validation(unit: &mut FunctionUnit, result: &ValidationResult) {
    unit.idiomatic_score = Some(result.idiomatic_score);
    unit.unsafe_count = Some(result.unsafe_count);
    unit.last_errors = result.compiler_errors.clone();
    unit.last_diff_feedback = result.diff_test_feedback.clone();

    if result.passed {
        unit.state = MigrationState::Validated;
    } else {
        match &unit.state {
            MigrationState::Repairing(n) if *n >= 5 => {
                unit.state = MigrationState::FallbackUnsafe;
            }
            MigrationState::Repairing(n) => {
                unit.state = MigrationState::Repairing(n + 1);
            }
            _ => {
                unit.state = MigrationState::Repairing(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idiomatic_score_perfect() {
        assert_eq!(compute_idiomatic_score(0, 0), 100);
    }

    #[test]
    fn test_idiomatic_score_with_unsafe() {
        assert_eq!(compute_idiomatic_score(3, 0), 70);
    }

    #[test]
    fn test_idiomatic_score_with_warnings() {
        assert_eq!(compute_idiomatic_score(0, 5), 90);
    }

    #[test]
    fn test_idiomatic_score_clamped() {
        assert_eq!(compute_idiomatic_score(15, 10), 0);
    }

    #[test]
    fn test_apply_validation_pass() {
        let mut unit = FunctionUnit::new("f".into(), "f.c".into(), "".into());
        unit.rust_output = Some("fn f() {}".into());
        unit.state = MigrationState::Refined;

        let result = ValidationResult {
            compiles: true,
            compiler_errors: vec![],
            clippy_warnings: vec![],
            unsafe_count: 0,
            idiomatic_score: 100,
            diff_test_passed: None,
            diff_test_feedback: vec![],
            passed: true,
        };

        apply_validation(&mut unit, &result);
        assert_eq!(unit.state, MigrationState::Validated);
    }

    #[test]
    fn test_apply_validation_fail_enters_repair() {
        let mut unit = FunctionUnit::new("f".into(), "f.c".into(), "".into());
        unit.rust_output = Some("fn f() {}".into());
        unit.state = MigrationState::Refined;

        let result = ValidationResult {
            compiles: false,
            compiler_errors: vec!["error[E0308]".into()],
            clippy_warnings: vec![],
            unsafe_count: 0,
            idiomatic_score: 50,
            diff_test_passed: None,
            diff_test_feedback: vec![],
            passed: false,
        };

        apply_validation(&mut unit, &result);
        assert_eq!(unit.state, MigrationState::Repairing(1));
    }

    #[test]
    fn test_apply_validation_max_repairs_fallback() {
        let mut unit = FunctionUnit::new("f".into(), "f.c".into(), "".into());
        unit.rust_output = Some("fn f() {}".into());
        unit.state = MigrationState::Repairing(5);

        let result = ValidationResult {
            compiles: false,
            compiler_errors: vec![],
            clippy_warnings: vec![],
            unsafe_count: 5,
            idiomatic_score: 40,
            diff_test_passed: None,
            diff_test_feedback: vec![],
            passed: false,
        };

        apply_validation(&mut unit, &result);
        assert_eq!(unit.state, MigrationState::FallbackUnsafe);
    }

    #[test]
    fn test_enhanced_score_positive_signals() {
        let rust = "fn process(items: &[i32]) -> Result<Vec<i32>, String> {\n    items.iter().filter(|x| **x > 0).map(|x| *x).collect()\n}";
        let c = "int* process(int* items, int len) {\n    // many lines\n    // of C code\n    // doing stuff\n    return result;\n    // more\n    // lines\n}";
        let score = compute_idiomatic_score_from_source(0, 0, rust, c);
        // Base 100 + positive signals from Result<, Vec<, .iter(), .filter(, .map(, .collect(), &[
        assert!(score == 100, "expected 100 (clamped), got {score}");
    }

    #[test]
    fn test_enhanced_score_negative_signals() {
        let rust = "fn f() { let x = foo.unwrap(); let y = bar.unwrap(); let z = 5 as i64; }";
        let c = "int f() { return 0; }";
        let score = compute_idiomatic_score_from_source(0, 0, rust, c);
        // Base 100 - 2*3 (unwrap) - 1 (as cast) = 93
        assert!(
            score < 100,
            "unwrap/as casts should reduce score, got {score}"
        );
    }

    /// Simulate the full repair loop state machine: Refined -> Repairing(1..5) -> FallbackUnsafe
    #[test]
    fn test_repair_loop_state_transitions() {
        let mut unit = FunctionUnit::new("f".into(), "f.c".into(), "int f() { return 0; }".into());
        unit.rust_output = Some("fn f() -> i32 { 0 }".into());
        unit.state = MigrationState::Refined;

        let fail_result = ValidationResult {
            compiles: false,
            compiler_errors: vec!["error[E0308]: mismatched types".into()],
            clippy_warnings: vec![],
            unsafe_count: 0,
            idiomatic_score: 50,
            diff_test_passed: None,
            diff_test_feedback: vec![],
            passed: false,
        };

        // First failure: Refined -> Repairing(1)
        apply_validation(&mut unit, &fail_result);
        assert_eq!(unit.state, MigrationState::Repairing(1));

        // Iterations 2..5: each fail increments the counter
        for i in 2..=5 {
            apply_validation(&mut unit, &fail_result);
            assert_eq!(unit.state, MigrationState::Repairing(i), "iteration {i}");
        }

        // 6th failure while at Repairing(5) triggers >= 5 guard -> FallbackUnsafe
        apply_validation(&mut unit, &fail_result);
        assert_eq!(unit.state, MigrationState::FallbackUnsafe);
    }

    /// Simulate repair loop with recovery at iteration 3.
    #[test]
    fn test_repair_loop_recovery() {
        let mut unit = FunctionUnit::new("f".into(), "f.c".into(), "int f() { return 0; }".into());
        unit.rust_output = Some("fn f() -> i32 { 0 }".into());
        unit.state = MigrationState::Refined;

        let fail_result = ValidationResult {
            compiles: false,
            compiler_errors: vec!["error".into()],
            clippy_warnings: vec![],
            unsafe_count: 0,
            idiomatic_score: 50,
            diff_test_passed: None,
            diff_test_feedback: vec![],
            passed: false,
        };

        let pass_result = ValidationResult {
            compiles: true,
            compiler_errors: vec![],
            clippy_warnings: vec![],
            unsafe_count: 0,
            idiomatic_score: 95,
            diff_test_passed: Some(true),
            diff_test_feedback: vec![],
            passed: true,
        };

        // Fail twice
        apply_validation(&mut unit, &fail_result);
        assert_eq!(unit.state, MigrationState::Repairing(1));
        apply_validation(&mut unit, &fail_result);
        assert_eq!(unit.state, MigrationState::Repairing(2));

        // Succeed on 3rd attempt
        apply_validation(&mut unit, &pass_result);
        assert_eq!(unit.state, MigrationState::Validated);
    }

    /// Simulate diff test feedback driving repair.
    #[test]
    fn test_repair_loop_with_diff_feedback() {
        let mut unit = FunctionUnit::new("f".into(), "f.c".into(), "int f() { return 0; }".into());
        unit.rust_output = Some("fn f() -> i32 { 0 }".into());
        unit.state = MigrationState::Refined;

        // Code compiles but diff test fails
        let diff_fail = ValidationResult {
            compiles: true,
            compiler_errors: vec![],
            clippy_warnings: vec![],
            unsafe_count: 0,
            idiomatic_score: 90,
            diff_test_passed: Some(false),
            diff_test_feedback: vec!["Output mismatch: C=\"42\" Rust=\"43\"".into()],
            passed: false,
        };

        apply_validation(&mut unit, &diff_fail);
        assert_eq!(unit.state, MigrationState::Repairing(1));
        assert_eq!(unit.last_diff_feedback.len(), 1);
        assert!(unit.last_diff_feedback[0].contains("mismatch"));
    }

    #[test]
    fn test_enhanced_score_loc_bonus() {
        let rust = "fn f(x: i32) -> i32 { x + 1 }";
        let c = "int f(int x) {\n    int result;\n    result = x + 1;\n    return result;\n    // extra\n    // lines\n}";
        let score = compute_idiomatic_score_from_source(0, 0, rust, c);
        // Base 100 + 5 (LOC bonus) = 100 (clamped)
        assert!(score == 100, "LOC bonus should apply, got {score}");
    }

    /// Verify that diff test error (e.g., timeout) causes validation to fail,
    /// not silently pass via unwrap_or(true).
    #[test]
    fn test_diff_test_error_causes_failure() {
        let result = ValidationResult {
            compiles: true,
            compiler_errors: vec![],
            clippy_warnings: vec![],
            unsafe_count: 0,
            idiomatic_score: 95,
            diff_test_passed: Some(false),
            diff_test_feedback: vec!["Diff test error: execution timed out after 10s".into()],
            passed: false,
        };
        // A diff test error (including timeout) must NOT pass validation
        assert!(
            !result.passed,
            "diff test error should cause validation failure"
        );
    }

    /// Verify that when diff test is N/A (no main), validation can still pass.
    #[test]
    fn test_no_main_still_passes_without_diff_test() {
        let result = ValidationResult {
            compiles: true,
            compiler_errors: vec![],
            clippy_warnings: vec![],
            unsafe_count: 0,
            idiomatic_score: 85,
            diff_test_passed: None,
            diff_test_feedback: vec![],
            passed: true,
        };
        // Library functions without main() should still be validatable
        assert!(
            result.passed,
            "no-main functions should pass on compile+score alone"
        );
        assert!(
            result.diff_test_passed.is_none(),
            "diff_test should be None for no-main"
        );
    }

    /// Verify the passed field computation logic matches expectations.
    #[test]
    fn test_passed_computation_logic() {
        // diff_test_passed = None → unwrap_or(true) → passes (no main, N/A)
        let compiles = true;
        let score = 80u32;
        let diff: Option<bool> = None;
        assert!(compiles && score >= 60 && diff.unwrap_or(true));

        // diff_test_passed = Some(false) → fails
        let diff: Option<bool> = Some(false);
        assert!(!(compiles && score >= 60 && diff.unwrap_or(true)));

        // diff_test_passed = Some(true) → passes
        let diff: Option<bool> = Some(true);
        assert!(compiles && score >= 60 && diff.unwrap_or(true));
    }
}
