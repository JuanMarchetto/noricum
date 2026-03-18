//! Ensemble translation: generate multiple translation candidates and select the best.
//!
//! When a single-pass translation fails, the ensemble generates N candidates using
//! different providers and temperatures, then filters and ranks by objective criteria.

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::providers::ProviderKind;

// ---------------------------------------------------------------------------
// Configuration types (Task 1)
// ---------------------------------------------------------------------------

/// Configuration for ensemble translation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsembleConfig {
    /// Maximum number of candidates to generate.
    pub max_candidates: usize,
    /// Provider/temperature combinations to try.
    pub candidate_configs: Vec<CandidateConfig>,
    /// Selection strategy for picking the winner.
    pub strategy: SelectionStrategy,
    /// Whether to run candidates in parallel (true) or sequential (false).
    pub parallel: bool,
    /// Maximum total budget for ensemble (in estimated USD).
    pub max_ensemble_cost_usd: f64,
}

impl Default for EnsembleConfig {
    fn default() -> Self {
        Self {
            max_candidates: 4,
            candidate_configs: vec![
                CandidateConfig {
                    provider: ProviderKind::Anthropic,
                    temperature: 0.3,
                    label: "claude-low".to_string(),
                },
                CandidateConfig {
                    provider: ProviderKind::DeepSeek,
                    temperature: 0.3,
                    label: "deepseek-low".to_string(),
                },
                CandidateConfig {
                    provider: ProviderKind::Anthropic,
                    temperature: 0.6,
                    label: "claude-high".to_string(),
                },
                CandidateConfig {
                    provider: ProviderKind::DeepSeek,
                    temperature: 0.6,
                    label: "deepseek-high".to_string(),
                },
            ],
            strategy: SelectionStrategy::BestScore,
            parallel: true,
            max_ensemble_cost_usd: 5.0,
        }
    }
}

/// Configuration for a single translation candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateConfig {
    /// Which LLM provider to use.
    pub provider: ProviderKind,
    /// Temperature for this candidate.
    pub temperature: f64,
    /// Human-readable label for logging.
    pub label: String,
}

/// Strategy for selecting the best candidate from the ensemble.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SelectionStrategy {
    /// Pick the candidate with the highest idiomatic score (among those that compile).
    BestScore,
    /// Pick the first candidate that compiles (fastest, cheapest).
    FirstCompiles,
    /// Tournament bracket: pair candidates, pick winner of each pair, recurse.
    Tournament,
}

/// A single translation candidate with its evaluation results.
#[derive(Debug, Clone)]
pub struct TranslationCandidate {
    /// Which configuration produced this candidate.
    pub config_label: String,
    /// Provider used.
    pub provider: ProviderKind,
    /// Temperature used.
    pub temperature: f64,
    /// The translated Rust source code.
    pub rust_source: String,
    /// Whether it compiles.
    pub compiles: bool,
    /// Compiler error output (empty if compiles).
    pub compiler_errors: Vec<String>,
    /// Idiomatic score (0-100).
    pub idiomatic_score: u32,
    /// Unsafe block count.
    pub unsafe_count: u32,
    /// Estimated cost in USD for this candidate.
    pub estimated_cost_usd: f64,
    /// Spec validation results (if Phase 3 available).
    pub specs_passed: Option<usize>,
    /// Spec validation total (if Phase 3 available).
    pub specs_total: Option<usize>,
}

/// Result of an ensemble translation run.
#[derive(Debug)]
pub struct EnsembleResult {
    /// The selected best candidate (None if all failed).
    pub winner: Option<TranslationCandidate>,
    /// All candidates that were generated (including failures).
    pub all_candidates: Vec<TranslationCandidate>,
    /// Total estimated cost for the entire ensemble.
    pub total_cost_usd: f64,
    /// Number of candidates that compiled.
    pub compiled_count: usize,
}

