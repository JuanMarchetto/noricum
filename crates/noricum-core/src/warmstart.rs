//! Warm-start module loading and seed strategy for modular migration.
//!
//! When a previous run's artifacts are available, this module determines
//! whether each module should be skipped, seeded, or retranslated.

/// Warm-start action for a module based on its previous run results.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WarmAction {
    /// Module was Validated — use directly, skip all processing.
    Skip,
    /// Module compiled but didn't reach threshold — start from its code, skip translation.
    SeedRepair,
    /// Module was garbage — re-translate from scratch.
    Retranslate,
}

/// Determine warm-start action for a module based on its previous artifact.
pub(crate) fn warm_start_action(artifact: &crate::artifacts::ModuleArtifact) -> WarmAction {
    match artifact.state.as_str() {
        // P23: CompilesUnsafe with high score is good enough to skip
        "Validated" | "CompilesUnsafe" if artifact.score >= 70.0 => WarmAction::Skip,
        // P23: NearlyCompiles with decent score is worth seeding
        "NearlyCompiles" if artifact.score >= 50.0 => WarmAction::SeedRepair,
        _ if artifact.compiles && artifact.score >= 40.0 => WarmAction::SeedRepair,
        _ => WarmAction::Retranslate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::ModuleArtifact;

    #[test]
    fn test_warm_start_strategy() {
        let validated = ModuleArtifact {
            name: "if".into(),
            state: "Validated".into(),
            score: 100.0,
            compiles: true,
            unsafe_count: 0,
        };
        assert_eq!(warm_start_action(&validated), WarmAction::Skip);

        let good_fallback = ModuleArtifact {
            name: "mz_p6".into(),
            state: "FallbackUnsafe".into(),
            score: 87.0,
            compiles: true,
            unsafe_count: 0,
        };
        assert_eq!(warm_start_action(&good_fallback), WarmAction::SeedRepair);

        let mid_fallback = ModuleArtifact {
            name: "mz_p3".into(),
            state: "FallbackUnsafe".into(),
            score: 50.0,
            compiles: true,
            unsafe_count: 0,
        };
        assert_eq!(warm_start_action(&mid_fallback), WarmAction::SeedRepair);

        let bad_fallback = ModuleArtifact {
            name: "mz_p8".into(),
            state: "FallbackUnsafe".into(),
            score: 5.0,
            compiles: false,
            unsafe_count: 0,
        };
        assert_eq!(warm_start_action(&bad_fallback), WarmAction::Retranslate);

        let skipped = ModuleArtifact {
            name: "mz_p2".into(),
            state: "Skipped".into(),
            score: 0.0,
            compiles: false,
            unsafe_count: 0,
        };
        assert_eq!(warm_start_action(&skipped), WarmAction::Retranslate);
    }

    #[test]
    fn test_warm_start_with_graduated_states() {
        let compiles_unsafe = ModuleArtifact {
            name: "mod_a".into(),
            state: "CompilesUnsafe".into(),
            score: 75.0,
            compiles: true,
            unsafe_count: 2,
        };
        assert_eq!(warm_start_action(&compiles_unsafe), WarmAction::Skip);

        let nearly = ModuleArtifact {
            name: "mod_b".into(),
            state: "NearlyCompiles".into(),
            score: 60.0,
            compiles: false,
            unsafe_count: 0,
        };
        assert_eq!(warm_start_action(&nearly), WarmAction::SeedRepair);

        let nearly_low = ModuleArtifact {
            name: "mod_c".into(),
            state: "NearlyCompiles".into(),
            score: 30.0,
            compiles: false,
            unsafe_count: 0,
        };
        assert_eq!(warm_start_action(&nearly_low), WarmAction::Retranslate);
    }
}
