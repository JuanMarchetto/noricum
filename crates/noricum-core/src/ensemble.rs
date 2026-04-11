//! Ensemble integration for the migration orchestrator.
//!
//! Determines when to trigger ensemble translation and applies
//! the result back to the FunctionUnit.

use noricum_agents::ensemble_translation::EnsembleResult;
use noricum_ir::{FunctionUnit, MigrationState};
use tracing::info;

/// Threshold: trigger ensemble if initial score is below this after N repair iterations.
pub const ENSEMBLE_TRIGGER_SCORE: u32 = 70;

/// Threshold: trigger ensemble after this many failed repair iterations.
pub const ENSEMBLE_TRIGGER_REPAIR_ITERS: u32 = 2;

/// Determine whether to trigger ensemble for a given unit.
pub fn should_trigger_ensemble(unit: &FunctionUnit, ensemble_enabled: bool) -> bool {
    if !ensemble_enabled {
        return false;
    }

    match unit.state {
        MigrationState::Repairing(n) if n >= ENSEMBLE_TRIGGER_REPAIR_ITERS => {
            let score = unit.idiomatic_score.unwrap_or(0);
            let compiles = unit.last_errors.is_empty();
            !compiles || score < ENSEMBLE_TRIGGER_SCORE
        }
        MigrationState::FallbackUnsafe => true,
        _ => false,
    }
}

/// Apply ensemble result to a FunctionUnit.
///
/// If the ensemble produced a better translation, replace the unit's rust_output
/// and reset state to Refined for re-validation.
pub fn apply_ensemble_result(unit: &mut FunctionUnit, result: &EnsembleResult) {
    if let Some(ref winner) = result.winner {
        let current_score = unit.idiomatic_score.unwrap_or(0);
        let current_compiles = unit.last_errors.is_empty();

        let winner_better =
            (winner.compiles && !current_compiles) || (winner.idiomatic_score > current_score);

        if winner_better {
            info!(
                function = %unit.name,
                winner_label = %winner.config_label,
                winner_score = winner.idiomatic_score,
                winner_compiles = winner.compiles,
                current_score,
                "P38: ensemble produced better translation, replacing"
            );
            unit.rust_output = Some(winner.rust_source.clone());
            unit.idiomatic_score = Some(winner.idiomatic_score);
            unit.unsafe_count = Some(winner.unsafe_count);
            unit.last_errors = winner.compiler_errors.clone();
            unit.state = MigrationState::Refined;
        } else {
            info!(
                function = %unit.name,
                "P38: ensemble did not improve on current translation"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noricum_agents::ensemble_translation::TranslationCandidate;
    use noricum_agents::providers::ProviderKind;

    #[test]
    fn test_should_trigger_ensemble_disabled() {
        let mut unit = FunctionUnit::new("f".into(), "f.c".into(), "int f() { return 0; }".into());
        unit.state = MigrationState::Repairing(3);
        assert!(!should_trigger_ensemble(&unit, false));
    }

    #[test]
    fn test_should_trigger_ensemble_early_repair() {
        let mut unit = FunctionUnit::new("f".into(), "f.c".into(), "int f() { return 0; }".into());
        unit.state = MigrationState::Repairing(1);
        assert!(!should_trigger_ensemble(&unit, true));
    }

    #[test]
    fn test_should_trigger_ensemble_after_threshold() {
        let mut unit = FunctionUnit::new("f".into(), "f.c".into(), "int f() { return 0; }".into());
        unit.state = MigrationState::Repairing(2);
        unit.last_errors = vec!["error".into()];
        assert!(should_trigger_ensemble(&unit, true));
    }

    #[test]
    fn test_should_trigger_ensemble_fallback_unsafe() {
        let mut unit = FunctionUnit::new("f".into(), "f.c".into(), "int f() { return 0; }".into());
        unit.state = MigrationState::FallbackUnsafe;
        assert!(should_trigger_ensemble(&unit, true));
    }

    #[test]
    fn test_should_not_trigger_ensemble_validated() {
        let mut unit = FunctionUnit::new("f".into(), "f.c".into(), "int f() { return 0; }".into());
        unit.state = MigrationState::Validated;
        assert!(!should_trigger_ensemble(&unit, true));
    }

    #[test]
    fn test_apply_ensemble_result_better_candidate() {
        let mut unit = FunctionUnit::new("f".into(), "f.c".into(), "int f() { return 0; }".into());
        unit.rust_output = Some("fn f() -> i32 { 0 }".into());
        unit.idiomatic_score = Some(50);
        unit.last_errors = vec!["error".into()];
        unit.state = MigrationState::Repairing(3);

        let result = EnsembleResult {
            winner: Some(TranslationCandidate {
                config_label: "claude-low".into(),
                provider: ProviderKind::Anthropic,
                temperature: 0.3,
                rust_source: "pub fn f() -> i32 { 0 }".into(),
                compiles: true,
                compiler_errors: vec![],
                idiomatic_score: 85,
                unsafe_count: 0,
                estimated_cost_usd: 0.05,
                specs_passed: None,
                specs_total: None,
            }),
            all_candidates: vec![],
            total_cost_usd: 0.05,
            compiled_count: 1,
        };

        apply_ensemble_result(&mut unit, &result);
        assert_eq!(unit.idiomatic_score, Some(85));
        assert!(unit.last_errors.is_empty());
        assert_eq!(unit.state, MigrationState::Refined);
    }

    #[test]
    fn test_apply_ensemble_result_no_improvement() {
        let mut unit = FunctionUnit::new("f".into(), "f.c".into(), "int f() { return 0; }".into());
        unit.rust_output = Some("pub fn f() -> i32 { 0 }".into());
        unit.idiomatic_score = Some(90);
        unit.state = MigrationState::Repairing(3);

        let result = EnsembleResult {
            winner: Some(TranslationCandidate {
                config_label: "ds-low".into(),
                provider: ProviderKind::DeepSeek,
                temperature: 0.3,
                rust_source: "fn f() -> i32 { 0 }".into(),
                compiles: true,
                compiler_errors: vec![],
                idiomatic_score: 70,
                unsafe_count: 0,
                estimated_cost_usd: 0.01,
                specs_passed: None,
                specs_total: None,
            }),
            all_candidates: vec![],
            total_cost_usd: 0.01,
            compiled_count: 1,
        };

        apply_ensemble_result(&mut unit, &result);
        assert_eq!(unit.idiomatic_score, Some(90), "should keep current score");
        assert_eq!(
            unit.state,
            MigrationState::Repairing(3),
            "should keep current state"
        );
    }
}