// ---------------------------------------------------------------------------
// Candidate evaluation (Task 2)
// ---------------------------------------------------------------------------

/// Evaluate a raw Rust translation and produce a graded TranslationCandidate.
///
/// Runs the three-gate filter:
/// 1. Compilation check
/// 2. Unsafe counting
/// 3. Idiomatic scoring
pub fn evaluate_candidate(
    rust_source: &str,
    c_source: &str,
    config_label: &str,
    provider: ProviderKind,
    temperature: f64,
    estimated_cost_usd: f64,
) -> TranslationCandidate {
    // Gate 1: Compilation
    let (compiles, compiler_errors) = match noricum_tools::compiler::check_rust_compiles(rust_source)
    {
        Ok(result) => {
            let errors: Vec<String> = if !result.success {
                result
                    .stderr
                    .lines()
                    .filter(|l| l.contains("error"))
                    .map(String::from)
                    .collect()
            } else {
                Vec::new()
            };
            (result.success, errors)
        }
        Err(e) => {
            debug!(error = %e, "compilation check failed for candidate {}", config_label);
            (false, vec![format!("Compilation check error: {e}")])
        }
    };

    // Gate 2: Unsafe counting
    let unsafe_count = noricum_tools::compiler::count_unsafe_blocks(rust_source);

    // Gate 3: Idiomatic scoring
    let idiomatic_score = noricum_validation::compute_idiomatic_score_from_source(
        unsafe_count,
        0, // clippy not run during ensemble (too slow)
        rust_source,
        c_source,
    );

    info!(
        label = config_label,
        compiles,
        unsafe_count,
        idiomatic_score,
        "evaluated ensemble candidate"
    );

    TranslationCandidate {
        config_label: config_label.to_string(),
        provider,
        temperature,
        rust_source: rust_source.to_string(),
        compiles,
        compiler_errors,
        idiomatic_score,
        unsafe_count,
        estimated_cost_usd,
        specs_passed: None,
        specs_total: None,
    }
}

/// Evaluate a candidate with optional spec validation (Phase 3 / P37 integration).
pub fn evaluate_candidate_with_specs(
    rust_source: &str,
    c_source: &str,
    config_label: &str,
    provider: ProviderKind,
    temperature: f64,
    estimated_cost_usd: f64,
    traces: Option<&[noricum_tools::spec_mining::FunctionTrace]>,
) -> TranslationCandidate {
    let mut candidate = evaluate_candidate(
        rust_source,
        c_source,
        config_label,
        provider,
        temperature,
        estimated_cost_usd,
    );

    // Optional Gate: Spec validation (only if compiles and traces available)
    if candidate.compiles {
        if let Some(traces) = traces {
            if !traces.is_empty() {
                match noricum_tools::spec_mining::validate_against_specs(rust_source, traces) {
                    Ok(spec_result) => {
                        candidate.specs_passed = Some(spec_result.passed);
                        candidate.specs_total = Some(spec_result.total_specs);
                        info!(
                            label = config_label,
                            specs_passed = spec_result.passed,
                            specs_total = spec_result.total_specs,
                            "P38: spec validation for ensemble candidate"
                        );
                    }
                    Err(e) => {
                        debug!(
                            label = config_label,
                            error = %e,
                            "P38: spec validation failed for ensemble candidate"
                        );
                    }
                }
            }
        }
    }

    candidate
}

// ---------------------------------------------------------------------------
// Selection strategies (Task 2)
// ---------------------------------------------------------------------------

/// Select the best candidate from a set of evaluated candidates.
pub fn select_best(
    candidates: &[TranslationCandidate],
    strategy: &SelectionStrategy,
) -> Option<TranslationCandidate> {
    if candidates.is_empty() {
        return None;
    }

    match strategy {
        SelectionStrategy::BestScore => select_best_score(candidates),
        SelectionStrategy::FirstCompiles => select_first_compiles(candidates),
        SelectionStrategy::Tournament => select_tournament(candidates),
    }
}

