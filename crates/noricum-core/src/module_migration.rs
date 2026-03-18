//! Per-module migration pipeline for large C files.
//!
//! Splits a large C file into semantic modules (by function prefix) and
//! migrates each module independently through the translate -> validate -> repair
//! cycle, then assembles results.

use noricum_ir::{FunctionUnit, MigrationState};

/// LOC threshold above which chunked translation is used.
pub(crate) const MEDIUM_FILE_LOC: usize = 800;
/// LOC threshold above which repair iterations are further reduced and larger chunk targets apply.
pub(crate) const VERY_LARGE_FILE_LOC: usize = 2000;
/// LOC threshold for very large files where we use aggressive chunking and P11 signature agreement.
pub(crate) const MASSIVE_FILE_LOC: usize = 5000;
/// LOC threshold above which modular (per-module) migration is used instead of chunked.
pub(crate) const MODULAR_FILE_LOC: usize = 2000;
/// Number of compilation errors above which we re-translate instead of repairing.
pub(crate) const RETRANSLATE_ERROR_THRESHOLD: usize = 100;

/// Result of migrating a single module within a wave.
/// Contains all data needed to merge back into the shared state after parallel execution.
pub(crate) struct ModuleMigrationResult {
    /// Module name
    pub name: String,
    /// Index in the modules array (for ordering)
    pub mod_idx: usize,
    /// The migrated FunctionUnit (contains rust_output, state, scores, etc.)
    pub unit: Option<FunctionUnit>,
    /// Artifact entry for the v2 manifest
    pub artifact: crate::artifacts::ModuleArtifact,
    /// Metrics accumulated by this module's migration
    pub metrics: noricum_ir::MigrationMetrics,
    /// Whether the module reached Validated or equivalent
    pub validated: bool,
    /// Whether this was a warm-start skip (output already in artifact)
    pub was_skip: bool,
}

/// Result of modular migration attempt.
pub(crate) enum ModularResult {
    /// All modules migrated successfully.
    Success {
        rust_code: String,
        metrics: noricum_ir::MigrationMetrics,
        all_validated: bool,
    },
    /// Modular split was not useful, fall back to chunked translation.
    FallbackToChunked,
}

/// P23: Assign a graduated state based on compilation, unsafe, and score.
pub(crate) fn graduated_state(
    compiles: bool,
    unsafe_count: u32,
    score: u32,
    threshold: u32,
    error_count: usize,
) -> MigrationState {
    if compiles && unsafe_count == 0 && score >= threshold {
        MigrationState::Validated
    } else if compiles && unsafe_count > 0 && score >= 50 {
        MigrationState::CompilesUnsafe
    } else if compiles && score < threshold {
        MigrationState::CompilesLowScore
    } else if !compiles && error_count <= 5 && score >= 50 {
        MigrationState::NearlyCompiles
    } else {
        MigrationState::FallbackUnsafe
    }
}

/// Returns true if the module should be re-translated instead of repaired.
/// Criteria: more than 100 compilation errors AND hasn't been re-translated yet.
pub(crate) fn should_retranslate(error_count: usize, retranslation_attempts: u32) -> bool {
    error_count > RETRANSLATE_ERROR_THRESHOLD && retranslation_attempts == 0
}

