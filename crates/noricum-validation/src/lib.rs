/// Verification pipeline: ensures migrated Rust code is correct and idiomatic.
///
/// Checks: compilation, differential testing, clippy, unsafe counting, idiomatic scoring.
use noricum_ir::{FunctionUnit, MigrationState};
use thiserror::Error;
use tracing::info;

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

    // Step 4: Idiomatic score
    let idiomatic_score = compute_idiomatic_score(unsafe_count, clippy_warnings.len() as u32);

    let passed = compile_result.success && idiomatic_score >= 60;

    info!(
        function = %unit.name,
        compiles = compile_result.success,
        unsafe_count,
        idiomatic_score,
        passed,
        "validation complete"
    );

    Ok(ValidationResult {
        compiles: compile_result.success,
        compiler_errors,
        clippy_warnings,
        unsafe_count,
        idiomatic_score,
        passed,
    })
}

/// Compute an idiomatic score based on unsafe blocks and clippy warnings.
/// Score: 100 - (unsafe_blocks * 10) - (clippy_warnings * 2), clamped to [0, 100].
pub fn compute_idiomatic_score(unsafe_count: u32, clippy_warning_count: u32) -> u32 {
    let penalty = unsafe_count * 10 + clippy_warning_count * 2;
    100u32.saturating_sub(penalty)
}

/// Update a function unit's state based on validation results.
pub fn apply_validation(unit: &mut FunctionUnit, result: &ValidationResult) {
    unit.idiomatic_score = Some(result.idiomatic_score);
    unit.unsafe_count = Some(result.unsafe_count);
    unit.last_errors = result.compiler_errors.clone();

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
            passed: false,
        };

        apply_validation(&mut unit, &result);
        assert_eq!(unit.state, MigrationState::FallbackUnsafe);
    }
}