/// BestScore: highest idiomatic_score among compiling candidates.
/// Factors in spec pass ratio when available.
fn select_best_score(candidates: &[TranslationCandidate]) -> Option<TranslationCandidate> {
    let mut compiling: Vec<&TranslationCandidate> =
        candidates.iter().filter(|c| c.compiles).collect();

    if compiling.is_empty() {
        // Fallback: pick the one with highest score even if it doesn't compile
        let best = candidates.iter().max_by_key(|c| c.idiomatic_score)?;
        return Some(best.clone());
    }

    compiling.sort_by(|a, b| {
        // Prefer candidates with all specs passing
        let a_spec_ratio = a
            .specs_passed
            .zip(a.specs_total)
            .map(|(p, t)| if t > 0 { p as f64 / t as f64 } else { 0.5 })
            .unwrap_or(0.5);
        let b_spec_ratio = b
            .specs_passed
            .zip(b.specs_total)
            .map(|(p, t)| if t > 0 { p as f64 / t as f64 } else { 0.5 })
            .unwrap_or(0.5);

        b_spec_ratio
            .partial_cmp(&a_spec_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.idiomatic_score.cmp(&a.idiomatic_score))
            .then(a.unsafe_count.cmp(&b.unsafe_count))
    });

    compiling.first().cloned().cloned()
}

/// FirstCompiles: first candidate that compiles (preserves config ordering).
fn select_first_compiles(candidates: &[TranslationCandidate]) -> Option<TranslationCandidate> {
    candidates
        .iter()
        .find(|c| c.compiles)
        .cloned()
        .or_else(|| candidates.first().cloned())
}

/// Tournament: pair candidates, compare each pair, winner advances.
fn select_tournament(candidates: &[TranslationCandidate]) -> Option<TranslationCandidate> {
    if candidates.len() <= 1 {
        return candidates.first().cloned();
    }

    let mut round: Vec<TranslationCandidate> = candidates.to_vec();

    while round.len() > 1 {
        let mut next_round = Vec::new();
        let mut i = 0;
        while i < round.len() {
            if i + 1 < round.len() {
                let winner = compare_candidates(&round[i], &round[i + 1]);
                next_round.push(winner);
                i += 2;
            } else {
                next_round.push(round[i].clone());
                i += 1;
            }
        }
        round = next_round;
    }

    round.into_iter().next()
}

/// Compare two candidates: prefer compiling, then higher score, then fewer unsafe.
fn compare_candidates(a: &TranslationCandidate, b: &TranslationCandidate) -> TranslationCandidate {
    if a.compiles && !b.compiles {
        return a.clone();
    }
    if b.compiles && !a.compiles {
        return b.clone();
    }
    if a.idiomatic_score > b.idiomatic_score {
        return a.clone();
    }
    if b.idiomatic_score > a.idiomatic_score {
        return b.clone();
    }
    if a.unsafe_count < b.unsafe_count {
        return a.clone();
    }
    if b.unsafe_count < a.unsafe_count {
        return b.clone();
    }
    // Tiebreak: prefer cheaper candidate
    if a.estimated_cost_usd <= b.estimated_cost_usd {
        a.clone()
    } else {
        b.clone()
    }
}

// ---------------------------------------------------------------------------
// Cost estimation (Task 3 partial)
// ---------------------------------------------------------------------------

/// Estimate cost for a candidate based on token counts and provider pricing.
pub fn estimate_candidate_cost(
    input_tokens: u64,
    output_tokens: u64,
    provider: &ProviderKind,
) -> f64 {
    let (input_per_mtok, output_per_mtok) = match provider {
        ProviderKind::Anthropic => (3.0, 15.0),
        ProviderKind::DeepSeek => (0.14, 0.28),
        ProviderKind::Ollama => (0.0, 0.0),
    };
    (input_tokens as f64 / 1_000_000.0) * input_per_mtok
        + (output_tokens as f64 / 1_000_000.0) * output_per_mtok
}