/// Repair a single module within a crate context.
///
/// Compiles the entire crate via `cargo check`, extracts errors for the target module,
/// and runs mechanical rules + LLM repair on just that module's source.
/// Returns `true` if the module (and whole crate) compiles after repair.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn repair_module_in_crate(
    builder: &mut noricum_tools::crate_builder::CrateBuilder,
    module_name: &str,
    c_source: &str,
    client: &noricum_agents::LlmClient,
    repair_model: &str,
    max_iters: u32,
    repair_base_temperature: Option<f64>,
    max_unsafe_blocks: Option<u32>,
) -> Result<bool, crate::CoreError> {
    let mut iteration = 0u32;
    let baseline_unsafe = {
        let source = builder.read_module(module_name)
            .map_err(|e| crate::CoreError::Orchestration(format!("read module: {e}")))?;
        noricum_tools::ast::count_unsafe_blocks_ast(&source)
    };
    let unsafe_ceiling = max_unsafe_blocks.unwrap_or(baseline_unsafe).max(baseline_unsafe);

    while iteration < max_iters {
        // Compile the whole crate, filtering errors for this module
        let compile_result = noricum_tools::compiler::check_module_compiles(
            builder.output_dir(), module_name,
        ).map_err(crate::CoreError::Tool)?;

        if compile_result.success {
            return Ok(true);
        }

        let current_source = builder.read_module(module_name)
            .map_err(|e| crate::CoreError::Orchestration(format!("read module: {e}")))?;

        // Apply mechanical rules first (R0-R14)
        let errors = noricum_tools::repair_rules::parse_rustc_errors(&compile_result.stderr);
        let fixed = noricum_tools::repair_rules::apply_all_rules(&current_source, &errors);

        if fixed != current_source {
            builder.write_module(module_name, &fixed)
                .map_err(|e| crate::CoreError::Orchestration(format!("write module: {e}")))?;

            // Re-check after rules
            let re_check = noricum_tools::compiler::check_module_compiles(
                builder.output_dir(), module_name,
            ).map_err(crate::CoreError::Tool)?;

            if re_check.success {
                return Ok(true);
            }
        }

        // LLM repair on this module's source
        let error_strings: Vec<String> = errors.iter().map(|e| {
            format!("{}: {} (line {})", e.code, e.message, e.line)
        }).collect();

        let repaired = noricum_agents::repair::repair_function_full(
            client,
            repair_model,
            &current_source,
            &error_strings,
            &[], // no diff feedback at module level
            c_source,
            iteration + 1,
            max_iters,
            repair_base_temperature,
            None,
        ).await;

        match repaired {
            Ok(new_source) => {
                let new_unsafe = noricum_tools::ast::count_unsafe_blocks_ast(&new_source);
                if new_unsafe <= unsafe_ceiling {
                    builder.write_module(module_name, &new_source)
                        .map_err(|e| crate::CoreError::Orchestration(format!("write module: {e}")))?;
                }
            }
            Err(e) => {
                tracing::warn!(module = %module_name, error = %e, "repair LLM call failed");
            }
        }

        iteration += 1;
    }

    Ok(false)
}

/// Check if Rust output has substance relative to C source (not empty stubs).
pub(crate) fn has_substance(rust_source: &str, c_source: &str) -> bool {
    let c_nl = c_source.lines().filter(|l| !l.trim().is_empty()).count();
    let r_lines = rust_source.lines().filter(|l| !l.trim().is_empty()).count();
    let efn = noricum_validation::count_empty_functions(rust_source);
    let tfn = noricum_validation::count_total_functions(rust_source);
    !(c_nl > 20 && r_lines < c_nl / 4 || tfn > 3 && efn as f32 / tfn as f32 > 0.3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graduated_state_assignment() {
        // Compiles, 0 unsafe, score >= 80 -> Validated
        assert_eq!(
            graduated_state(true, 0, 85, 80, 0),
            MigrationState::Validated
        );
        // Compiles, 2 unsafe, score >= 50 -> CompilesUnsafe
        assert_eq!(
            graduated_state(true, 2, 70, 80, 0),
            MigrationState::CompilesUnsafe
        );
        // Compiles, 0 unsafe, score 45 -> CompilesLowScore
        assert_eq!(
            graduated_state(true, 0, 45, 80, 0),
            MigrationState::CompilesLowScore
        );
        // Doesn't compile, score 60, 3 errors -> NearlyCompiles
        assert_eq!(
            graduated_state(false, 0, 60, 80, 3),
            MigrationState::NearlyCompiles
        );
        // Doesn't compile, score 60, 10 errors -> FallbackUnsafe (too many errors)
        assert_eq!(
            graduated_state(false, 0, 60, 80, 10),
            MigrationState::FallbackUnsafe
        );
        // Doesn't compile, low score, 3 errors -> FallbackUnsafe
        assert_eq!(
            graduated_state(false, 0, 30, 80, 3),
            MigrationState::FallbackUnsafe
        );
    }

    #[test]
    fn test_should_retranslate_on_high_errors() {
        assert!(should_retranslate(150, 0));
        assert!(should_retranslate(101, 0));
        assert!(!should_retranslate(100, 0));
        assert!(!should_retranslate(50, 0));
        assert!(!should_retranslate(200, 1));
        assert!(!should_retranslate(200, 2));
    }

    #[test]
    fn test_module_validation_relaxed() {
        let compiles = true;
        let score = 55u32;
        let min_score = 60u32;

        let module_threshold = min_score.min(50);
        assert_eq!(module_threshold, 50);
        let module_passed = compiles && score >= module_threshold;
        assert!(module_passed, "compiling module with score 55 should pass");

        let file_passed = compiles && score >= 80;
        assert!(!file_passed, "as file, score 55 < 80 would fail");

        let module_threshold = 30u32.min(50);
        assert_eq!(module_threshold, 30);
    }

    #[test]
    fn test_modular_threshold_constant() {
        assert_eq!(
            MODULAR_FILE_LOC, 2000,
            "modular migration triggers at 2000 LOC"
        );
        assert!(MODULAR_FILE_LOC > MEDIUM_FILE_LOC);
    }
}
