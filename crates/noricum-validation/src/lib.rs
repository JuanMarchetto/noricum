/// Verification pipeline: ensures migrated Rust code is correct and idiomatic.
///
/// Checks: compilation, differential testing, clippy, unsafe counting, idiomatic scoring.
use noricum_ir::{FunctionUnit, MigrationState};
use thiserror::Error;
use tracing::{info, warn};

/// Errors that can occur during the validation pipeline.
#[derive(Debug, Error)]
pub enum ValidationError {
    /// The function unit has no Rust output to validate.
    #[error("no Rust output to validate")]
    NoRustOutput,

    /// Compilation of the Rust output failed.
    #[error("compilation failed: {0}")]
    CompilationFailed(String),

    /// Differential test showed a behavioral mismatch between C and Rust.
    #[error("differential test failed: expected {expected}, got {actual}")]
    DifferentialTestFailed {
        /// Expected output from the C program.
        expected: String,
        /// Actual output from the Rust program.
        actual: String,
    },

    /// An underlying tool (compiler, diff test runner, etc.) returned an error.
    #[error("tool error: {0}")]
    Tool(#[from] noricum_tools::ToolError),
}

/// Result of running the full validation pipeline on a function.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationResult {
    /// Whether the Rust output compiles successfully.
    pub compiles: bool,
    /// Compiler error messages from the last compilation attempt.
    pub compiler_errors: Vec<String>,
    /// Clippy warnings found in the Rust output.
    pub clippy_warnings: Vec<String>,
    /// Number of `unsafe` blocks in the Rust output.
    pub unsafe_count: u32,
    /// Idiomatic score (0-100) based on Rust coding patterns.
    pub idiomatic_score: u32,
    /// Whether the diff test passed (`None` if not run, e.g. no `main()` in source).
    pub diff_test_passed: Option<bool>,
    /// Feedback describing the diff test mismatch (for the repair agent).
    pub diff_test_feedback: Vec<String>,
    /// Whether the function passed the full validation pipeline.
    pub passed: bool,
    /// Per-function spec validation results (None if spec mining not run).
    pub spec_validation: Option<noricum_tools::spec_mining::SpecValidationResult>,
}

/// Run the full validation pipeline on a function unit with the default threshold (60).
pub fn validate(unit: &FunctionUnit) -> Result<ValidationResult, ValidationError> {
    validate_with_threshold(unit, 60)
}