// ---------------------------------------------------------------------------
// Parallel candidate generation (Task 3)
// ---------------------------------------------------------------------------

/// Generate translation candidates using the ensemble configuration.
///
/// For each `CandidateConfig`, creates an `LlmClient` for the specified provider,
/// translates the C source, and evaluates the result. Candidates are generated
/// concurrently when `config.parallel` is true.
pub async fn generate_candidates(
    c_source: &str,
    analysis: &crate::analysis::AnalysisResult,
    provider_config: &crate::providers::ProviderConfig,
    ensemble_config: &EnsembleConfig,
    difficulty: noricum_ir::Difficulty,
) -> EnsembleResult {
    let configs = &ensemble_config.candidate_configs;
    let max = ensemble_config.max_candidates.min(configs.len());

    info!(
        candidates = max,
        parallel = ensemble_config.parallel,
        strategy = ?ensemble_config.strategy,
        "P38: generating ensemble candidates"
    );

    let mut all_candidates: Vec<TranslationCandidate> = Vec::new();
    let mut total_cost: f64 = 0.0;

    if ensemble_config.parallel {
        let mut handles = Vec::new();

        for cc in configs.iter().take(max) {
            let c_source = c_source.to_string();
            let analysis = analysis.clone();
            let provider_config = provider_config.clone();
            let cc = cc.clone();
            let label = cc.label.clone();
            let difficulty = difficulty;

            let handle = tokio::spawn(async move {
                generate_single_candidate(&c_source, &analysis, &provider_config, &cc, difficulty)
                    .await
            });

            handles.push((label, handle));
        }

        for (label, handle) in handles {
            match handle.await {
                Ok(Ok(candidate)) => {
                    total_cost += candidate.estimated_cost_usd;
                    all_candidates.push(candidate);
                    if total_cost > ensemble_config.max_ensemble_cost_usd {
                        warn!(
                            total_cost,
                            budget = ensemble_config.max_ensemble_cost_usd,
                            "P38: ensemble budget exceeded, stopping"
                        );
                        break;
                    }
                }
                Ok(Err(e)) => {
                    warn!(label = %label, error = %e, "P38: candidate generation failed");
                }
                Err(e) => {
                    warn!(label = %label, error = %e, "P38: candidate task panicked");
                }
            }
        }
    } else {
        for cc in configs.iter().take(max) {
            if total_cost > ensemble_config.max_ensemble_cost_usd {
                warn!("P38: ensemble budget exceeded, skipping remaining candidates");
                break;
            }

            match generate_single_candidate(c_source, analysis, provider_config, cc, difficulty)
                .await
            {
                Ok(candidate) => {
                    total_cost += candidate.estimated_cost_usd;
                    all_candidates.push(candidate);
                }
                Err(e) => {
                    warn!(label = %cc.label, error = %e, "P38: candidate generation failed");
                }
            }
        }
    }

    let compiled_count = all_candidates.iter().filter(|c| c.compiles).count();
    let winner = select_best(&all_candidates, &ensemble_config.strategy);

    info!(
        total_candidates = all_candidates.len(),
        compiled_count,
        total_cost_usd = total_cost,
        winner_label = winner
            .as_ref()
            .map(|w| w.config_label.as_str())
            .unwrap_or("none"),
        winner_score = winner.as_ref().map(|w| w.idiomatic_score).unwrap_or(0),
        "P38: ensemble complete"
    );

    EnsembleResult {
        winner,
        all_candidates,
        total_cost_usd: total_cost,
        compiled_count,
    }
}

/// Generate a single translation candidate.
async fn generate_single_candidate(
    c_source: &str,
    analysis: &crate::analysis::AnalysisResult,
    provider_config: &crate::providers::ProviderConfig,
    cc: &CandidateConfig,
    difficulty: noricum_ir::Difficulty,
) -> Result<TranslationCandidate, crate::AgentError> {
    let mut pc = provider_config.clone();
    pc.primary_provider = match cc.provider {
        ProviderKind::Anthropic => "anthropic".to_string(),
        ProviderKind::DeepSeek => "deepseek".to_string(),
        ProviderKind::Ollama => "ollama".to_string(),
    };

    let client = crate::providers::create_llm_client(&pc)?;
    let model_sel = crate::providers::select_model(&pc, difficulty, "ensemble")?;

    info!(
        label = %cc.label,
        provider = ?cc.provider,
        model = %model_sel.model,
        temperature = cc.temperature,
        "P38: generating candidate"
    );

    let rust_source = crate::translation::translate_function_with_patterns_and_temperature(
        &client,
        &model_sel.model,
        c_source,
        None,
        analysis,
        &[],
        Some(cc.temperature),
    )
    .await?;

    let input_tokens = crate::estimate_tokens(c_source);
    let output_tokens = crate::estimate_tokens(&rust_source);
    let cost = estimate_candidate_cost(input_tokens, output_tokens, &cc.provider);

    Ok(evaluate_candidate(
        &rust_source,
        c_source,
        &cc.label,
        cc.provider.clone(),
        cc.temperature,
        cost,
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Task 1: Config types ---

    #[test]
    fn test_default_ensemble_config() {
        let config = EnsembleConfig::default();
        assert_eq!(config.max_candidates, 4);
        assert_eq!(config.candidate_configs.len(), 4);
        assert_eq!(config.strategy, SelectionStrategy::BestScore);
        assert!(config.parallel);
        assert!(config.max_ensemble_cost_usd > 0.0);
    }

    #[test]
    fn test_candidate_config_labels() {
        let config = EnsembleConfig::default();
        let labels: Vec<&str> = config
            .candidate_configs
            .iter()
            .map(|c| c.label.as_str())
            .collect();
        assert!(labels.contains(&"claude-low"));
        assert!(labels.contains(&"deepseek-low"));
        assert!(labels.contains(&"claude-high"));
        assert!(labels.contains(&"deepseek-high"));
    }

    #[test]
    fn test_selection_strategy_variants() {
        let _a = SelectionStrategy::BestScore;
        let _b = SelectionStrategy::FirstCompiles;
        let _c = SelectionStrategy::Tournament;
    }

    #[test]
    fn test_ensemble_config_serialization() {
        let config = EnsembleConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: EnsembleConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_candidates, config.max_candidates);
        assert_eq!(parsed.strategy, config.strategy);
    }

    // --- Task 2: Evaluation ---

    #[test]
    fn test_evaluate_candidate_compiling() {
        let rust = "pub fn add(a: i32, b: i32) -> i32 { a + b }";
        let c = "int add(int a, int b) { return a + b; }";
        let candidate = evaluate_candidate(rust, c, "test", ProviderKind::Anthropic, 0.3, 0.01);
        assert!(candidate.compiles, "valid Rust should compile");
        assert_eq!(candidate.unsafe_count, 0);
        assert!(candidate.idiomatic_score > 0);
    }

    #[test]
    fn test_evaluate_candidate_non_compiling() {
        let rust = "fn bad( { }";
        let c = "int f() { return 0; }";
        let candidate = evaluate_candidate(rust, c, "test", ProviderKind::DeepSeek, 0.6, 0.02);
        assert!(!candidate.compiles);
        assert!(!candidate.compiler_errors.is_empty());
    }

    // --- Task 2: Selection ---

    #[test]
    fn test_select_best_score_prefers_compiling() {
        let candidates = vec![
            mock_candidate("a", false, 95, 0, 0.01),
            mock_candidate("b", true, 70, 0, 0.02),
        ];
        let winner = select_best(&candidates, &SelectionStrategy::BestScore).unwrap();
        assert_eq!(
            winner.config_label, "b",
            "compiling candidate should win even with lower score"
        );
    }

    #[test]
    fn test_select_best_score_highest_among_compiling() {
        let candidates = vec![
            mock_candidate("a", true, 80, 0, 0.01),
            mock_candidate("b", true, 95, 0, 0.02),
        ];
        let winner = select_best(&candidates, &SelectionStrategy::BestScore).unwrap();
        assert_eq!(
            winner.config_label, "b",
            "higher score should win among compiling"
        );
    }

    #[test]
    fn test_select_first_compiles() {
        let candidates = vec![
            mock_candidate("first-fail", false, 90, 0, 0.01),
            mock_candidate("second-ok", true, 60, 0, 0.02),
            mock_candidate("third-ok", true, 95, 0, 0.03),
        ];
        let winner = select_best(&candidates, &SelectionStrategy::FirstCompiles).unwrap();
        assert_eq!(
            winner.config_label, "second-ok",
            "FirstCompiles should pick first compiling"
        );
    }

    #[test]
    fn test_select_tournament() {
        let candidates = vec![
            mock_candidate("a", true, 70, 2, 0.01),
            mock_candidate("b", true, 85, 0, 0.02),
            mock_candidate("c", true, 90, 1, 0.03),
        ];
        let winner = select_best(&candidates, &SelectionStrategy::Tournament).unwrap();
        assert_eq!(
            winner.config_label, "c",
            "tournament should pick highest score"
        );
    }

    #[test]
    fn test_select_empty_candidates() {
        let result = select_best(&[], &SelectionStrategy::BestScore);
        assert!(result.is_none());
    }

    #[test]
    fn test_select_all_fail_returns_best_failing() {
        let candidates = vec![
            mock_candidate("a", false, 50, 0, 0.01),
            mock_candidate("b", false, 70, 0, 0.02),
        ];
        let winner = select_best(&candidates, &SelectionStrategy::BestScore).unwrap();
        assert_eq!(
            winner.config_label, "b",
            "should return best-scoring non-compiling as fallback"
        );
    }

    #[test]
    fn test_compare_candidates_tiebreak_unsafe() {
        let a = mock_candidate("a", true, 80, 3, 0.01);
        let b = mock_candidate("b", true, 80, 1, 0.02);
        let winner = compare_candidates(&a, &b);
        assert_eq!(
            winner.config_label, "b",
            "fewer unsafe should win on tiebreak"
        );
    }

    // --- Task 3: Cost estimation ---

    #[test]
    fn test_estimate_candidate_cost_anthropic() {
        let cost = estimate_candidate_cost(1_000_000, 500_000, &ProviderKind::Anthropic);
        assert!(
            (cost - 10.5).abs() < 0.01,
            "expected ~$10.50, got ${cost}"
        );
    }

    #[test]
    fn test_estimate_candidate_cost_deepseek() {
        let cost = estimate_candidate_cost(1_000_000, 500_000, &ProviderKind::DeepSeek);
        assert!((cost - 0.28).abs() < 0.01, "expected ~$0.28, got ${cost}");
    }

    #[test]
    fn test_estimate_candidate_cost_ollama_free() {
        let cost = estimate_candidate_cost(1_000_000, 500_000, &ProviderKind::Ollama);
        assert_eq!(cost, 0.0, "Ollama should be free");
    }

    // --- Task 7: Spec-aware selection ---

    #[test]
    fn test_evaluate_candidate_with_specs() {
        let rust = "pub fn add(a: i32, b: i32) -> i32 { a + b }";
        let c = "int add(int a, int b) { return a + b; }";
        let traces = vec![noricum_tools::spec_mining::FunctionTrace {
            function_name: "add".to_string(),
            inputs: vec![
                noricum_tools::spec_mining::TraceValue {
                    c_type: "int".to_string(),
                    value: "2".to_string(),
                },
                noricum_tools::spec_mining::TraceValue {
                    c_type: "int".to_string(),
                    value: "3".to_string(),
                },
            ],
            output: Some(noricum_tools::spec_mining::TraceValue {
                c_type: "int".to_string(),
                value: "5".to_string(),
            }),
            call_index: 0,
        }];
        let candidate = evaluate_candidate_with_specs(
            rust,
            c,
            "test",
            ProviderKind::Anthropic,
            0.3,
            0.01,
            Some(&traces),
        );
        assert!(candidate.compiles);
        assert_eq!(candidate.specs_passed, Some(1));
        assert_eq!(candidate.specs_total, Some(1));
    }

    #[test]
    fn test_select_best_prefers_spec_passing() {
        let mut no_specs = mock_candidate("no-specs", true, 90, 0, 0.01);
        no_specs.specs_passed = Some(0);
        no_specs.specs_total = Some(3);

        let mut all_specs = mock_candidate("all-specs", true, 80, 0, 0.02);
        all_specs.specs_passed = Some(3);
        all_specs.specs_total = Some(3);

        let candidates = vec![no_specs, all_specs];
        let winner = select_best(&candidates, &SelectionStrategy::BestScore).unwrap();
        assert_eq!(
            winner.config_label, "all-specs",
            "spec-passing candidate should win even with lower score"
        );
    }

    // --- Task 8: Mock tests ---

    fn mock_candidate(
        label: &str,
        compiles: bool,
        score: u32,
        unsafe_count: u32,
        cost: f64,
    ) -> TranslationCandidate {
        TranslationCandidate {
            config_label: label.to_string(),
            provider: ProviderKind::Anthropic,
            temperature: 0.3,
            rust_source: if compiles {
                "pub fn f() -> i32 { 42 }".to_string()
            } else {
                "fn f( { invalid".to_string()
            },
            compiles,
            compiler_errors: if compiles {
                vec![]
            } else {
                vec!["error[E0308]".to_string()]
            },
            idiomatic_score: score,
            unsafe_count,
            estimated_cost_usd: cost,
            specs_passed: None,
            specs_total: None,
        }
    }

    #[test]
    fn test_ensemble_selection_with_4_candidates() {
        let candidates = vec![
            mock_candidate("claude-low", true, 75, 0, 0.05),
            mock_candidate("deepseek-low", false, 80, 0, 0.01),
            mock_candidate("claude-high", true, 92, 1, 0.05),
            mock_candidate("deepseek-high", true, 88, 0, 0.01),
        ];

        let winner = select_best(&candidates, &SelectionStrategy::BestScore).unwrap();
        assert_eq!(winner.config_label, "claude-high");

        let winner = select_best(&candidates, &SelectionStrategy::FirstCompiles).unwrap();
        assert_eq!(winner.config_label, "claude-low");

        let winner = select_best(&candidates, &SelectionStrategy::Tournament).unwrap();
        assert_eq!(winner.config_label, "claude-high");
    }

    #[test]
    fn test_ensemble_cost_budget_enforcement() {
        let config = EnsembleConfig {
            max_ensemble_cost_usd: 0.10,
            ..EnsembleConfig::default()
        };
        let candidates = vec![
            mock_candidate("a", true, 80, 0, 0.05),
            mock_candidate("b", true, 85, 0, 0.06),
            mock_candidate("c", true, 90, 0, 0.07),
        ];
        let total_cost: f64 = candidates.iter().map(|c| c.estimated_cost_usd).sum();
        assert!(
            total_cost > config.max_ensemble_cost_usd,
            "should exceed budget"
        );
    }

    #[test]
    fn test_ensemble_result_tracking() {
        let candidates = vec![
            mock_candidate("a", false, 40, 0, 0.01),
            mock_candidate("b", true, 85, 0, 0.02),
            mock_candidate("c", true, 90, 1, 0.03),
        ];

        let compiled_count = candidates.iter().filter(|c| c.compiles).count();
        assert_eq!(compiled_count, 2);

        let total_cost: f64 = candidates.iter().map(|c| c.estimated_cost_usd).sum();
        assert!((total_cost - 0.06).abs() < 0.001);

        let result = EnsembleResult {
            winner: select_best(&candidates, &SelectionStrategy::BestScore),
            all_candidates: candidates,
            total_cost_usd: total_cost,
            compiled_count,
        };

        assert!(result.winner.is_some());
        assert_eq!(result.winner.unwrap().config_label, "c");
        assert_eq!(result.compiled_count, 2);
    }
}

// ---------------------------------------------------------------------------
// Integration tests (Task 9)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_evaluate_real_translation_good() {
        let rust = r#"
pub fn hash_key(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for c in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(c as u64);
    }
    hash
}
"#;
        let c = r#"