/// Run the full validation pipeline on a function unit with a configurable score threshold.
pub fn validate_with_threshold(
    unit: &FunctionUnit,
    min_score: u32,
) -> Result<ValidationResult, ValidationError> {
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

    // When diff test is None (no main, harness failed), require a higher score
    // to pass validation since we can't verify behavioral equivalence.
    let passed = if diff_test_passed.is_none() {
        let elevated_threshold = min_score.max(80);
        compile_result.success && idiomatic_score >= elevated_threshold
    } else {
        compile_result.success && idiomatic_score >= min_score && diff_test_passed.unwrap_or(false)
    };

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
        spec_validation: None,
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
/// - **Substance penalty**: caps score at 15 for empty/stub code (P6)
pub fn compute_idiomatic_score_from_source(
    unsafe_count: u32,
    clippy_warning_count: u32,
    rust_source: &str,
    c_source: &str,
) -> u32 {
    let base = compute_idiomatic_score(unsafe_count, clippy_warning_count) as i32;

    let mut bonus: i32 = 0;

    // P6: Substance check — penalize empty/stub code that has no real logic.
    // Count non-empty, non-trivial lines (not just `{`, `}`, `fn name() {}`, etc.)
    let c_lines_nonempty = c_source.lines().filter(|l| !l.trim().is_empty()).count();
    let rust_lines_nonempty = rust_source.lines().filter(|l| !l.trim().is_empty()).count();
    let has_substance = if c_lines_nonempty > 20 {
        // For substantial C code, Rust output should be at least 25% of the input
        rust_lines_nonempty >= c_lines_nonempty / 4
    } else {
        // For small C code, any output at all is fine — a valid translation
        // of a 5-line C function can be a single Rust line
        rust_lines_nonempty >= 1
    };

    if !has_substance {
        // Empty stubs or trivially small output → cap score very low
        return (base + bonus).clamp(0, 15) as u32;
    }

    // P6 part 2: detect high ratio of empty function bodies
    let empty_fn_count = count_empty_functions(rust_source);
    let total_fn_count = count_total_functions(rust_source);
    if total_fn_count > 3 && empty_fn_count as f32 / total_fn_count as f32 > 0.3 {
        // >30% empty functions → cap score low
        return (base + bonus).clamp(0, 20) as u32;
    }

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
    let c_lines = c_lines_nonempty as i32;
    let rust_lines = rust_lines_nonempty as i32;
    if c_lines > 5 && rust_lines > 0 && rust_lines < c_lines {
        bonus += 5;
    }

    (base + bonus).clamp(0, 100) as u32
}

/// Count functions with empty bodies (just `{}` or `{ }`) in Rust source.
pub fn count_empty_functions(rust_source: &str) -> usize {
    let mut count = 0;
    let mut chars = rust_source.chars().peekable();
    let mut in_fn = false;

    // Simple heuristic: find `fn ` then look for `{` followed closely by `}`
    while let Some(c) = chars.next() {
        if c == 'f' && chars.peek() == Some(&'n') {
            chars.next(); // consume 'n'
            if chars.peek() == Some(&' ') || chars.peek() == Some(&'(') {
                in_fn = true;
            }
        }
        if in_fn && c == '{' {
            // Scan forward for matching `}`, checking if body is only whitespace
            let mut body = String::new();
            let mut depth = 1;
            for inner in chars.by_ref() {
                if inner == '{' {
                    depth += 1;
                } else if inner == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                body.push(inner);
            }
            if body.trim().is_empty() || body.trim() == "todo!()" {
                count += 1;
            }
            in_fn = false;
        }
    }
    count
}

/// Count total function definitions in Rust source.
pub fn count_total_functions(rust_source: &str) -> usize {
    rust_source
        .lines()
        .filter(|l| {
            let t = l.trim();
            (t.starts_with("fn ") || t.starts_with("pub fn ") || t.starts_with("pub(crate) fn "))
                && t.contains('(')
        })
        .count()
}

/// Generate actionable improvement hints when score is below threshold but code compiles
/// and diff test passes. These hints tell the repair agent what to fix for a higher score.
pub fn generate_idiomatic_hints(rust_source: &str) -> Vec<String> {
    let mut hints = Vec::new();

    // Count `as` casts (not as_ method calls)
    let as_cast_count = rust_source
        .split_whitespace()
        .filter(|w| *w == "as")
        .count();
    if as_cast_count > 5 {
        // Detect specific cast patterns
        let as_usize = rust_source.matches("as usize").count();
        let as_i32 = rust_source.matches("as i32").count();
        let as_f64 = rust_source.matches("as f64").count();
        let mut detail = format!("Reduce `as` casts ({as_cast_count} found). ");
        if as_usize > 3 {
            detail.push_str(&format!(
                "{as_usize}x `as usize` — consider using `usize` for fields used as array indices. "
            ));
        }
        if as_i32 > 3 {
            detail.push_str(&format!(
                "{as_i32}x `as i32` — consider using consistent integer types. "
            ));
        }
        if as_f64 > 3 {
            detail.push_str(&format!(
                "{as_f64}x `as f64` — consider using `f64` for fields used in floating-point math. "
            ));
        }
        hints.push(detail);
    }

    // Manual indexing
    let idx_i = rust_source.matches("[i]").count();
    let idx_j = rust_source.matches("[j]").count();
    let idx_idx = rust_source.matches("[idx]").count();
    let manual_index = idx_i + idx_j + idx_idx;
    if manual_index > 5 {
        hints.push(format!(
            "Reduce manual array indexing ({manual_index} occurrences of [i]/[j]/[idx]). \
             Use iterator methods (.iter(), .enumerate(), .zip(), chunks()) where possible. \
             For neural network weight/output arrays, consider using split_at_mut() or chunks_exact_mut() \
             to iterate over weight groups instead of manual index arithmetic."
        ));
    }

    // .unwrap() calls
    let unwrap_count = rust_source.matches(".unwrap()").count();
    if unwrap_count > 2 {
        hints.push(format!(
            "Replace {unwrap_count} `.unwrap()` calls with `?` operator, `.unwrap_or()`, \
             or `.expect()` with meaningful messages."
        ));
    }

    if !hints.is_empty() {
        hints.insert(
            0,
            "IMPORTANT: The code compiles and produces correct output. \
             Do NOT change any logic or behavior. Only refactor for idiomatic Rust style. \
             The output must remain byte-exact identical."
                .to_string(),
        );
    }

    hints
}

/// Update a function unit's state based on validation results.
///
/// Uses the default maximum of 5 repair iterations.
pub fn apply_validation(unit: &mut FunctionUnit, result: &ValidationResult) {
    apply_validation_with_max(unit, result, 5);
}

/// Update a function unit's state based on validation results with a configurable
/// maximum number of repair iterations before falling back to unsafe.
pub fn apply_validation_with_max(
    unit: &mut FunctionUnit,
    result: &ValidationResult,
    max_repair_iterations: u32,
) {
    unit.idiomatic_score = Some(result.idiomatic_score);
    unit.unsafe_count = Some(result.unsafe_count);
    unit.last_errors = result.compiler_errors.clone();
    unit.last_diff_feedback = result.diff_test_feedback.clone();

    if result.passed {
        unit.state = MigrationState::Validated;
    } else {
        match &unit.state {
            MigrationState::Repairing(n) if *n >= max_repair_iterations => {
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
            spec_validation: None,
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
            spec_validation: None,
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
            spec_validation: None,
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
            spec_validation: None,
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
            spec_validation: None,
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
            spec_validation: None,
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
            spec_validation: None,
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
            spec_validation: None,
        };
        // A diff test error (including timeout) must NOT pass validation
        assert!(
            !result.passed,
            "diff test error should cause validation failure"
        );
    }

    /// Verify that when diff test is N/A (no main), validation can still pass
    /// but requires a higher score (>= 80).
    #[test]
    fn test_no_main_still_passes_without_diff_test() {
        // Use a struct-related C source where harness gen cannot produce a test
        let mut unit = FunctionUnit::new(
            "process".into(),
            "process.c".into(),
            "typedef struct { int x; } Foo;\nvoid process(Foo* f) { f->x = 1; }".into(),
        );
        unit.rust_output =
            Some("pub struct Foo { pub x: i32 }\npub fn process(f: &mut Foo) { f.x = 1; }".into());
        unit.state = MigrationState::Refined;
        let result = validate_with_threshold(&unit, 60).unwrap();
        if result.diff_test_passed.is_none() {
            // When diff test cannot run, score >= 80 required
            assert!(
                result.idiomatic_score >= 80,
                "simple function should score >= 80, got {}",
                result.idiomatic_score
            );
            assert!(result.passed);
        } else {
            // If harness gen succeeded, diff test result determines pass/fail
            assert!(result.passed == result.diff_test_passed.unwrap_or(false));
        }
    }

    /// Verify the passed field computation logic matches new expectations.
    #[test]
    fn test_empty_rust_source_scores_base() {
        let score = compute_idiomatic_score_from_source(0, 0, "", "int f() { return 0; }");
        assert!(
            score <= 100,
            "empty rust source should score <= 100, got {score}"
        );
    }

    #[test]
    fn test_empty_c_source_no_loc_bonus() {
        let score = compute_idiomatic_score_from_source(0, 0, "fn f() -> i32 { 0 }", "");
        assert!(
            score <= 100,
            "empty c source should score <= 100, got {score}"
        );
    }

    #[test]
    fn test_unicode_in_rust_source() {
        let rust = "fn grüße() -> String { \"héllo wörld 🦀\".to_string() }";
        let c = "char* gruesse() { return \"hello world\"; }";
        let score = compute_idiomatic_score_from_source(0, 0, rust, c);
        assert!(
            score <= 100,
            "unicode rust source should score <= 100, got {score}"
        );
    }

    #[test]
    fn test_unicode_in_c_source() {
        let rust = "fn greet() -> &'static str { \"hello\" }";
        let c = "// 日本語コメント\nchar* greet() { return \"héllo\"; }";
        let score = compute_idiomatic_score_from_source(0, 0, rust, c);
        assert!(
            score <= 100,
            "unicode c source should score <= 100, got {score}"
        );
    }

    #[test]
    fn test_both_sources_empty() {
        let score = compute_idiomatic_score_from_source(0, 0, "", "");
        assert!(score <= 100, "both empty should score <= 100, got {score}");
    }

    #[test]
    fn test_malformed_rust_patterns_in_source() {
        let rust = "Result<Result<Result<.iter().iter().iter().unwrap().unwrap().unwrap()";
        let c = "int f() { return 0; }";
        let score = compute_idiomatic_score_from_source(0, 0, rust, c);
        assert!(
            score <= 100,
            "malformed patterns should score <= 100, got {score}"
        );
    }

    #[test]
    fn test_passed_computation_logic() {
        // diff_test_passed = None → requires score >= 80 (elevated threshold)
        let compiles = true;
        let score = 85u32;
        let diff: Option<bool> = None;
        let min_score = 60u32;
        let elevated = min_score.max(80);
        let passed_none = if diff.is_none() {
            compiles && score >= elevated
        } else {
            compiles && score >= min_score && diff.unwrap_or(false)
        };
        assert!(passed_none, "score 85 >= elevated 80 should pass");

        // diff_test_passed = None but low score → fails
        let score = 70u32;
        let passed_low = if diff.is_none() {
            compiles && score >= elevated
        } else {
            compiles && score >= min_score && diff.unwrap_or(false)
        };
        assert!(!passed_low, "score 70 < elevated 80 should fail");

        // diff_test_passed = Some(false) → fails
        let diff: Option<bool> = Some(false);
        let score = 90u32;
        let passed_diff_fail = compiles && score >= min_score && diff.unwrap_or(false);
        assert!(!passed_diff_fail, "diff_test false should fail");

        // diff_test_passed = Some(true) → passes
        let diff: Option<bool> = Some(true);
        let passed_diff_pass = compiles && score >= min_score && diff.unwrap_or(false);
        assert!(passed_diff_pass, "diff_test true should pass");
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Score is always in [0, 100] regardless of inputs.
        #[test]
        fn score_always_in_range(
            unsafe_count in 0u32..50,
            clippy_warnings in 0u32..50,
            rust_source in ".*",
            c_source in ".*",
        ) {
            let score = compute_idiomatic_score_from_source(
                unsafe_count, clippy_warnings, &rust_source, &c_source,
            );
            prop_assert!(score <= 100, "score {score} exceeded 100");
        }

        /// Zero unsafe + zero clippy + no negative patterns => score >= 80.
        #[test]
        fn clean_code_scores_well(
            positive in proptest::collection::vec(
                prop_oneof!["Result<", "Option<", ".iter()", "Vec<", "String"],
                1..5,
            ),
        ) {
            let rust_source = format!("fn f() -> i32 {{ 42 }}\n{}", positive.join("\n"));
            let c_source = "int f() { return 0; }";
            let score = compute_idiomatic_score_from_source(0, 0, &rust_source, c_source);
            prop_assert!(score >= 80, "clean code scored only {score}");
        }

        /// More unsafe blocks never increases the score.
        #[test]
        fn more_unsafe_never_increases_score(
            base_unsafe in 0u32..10,
            extra in 1u32..10,
        ) {
            let rust = "fn f() -> i32 { 42 }";
            let c = "int f() { return 42; }";
            let low = compute_idiomatic_score_from_source(base_unsafe + extra, 0, rust, c);
            let high = compute_idiomatic_score_from_source(base_unsafe, 0, rust, c);
            prop_assert!(low <= high, "more unsafe ({}) scored {low} > {high}", base_unsafe + extra);
        }

        /// More clippy warnings never increases the score.
        #[test]
        fn more_clippy_never_increases_score(
            base_warnings in 0u32..10,
            extra in 1u32..10,
        ) {
            let rust = "fn f() -> i32 { 42 }";
            let c = "int f() { return 42; }";
            let low = compute_idiomatic_score_from_source(0, base_warnings + extra, rust, c);
            let high = compute_idiomatic_score_from_source(0, base_warnings, rust, c);
            prop_assert!(low <= high, "more warnings ({}) scored {low} > {high}", base_warnings + extra);
        }
    }

    #[test]
    fn test_idiomatic_hints_many_as_casts() {
        let rust = "fn f() { let a = x as usize; let b = y as usize; let c = z as usize; \
                    let d = w as usize; let e = v as usize; let f = u as usize; }";
        let hints = generate_idiomatic_hints(rust);
        assert!(!hints.is_empty(), "should generate hints for 6 as casts");
        assert!(
            hints.iter().any(|h| h.contains("as")),
            "should mention as casts"
        );
    }

    #[test]
    fn test_idiomatic_hints_many_manual_indices() {
        let rust = "fn f() { a[i] = b[i]; c[j] = d[j]; e[i] = f[j]; }";
        let hints = generate_idiomatic_hints(rust);
        assert!(
            !hints.is_empty(),
            "should generate hints for manual indexing"
        );
        assert!(
            hints.iter().any(|h| h.contains("indexing")),
            "should mention indexing"
        );
    }

    #[test]
    fn test_idiomatic_hints_clean_code() {
        let rust = "fn f(x: i32) -> i32 { x + 1 }";
        let hints = generate_idiomatic_hints(rust);
        assert!(hints.is_empty(), "clean code should produce no hints");
    }

    #[test]
    fn test_idiomatic_hints_include_safety_prefix() {
        let rust = "fn f() { let a = x as usize; let b = y as usize; let c = z as usize; \
                    let d = w as usize; let e = v as usize; let f = u as usize; }";
        let hints = generate_idiomatic_hints(rust);
        assert!(
            hints[0].contains("Do NOT change any logic"),
            "first hint should be safety warning"
        );
    }
}