unsigned long hash_key(const char *str) {
    unsigned long hash = 5381;
    int c;
    while ((c = *str++))
        hash = ((hash << 5) + hash) + c;
    return hash;
}
"#;
        let candidate =
            evaluate_candidate(rust, c, "claude-good", ProviderKind::Anthropic, 0.3, 0.05);
        assert!(candidate.compiles, "good translation should compile");
        assert_eq!(candidate.unsafe_count, 0, "should have no unsafe");
        assert!(
            candidate.idiomatic_score >= 70,
            "should score well, got {}",
            candidate.idiomatic_score
        );
    }

    #[test]
    fn test_evaluate_real_translation_bad() {
        let rust = r#"
pub fn hash_key(s: *const u8) -> u64 {
    unsafe {
        let mut hash = 5381u64;
        let mut p = s;
        while *p != 0 {
            hash = hash << 5 + hash + *p as u64;
            p = p.offset(1);
        }
        hash
    }
"#; // Missing closing brace
        let c = "unsigned long hash_key(const char *str) { return 0; }";
        let candidate =
            evaluate_candidate(rust, c, "deepseek-bad", ProviderKind::DeepSeek, 0.6, 0.01);
        assert!(!candidate.compiles, "bad translation should not compile");
    }

    #[test]
    fn test_ensemble_selection_realistic_mix() {
        let good_rust = "pub fn hash_key(s: &str) -> u64 {\n    let mut hash: u64 = 5381;\n    for c in s.bytes() {\n        hash = hash.wrapping_mul(33).wrapping_add(c as u64);\n    }\n    hash\n}";
        let mid_rust = "pub fn hash_key(s: &str) -> u64 {\n    let mut hash = 5381u64;\n    for b in s.as_bytes() {\n        hash = ((hash << 5) + hash) + *b as u64;\n    }\n    hash\n}";
        let bad_rust = "fn hash_key(s: &str -> u64 { 0 }";

        let c = "unsigned long hash_key(const char *str) { unsigned long hash = 5381; int c; while ((c = *str++)) hash = ((hash << 5) + hash) + c; return hash; }";

        let candidates = vec![
            evaluate_candidate(good_rust, c, "claude-0.3", ProviderKind::Anthropic, 0.3, 0.05),
            evaluate_candidate(mid_rust, c, "deepseek-0.3", ProviderKind::DeepSeek, 0.3, 0.01),
            evaluate_candidate(bad_rust, c, "deepseek-0.6", ProviderKind::DeepSeek, 0.6, 0.01),
        ];

        let compiled_count = candidates.iter().filter(|c| c.compiles).count();
        assert!(
            compiled_count >= 2,
            "at least 2 should compile, got {compiled_count}"
        );

        let winner = select_best(&candidates, &SelectionStrategy::BestScore).unwrap();
        assert!(winner.compiles, "winner must compile");
        assert!(
            winner.idiomatic_score >= 70,
            "winner should have decent score"
        );
    }
}
