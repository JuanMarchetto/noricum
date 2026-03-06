/// Migration orchestrator: drives the per-function state machine.
///
/// The orchestrator takes a C source file through the full pipeline:
/// Pending -> Extracted -> Characterized -> C2RustDone -> Analyzed -> Refined -> Validated
///
/// Two execution modes:
/// - `migrate_file_sync`: deterministic, no LLM (C2Rust + validation only)
/// - `migrate_file`: async, full LLM agent pipeline (analysis, translation, repair, test gen)
use std::path::Path;

use noricum_agents::LlmClient;
use noricum_agents::providers::{ProviderConfig, create_llm_client, select_model};
use noricum_ir::pattern_store::PatternStore;
use noricum_ir::{FunctionUnit, MigrationProject, MigrationState};
use std::time::Instant;
use tracing::{debug, info, warn};

/// Maximum input C source file size: 5 MB.
/// Prevents OOM on extremely large C files.
const MAX_C_SOURCE_SIZE: usize = 5 * 1024 * 1024;

/// LOC threshold above which repair iterations are reduced to save tokens.
const LARGE_FILE_LOC: usize = 1000;
/// LOC threshold above which repair iterations are further reduced.
const VERY_LARGE_FILE_LOC: usize = 2000;

use crate::CoreError;
use crate::audit::{AuditEvent, SharedAuditTrail, audit_log, create_shared_audit};

/// Configuration for the async LLM-based migration pipeline.
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Anthropic API key. Defaults to `ANTHROPIC_API_KEY` env var.
    pub anthropic_api_key: Option<String>,
    /// Ollama URL override. If `None`, uses the default `http://localhost:11434`.
    pub ollama_url: Option<String>,
    /// Ollama model name override. Defaults to "llama3.2".
    pub ollama_model: Option<String>,
    /// Maximum number of repair iterations before falling back to unsafe.
    pub max_repair_iterations: u32,
    /// Minimum idiomatic score (0-100) required to pass validation.
    pub min_idiomatic_score: u32,
    /// Whether to generate equivalence tests after successful migration.
    pub generate_tests: bool,
    /// Path for audit log output (JSON-lines format).
    pub audit_log: Option<std::path::PathBuf>,
    /// Audit logging detail level.
    pub audit_level: crate::audit::AuditLevel,
    /// Whether to run fuzz testing after diff test passes.
    pub fuzz_test: bool,
    /// Number of fuzz test iterations.
    pub fuzz_iterations: u32,
    /// Whether to run the C preprocessor before analysis.
    pub preprocess: bool,
    /// Whether to generate doc comments on migrated Rust functions.
    pub generate_docs: bool,
    /// Temperature for analysis agent (default: 0.2).
    pub analysis_temperature: Option<f64>,
    /// Temperature for translation agent (default: 0.3).
    pub translation_temperature: Option<f64>,
    /// Base temperature for repair agent (default: 0.4).
    pub repair_base_temperature: Option<f64>,
    /// Temperature for test generation agent (default: 0.3).
    pub test_gen_temperature: Option<f64>,
    /// Maximum total token budget (input + output) per run.
    /// Defaults to 500,000 tokens. Set to `None` for unlimited (not recommended in production).
    pub max_tokens_budget: Option<u64>,
    /// Maximum number of LLM API calls per run. Prevents runaway loops.
    /// Defaults to 20. Set to `None` for unlimited.
    pub max_llm_calls: Option<u32>,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            ollama_url: None,
            ollama_model: None,
            max_repair_iterations: 5,
            min_idiomatic_score: 60,
            generate_tests: true,
            audit_log: None,
            audit_level: crate::audit::AuditLevel::Summary,
            fuzz_test: false,
            fuzz_iterations: 100,
            preprocess: false,
            generate_docs: false,
            analysis_temperature: None,
            translation_temperature: None,
            repair_base_temperature: None,
            test_gen_temperature: None,
            max_tokens_budget: Some(500_000),
            max_llm_calls: Some(20),
        }
    }
}

impl From<&MigrationConfig> for ProviderConfig {
    fn from(config: &MigrationConfig) -> Self {
        Self {
            anthropic_api_key: config.anthropic_api_key.clone(),
            ollama_url: config
                .ollama_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434".to_string()),
            ollama_model: config
                .ollama_model
                .clone()
                .unwrap_or_else(|| "llama3.2".to_string()),
        }
    }
}

impl MigrationConfig {
    /// Try to create an LLM client (Anthropic preferred, Ollama fallback).
    fn create_client(&self) -> Option<LlmClient> {
        let provider_config = ProviderConfig::from(self);
        match create_llm_client(&provider_config) {
            Ok(client) => Some(client),
            Err(e) => {
                warn!(error = %e, "no LLM client available");
                None
            }
        }
    }
}

/// Check whether the accumulated token usage exceeds the configured budget.
///
/// Returns `Ok(())` if within budget or no budget is set, otherwise
/// returns `CoreError::BudgetExceeded`.
fn check_budget(
    config: &MigrationConfig,
    metrics: &noricum_ir::MigrationMetrics,
) -> Result<(), CoreError> {
    if let Some(budget) = config.max_tokens_budget {
        let used = metrics.input_tokens + metrics.output_tokens;
        if used > budget {
            return Err(CoreError::BudgetExceeded { used, budget });
        }
    }
    if let Some(max_calls) = config.max_llm_calls
        && metrics.llm_calls > max_calls
    {
        return Err(CoreError::Orchestration(format!(
            "LLM call limit exceeded: {} calls (max {})",
            metrics.llm_calls, max_calls
        )));
    }
    Ok(())
}

/// Estimate total token cost for migrating a file before starting.
///
/// Returns (estimated_tokens, estimated_usd). Logs a warning if the estimate
/// exceeds 80% of the configured budget.
fn preflight_budget_check(config: &MigrationConfig, c_source: &str, name: &str) {
    let c_tokens = noricum_agents::estimate_tokens(c_source);
    // Analysis: C source in + analysis out (~500 tokens)
    let analysis_cost = c_tokens + 500;
    // Translation: C source + analysis context in, Rust (~1.5x) out
    let translation_cost = c_tokens * 2 + c_tokens * 3 / 2;
    // Repair (per iter): Rust source + C source + errors in, Rust out
    let repair_per_iter = c_tokens * 4;
    let c_lines = c_source.lines().count();
    let effective_iters = effective_repair_iterations(config.max_repair_iterations, c_lines);
    let repair_cost = repair_per_iter * effective_iters as u64;
    // Test gen: C + Rust in, tests out
    let test_gen_cost = c_tokens * 3;

    let total_estimate = analysis_cost + translation_cost + repair_cost + test_gen_cost;

    info!(
        function = %name,
        estimated_tokens = total_estimate,
        c_lines,
        effective_repair_iters = effective_iters,
        "pre-flight budget estimate"
    );

    if let Some(budget) = config.max_tokens_budget {
        let threshold = budget * 80 / 100;
        if total_estimate > threshold {
            warn!(
                function = %name,
                estimated = total_estimate,
                budget,
                "estimated token usage exceeds 80% of budget"
            );
        }
    }
}

/// Compute effective max repair iterations based on file size.
///
/// Large files use fewer iterations to conserve tokens — each repair
/// iteration sends the full Rust + C source, which is expensive.
fn effective_repair_iterations(configured_max: u32, c_lines: usize) -> u32 {
    if c_lines > VERY_LARGE_FILE_LOC {
        configured_max.min(2)
    } else if c_lines > LARGE_FILE_LOC {
        configured_max.min(3)
    } else {
        configured_max
    }
}

/// Simple file-based translation cache.
///
/// Caches successful translations keyed by a hash of the C source content.
/// Cache directory: `.noricum-cache/` in the current working directory.
mod cache {
    use std::path::PathBuf;
    use tracing::debug;

    fn cache_dir() -> PathBuf {
        PathBuf::from(".noricum-cache")
    }

    fn cache_key(c_source: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        c_source.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Look up a cached translation for the given C source.
    pub fn get(c_source: &str) -> Option<String> {
        let key = cache_key(c_source);
        let path = cache_dir().join(format!("{key}.rs"));
        match std::fs::read_to_string(&path) {
            Ok(cached) if !cached.is_empty() => {
                debug!(cache_key = %key, "translation cache hit");
                Some(cached)
            }
            _ => None,
        }
    }

    /// Store a successful translation in the cache.
    pub fn put(c_source: &str, rust_source: &str) {
        let key = cache_key(c_source);
        let dir = cache_dir();
        if std::fs::create_dir_all(&dir).is_ok() {
            let path = dir.join(format!("{key}.rs"));
            if let Err(e) = std::fs::write(&path, rust_source) {
                debug!(error = %e, "failed to write translation cache");
            } else {
                debug!(cache_key = %key, path = %path.display(), "cached translation");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Synchronous pipeline (backward compat, no LLM)
// ---------------------------------------------------------------------------

/// Run the migration pipeline on a single C file (synchronous, no LLM).
///
/// This is the original v0 pipeline:
/// 1. Read C source
/// 2. Attempt C2Rust transpilation
/// 3. Validate result
///
/// Returns the updated FunctionUnit with migration state.
pub fn migrate_file_sync(c_file: &Path) -> Result<FunctionUnit, CoreError> {
    let source_path = c_file.to_string_lossy().to_string();
    let c_source = std::fs::read_to_string(c_file)?;

    if c_source.len() > MAX_C_SOURCE_SIZE {
        return Err(CoreError::Orchestration(format!(
            "source file exceeds maximum size of {} bytes ({} bytes)",
            MAX_C_SOURCE_SIZE,
            c_source.len()
        )));
    }

    let name = c_file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    info!(function = %name, file = %source_path, "starting sync migration (no LLM)");

    let mut unit = FunctionUnit::new(name, source_path, c_source);
    unit.state = MigrationState::Extracted;

    // Step 1: Try C2Rust transpilation
    match noricum_tools::c2rust::transpile(c_file) {
        Ok(output) => {
            unit.c2rust_output = Some(output.rust_source.clone());
            unit.rust_output = Some(output.rust_source);
            unit.state = MigrationState::C2RustDone;
            info!(function = %unit.name, "c2rust transpilation succeeded");
        }
        Err(e) => {
            debug!(function = %unit.name, error = %e, "c2rust not available, trying rule-based translation");

            // Fallback: try rule-based translation for simple functions
            match noricum_tools::rule_translate::try_translate(&unit.c_source, &unit.name) {
                Some(rust_code) => {
                    unit.rust_output = Some(rust_code);
                    unit.state = MigrationState::Refined;
                    info!(function = %unit.name, "rule-based translation succeeded");
                }
                None => {
                    info!(function = %unit.name, "function too complex for rule-based translation, needs LLM");
                    unit.state = MigrationState::Extracted;
                    return Ok(unit);
                }
            }
        }
    }

    // Step 2: Validate
    let validation = noricum_validation::validate(&unit)?;
    noricum_validation::apply_validation(&mut unit, &validation);

    info!(
        function = %unit.name,
        state = ?unit.state,
        score = ?unit.idiomatic_score,
        "sync migration complete"
    );

    Ok(unit)
}

/// Run sync migration on all C files in a directory (no LLM).
pub fn migrate_directory_sync(dir: &Path) -> Result<MigrationProject, CoreError> {
    let dir_str = dir.to_string_lossy().to_string();
    let mut project = MigrationProject::new(
        dir.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string()),
        dir_str.clone(),
    );

    let mut found_any = false;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "c") {
            found_any = true;
            let unit = migrate_file_sync(&path)?;
            project.add_unit(unit);
        }
    }

    if !found_any {
        return Err(CoreError::NoSourceFiles(dir_str));
    }

    let summary = project.progress_summary();
    info!(
        total = summary.total,
        validated = summary.validated,
        failed = summary.failed,
        "directory sync migration complete"
    );

    Ok(project)
}

// ---------------------------------------------------------------------------
// Async pipeline (full LLM agents)
// ---------------------------------------------------------------------------

/// Run the full async LLM migration pipeline on a single C file.
///
/// Pipeline stages:
/// 1. Read C source, create FunctionUnit -> Extracted
/// 2. Classify difficulty with router
/// 3. Try C2Rust transpilation -> C2RustDone (or skip)
/// 4. Call analysis agent -> Analyzed
/// 5. Call translation agent -> Refined
/// 6. Validate (compile check + idiomatic score) -> Validated or Repairing
/// 7. If Repairing: call repair agent (max N iterations), re-validate each time
/// 8. After max retries: FallbackUnsafe (keep C2Rust output)
/// 9. If Validated and configured: call test_gen agent
///
/// Falls back to sync pipeline if no LLM provider is available.
pub async fn migrate_file(
    c_file: &Path,
    config: &MigrationConfig,
) -> Result<FunctionUnit, CoreError> {
    let source_path = c_file.to_string_lossy().to_string();
    let c_source = std::fs::read_to_string(c_file)?;

    if c_source.len() > MAX_C_SOURCE_SIZE {
        return Err(CoreError::Orchestration(format!(
            "source file exceeds maximum size of {} bytes ({} bytes)",
            MAX_C_SOURCE_SIZE,
            c_source.len()
        )));
    }

    let name = c_file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    info!(function = %name, file = %source_path, "starting async migration");
    let pipeline_start = Instant::now();

    // --- Initialize audit trail ---
    let audit: Option<SharedAuditTrail> = config.audit_log.as_ref().and_then(|path| {
        match create_shared_audit(path, config.audit_level) {
            Ok(trail) => Some(trail),
            Err(e) => {
                warn!(error = %e, "failed to create audit trail, continuing without");
                None
            }
        }
    });

    if let Some(ref trail) = audit {
        audit_log(
            trail,
            AuditEvent::PipelineStart {
                function_name: name.clone(),
                source_file: source_path.clone(),
                c_lines: c_source.lines().count() as u32,
            },
        );
    }

    // Try to get an Anthropic client; fall back to sync if unavailable
    let client = match config.create_client() {
        Some(c) => c,
        None => {
            warn!(
                function = %name,
                "no LLM provider available, falling back to sync migration"
            );
            return migrate_file_sync(c_file);
        }
    };

    let provider_config = ProviderConfig::from(config);

    // --- Pre-flight budget estimate ---
    preflight_budget_check(config, &c_source, &name);

    let mut unit = FunctionUnit::new(name.clone(), source_path, c_source);
    unit.state = MigrationState::Extracted;
    info!(function = %name, state = ?unit.state, "state -> Extracted");

    // --- Preprocessor step ---
    if config.preprocess {
        let pp_config = noricum_tools::preprocessor::PreprocessorConfig::default();
        match noricum_tools::preprocessor::preprocess_file(c_file, &pp_config) {
            Ok(pp) if pp.was_preprocessed => {
                info!(function = %name, "preprocessor expanded source");
                unit.preprocessed_source = Some(pp.source);
            }
            Ok(_) => {
                debug!(function = %name, "preprocessor not available, using original source");
            }
            Err(e) => {
                debug!(function = %name, error = %e, "preprocessor failed, using original source");
            }
        }
    }

    // --- Stage 2: Classify difficulty ---
    let analysis_source = unit
        .preprocessed_source
        .as_deref()
        .unwrap_or(&unit.c_source);
    let difficulty = crate::router::classify_difficulty(analysis_source);
    unit.difficulty = Some(difficulty);
    info!(function = %name, ?difficulty, "difficulty classified");

    if let Some(ref trail) = audit {
        audit_log(
            trail,
            AuditEvent::DifficultyClassified {
                function_name: name.clone(),
                difficulty: format!("{difficulty:?}"),
            },
        );
    }

    // --- Stage 3: Try C2Rust transpilation ---
    match noricum_tools::c2rust::transpile(c_file) {
        Ok(output) => {
            let rust_src = output.rust_source;
            unit.c2rust_output = Some(rust_src.clone());
            unit.rust_output = Some(rust_src);
            unit.state = MigrationState::C2RustDone;
            info!(function = %name, state = ?unit.state, "state -> C2RustDone");
        }
        Err(e) => {
            debug!(function = %name, error = %e, "c2rust not available, will translate from scratch");
        }
    }

    // --- Stage 4: Analysis agent ---
    let analysis_start = Instant::now();
    let analysis_model_sel = select_model(&provider_config, difficulty, "analysis")?;
    info!(
        function = %name,
        model = %analysis_model_sel.model,
        provider = ?analysis_model_sel.provider,
        "calling analysis agent"
    );
    let analysis = match noricum_agents::analysis::analyze_function_with_temperature(
        &client,
        &analysis_model_sel.model,
        &unit.c_source,
        &name,
        config.analysis_temperature,
    )
    .await
    {
        Ok(result) => result,
        Err(e) => {
            warn!(function = %name, error = %e, "analysis agent failed, falling back to sync");
            return migrate_file_sync(c_file);
        }
    };
    unit.state = MigrationState::Analyzed;
    unit.metrics.analysis_ms = analysis_start.elapsed().as_millis() as u64;
    unit.metrics.llm_calls += 1;
    // Estimate token usage for analysis call
    unit.metrics.input_tokens += noricum_agents::estimate_tokens(&unit.c_source);
    unit.metrics.output_tokens += noricum_agents::estimate_tokens(&format!("{analysis:?}"));
    info!(
        function = %name,
        state = ?unit.state,
        patterns = ?analysis.patterns,
        strategy = %analysis.strategy,
        analysis_ms = unit.metrics.analysis_ms,
        "state -> Analyzed"
    );
    check_budget(config, &unit.metrics)?;

    if let Some(ref trail) = audit {
        audit_log(
            trail,
            AuditEvent::StateTransition {
                function_name: name.clone(),
                from: "Extracted".to_string(),
                to: "Analyzed".to_string(),
            },
        );
    }

    // --- Translation cache check ---
    if let Some(cached_rust) = cache::get(&unit.c_source) {
        info!(function = %name, "using cached translation, skipping LLM call");
        unit.rust_output = Some(cached_rust);
        unit.state = MigrationState::Refined;
        // Skip to validation (no LLM cost)
    }

    // --- Stage 5: Translation agent (with RAG pattern context) ---
    if unit.state != MigrationState::Refined {
        let pattern_store = PatternStore::load_seed_patterns();
        let relevant_patterns = pattern_store.find_relevant(&unit.c_source, 3);
        if !relevant_patterns.is_empty() {
            info!(
                function = %name,
                patterns = relevant_patterns.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "),
                "injecting RAG patterns into translation prompt"
            );
        }

        let translation_start = Instant::now();
        let translation_model_sel = select_model(&provider_config, difficulty, "translation")?;
        info!(
            function = %name,
            model = %translation_model_sel.model,
            "calling translation agent"
        );
        let rust_code =
            match noricum_agents::translation::translate_function_with_patterns_and_temperature(
                &client,
                &translation_model_sel.model,
                &unit.c_source,
                unit.c2rust_output.as_deref(),
                &analysis,
                &relevant_patterns,
                config.translation_temperature,
            )
            .await
            {
                Ok(code) => code,
                Err(e) => {
                    warn!(function = %name, error = %e, "translation agent failed, falling back to sync");
                    return migrate_file_sync(c_file);
                }
            };
        unit.rust_output = Some(rust_code);
        unit.state = MigrationState::Refined;
        unit.metrics.translation_ms = translation_start.elapsed().as_millis() as u64;
        unit.metrics.llm_calls += 1;
        // Estimate token usage for translation call
        unit.metrics.input_tokens += noricum_agents::estimate_tokens(&unit.c_source);
        if let Some(ref rust) = unit.rust_output {
            unit.metrics.output_tokens += noricum_agents::estimate_tokens(rust);
        }
        info!(function = %name, state = ?unit.state, translation_ms = unit.metrics.translation_ms, "state -> Refined");
        check_budget(config, &unit.metrics)?;
    }

    // --- Stage 6: Validate ---
    let c_lines = unit.c_source.lines().count();
    let max_iters = effective_repair_iterations(config.max_repair_iterations, c_lines);

    let validation =
        noricum_validation::validate_with_threshold(&unit, config.min_idiomatic_score)?;
    noricum_validation::apply_validation_with_max(&mut unit, &validation, max_iters);
    info!(
        function = %name,
        state = ?unit.state,
        compiles = validation.compiles,
        idiomatic_score = validation.idiomatic_score,
        unsafe_count = validation.unsafe_count,
        diff_test = ?validation.diff_test_passed,
        "validation result"
    );

    if let Some(ref trail) = audit {
        audit_log(
            trail,
            AuditEvent::ValidationResult {
                function_name: name.clone(),
                compiles: validation.compiles,
                idiomatic_score: validation.idiomatic_score,
                unsafe_count: validation.unsafe_count,
                diff_test_passed: validation.diff_test_passed,
                passed: validation.passed,
            },
        );
    }

    // --- Stage 7: Repair loop (token-aware iteration limit) ---
    if !validation.passed {
        let repair_start = Instant::now();
        let repair_model_sel = select_model(&provider_config, difficulty, "repair")?;

        if max_iters < config.max_repair_iterations {
            info!(
                function = %name,
                c_lines,
                configured = config.max_repair_iterations,
                effective = max_iters,
                "reducing repair iterations for large file"
            );
        }

        let mut iteration = 1u32;
        while iteration <= max_iters {
            info!(
                function = %name,
                iteration,
                max = max_iters,
                model = %repair_model_sel.model,
                "entering repair iteration"
            );

            let current_rust = unit.rust_output.as_deref().unwrap_or("");
            let errors = &unit.last_errors;
            let diff_feedback = &unit.last_diff_feedback;

            if errors.is_empty()
                && diff_feedback.is_empty()
                && unit.idiomatic_score.unwrap_or(0) >= config.min_idiomatic_score
            {
                debug!(function = %name, "no errors or diff feedback remaining, re-validating");
            }

            if let Some(ref trail) = audit {
                audit_log(
                    trail,
                    AuditEvent::RepairIteration {
                        function_name: name.clone(),
                        iteration,
                        max_iterations: max_iters,
                        error_count: errors.len(),
                        diff_feedback_count: diff_feedback.len(),
                    },
                );
            }

            let repaired = noricum_agents::repair::repair_function_with_temperature(
                &client,
                &repair_model_sel.model,
                current_rust,
                errors,
                diff_feedback,
                &unit.c_source,
                iteration,
                max_iters,
                config.repair_base_temperature,
            )
            .await?;

            // Estimate token usage for repair call
            let input_token_est = noricum_agents::estimate_tokens(current_rust);
            let output_token_est = noricum_agents::estimate_tokens(&repaired);
            unit.rust_output = Some(repaired);
            unit.state = MigrationState::Repairing(iteration);
            unit.metrics.llm_calls += 1;
            unit.metrics.input_tokens += input_token_est;
            unit.metrics.output_tokens += output_token_est;
            unit.metrics.repair_iterations = iteration;
            check_budget(config, &unit.metrics)?;

            let re_validation =
                noricum_validation::validate_with_threshold(&unit, config.min_idiomatic_score)?;
            noricum_validation::apply_validation_with_max(&mut unit, &re_validation, max_iters);
            info!(
                function = %name,
                iteration,
                state = ?unit.state,
                compiles = re_validation.compiles,
                idiomatic_score = re_validation.idiomatic_score,
                unsafe_count = re_validation.unsafe_count,
                diff_test = ?re_validation.diff_test_passed,
                "repair iteration result"
            );

            if re_validation.passed {
                info!(function = %name, "repair succeeded, validated");
                break;
            }

            iteration += 1;
        }

        unit.metrics.repair_ms = repair_start.elapsed().as_millis() as u64;

        // --- Stage 8: Fallback ---
        if unit.state != MigrationState::Validated {
            unit.state = MigrationState::FallbackUnsafe;
            // Prefer C2Rust output as fallback if available
            if let Some(ref c2rust) = unit.c2rust_output {
                unit.rust_output = Some(c2rust.clone());
            }
            warn!(
                function = %name,
                state = ?unit.state,
                "max repair iterations reached, falling back to unsafe"
            );
        }
    }

    // --- Stage 9: Test generation ---
    if unit.state == MigrationState::Validated && config.generate_tests {
        let test_gen_start = Instant::now();
        let test_model_sel = select_model(&provider_config, difficulty, "test_gen")?;
        info!(
            function = %name,
            model = %test_model_sel.model,
            "calling test generation agent"
        );

        match noricum_agents::test_gen::generate_tests_with_temperature(
            &client,
            &test_model_sel.model,
            &unit.c_source,
            unit.rust_output.as_deref().unwrap_or(""),
            &name,
            config.test_gen_temperature,
        )
        .await
        {
            Ok(test_code) => {
                unit.metrics.test_gen_ms = test_gen_start.elapsed().as_millis() as u64;
                unit.metrics.llm_calls += 1;
                unit.metrics.input_tokens += noricum_agents::estimate_tokens(&unit.c_source);
                unit.metrics.output_tokens += noricum_agents::estimate_tokens(&test_code);
                info!(
                    function = %name,
                    test_code_len = test_code.len(),
                    test_gen_ms = unit.metrics.test_gen_ms,
                    "test generation succeeded"
                );
                unit.generated_tests = Some(test_code);
            }
            Err(e) => {
                warn!(
                    function = %name,
                    error = %e,
                    "test generation failed (non-fatal)"
                );
            }
        }
    }

    // --- Cache successful translations ---
    if unit.state == MigrationState::Validated
        && let Some(ref rust) = unit.rust_output
    {
        cache::put(&unit.c_source, rust);
    }

    // --- Fuzz testing (after validation passes) ---
    if config.fuzz_test
        && unit.state == MigrationState::Validated
        && let Some(ref rust_output) = unit.rust_output
    {
        let fuzz_config = noricum_tools::fuzz_test::FuzzConfig {
            iterations: config.fuzz_iterations,
            seed: Some(42),
            ..Default::default()
        };
        match noricum_tools::fuzz_test::run_fuzz_test(&unit.c_source, rust_output, &fuzz_config) {
            Ok(result) => {
                unit.metrics.fuzz_test_passed = Some(result.all_passed);
                unit.metrics.fuzz_divergence_count = result.failures;
                if !result.all_passed {
                    info!(
                        function = %name,
                        failures = result.failures,
                        iterations = result.iterations_run,
                        "fuzz test found divergences"
                    );
                    // Feed divergences back as diff feedback for potential repair
                    if let Some(ref div) = result.first_divergence {
                        unit.last_diff_feedback.push(format!(
                            "Fuzz divergence (input {:?}): C={:?} Rust={:?}",
                            div.input.label, div.c_output, div.rust_output
                        ));
                    }
                } else {
                    info!(
                        function = %name,
                        iterations = result.iterations_run,
                        "fuzz test passed"
                    );
                }
            }
            Err(e) => {
                debug!(function = %name, error = %e, "fuzz test failed (non-fatal)");
            }
        }
    }

    // Finalize metrics
    unit.metrics.total_ms = pipeline_start.elapsed().as_millis() as u64;
    unit.metrics.c_lines = unit.c_source.lines().count() as u32;
    unit.metrics.rust_lines = unit
        .rust_output
        .as_ref()
        .map(|s| s.lines().count() as u32)
        .unwrap_or(0);
    unit.metrics.diff_test_passed = validation.diff_test_passed;
    unit.metrics.compute_cost();

    info!(
        function = %name,
        state = ?unit.state,
        idiomatic_score = ?unit.idiomatic_score,
        unsafe_count = ?unit.unsafe_count,
        total_ms = unit.metrics.total_ms,
        llm_calls = unit.metrics.llm_calls,
        repair_iters = unit.metrics.repair_iterations,
        "async migration complete"
    );

    if let Some(ref trail) = audit {
        audit_log(
            trail,
            AuditEvent::PipelineComplete {
                function_name: name.clone(),
                final_state: format!("{:?}", unit.state),
                total_ms: unit.metrics.total_ms,
                llm_calls: unit.metrics.llm_calls,
            },
        );
        // Finalize the audit trail
        if let Ok(t) = std::sync::Arc::try_unwrap(trail.clone())
            && let Ok(inner) = t.into_inner()
        {
            let _ = inner.finalize();
        }
    }

    Ok(unit)
}

/// Run the full async LLM migration pipeline on all C files in a directory.
///
/// Uses `DependencyGraph` to determine topological order (dependencies first),
/// and accumulates migrated Rust signatures to inject as context for later files.
pub async fn migrate_directory(
    dir: &Path,
    config: &MigrationConfig,
) -> Result<MigrationProject, CoreError> {
    let dir_str = dir.to_string_lossy().to_string();
    let mut project = MigrationProject::new(
        dir.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string()),
        dir_str.clone(),
    );

    let mut found_any = false;
    let mut c_files: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "c") {
            found_any = true;
            c_files.push(path);
        }
    }

    if !found_any {
        return Err(CoreError::NoSourceFiles(dir_str));
    }

    // Build dependency graph for topological ordering
    let ordered_names = match crate::dependency::DependencyGraph::from_directory(dir) {
        Ok(graph) => {
            let order = graph.topological_sort();
            if !order.is_empty() {
                info!(order = ?order, "dependency-ordered migration");
            }
            order
        }
        Err(e) => {
            debug!(error = %e, "dependency graph failed, using filesystem order");
            Vec::new()
        }
    };

    // Sort c_files by topological order if available, otherwise alphabetical
    if !ordered_names.is_empty() {
        c_files.sort_by_key(|p| {
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            ordered_names
                .iter()
                .position(|n| n == stem)
                .unwrap_or(usize::MAX)
        });
    } else {
        c_files.sort();
    }

    // Accumulate migrated Rust signatures for dependency context
    let mut migrated_signatures: Vec<String> = Vec::new();

    for path in &c_files {
        info!(
            file = %path.display(),
            context_signatures = migrated_signatures.len(),
            "migrating file in directory"
        );
        let unit = migrate_file(path, config).await?;

        // Extract function signatures from successful migrations for context
        if unit.state == MigrationState::Validated
            && let Some(ref rust_output) = unit.rust_output
        {
            let sigs = extract_rust_signatures(rust_output);
            if !sigs.is_empty() {
                info!(
                    file = %path.display(),
                    signatures = sigs.len(),
                    "collected signatures for dependency context"
                );
                migrated_signatures.extend(sigs);
            }
        }

        project.add_unit(unit);
    }

    let summary = project.progress_summary();
    info!(
        total = summary.total,
        validated = summary.validated,
        failed = summary.failed,
        "directory async migration complete"
    );

    Ok(project)
}

/// Extract function signatures from Rust source code for dependency context.
///
/// Looks for `pub fn` and `fn` lines, returning them as context strings
/// that can be injected into translation prompts for dependent files.
fn extract_rust_signatures(rust_source: &str) -> Vec<String> {
    rust_source
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            (trimmed.starts_with("pub fn ") || trimmed.starts_with("fn ")) && trimmed.contains('(')
        })
        .map(|line| {
            // Take up to the opening brace or end of line
            let trimmed = line.trim();
            if let Some(brace) = trimmed.find('{') {
                trimmed[..brace].trim().to_string()
            } else {
                trimmed.to_string()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrate_file_sync_without_c2rust() {
        // Without c2rust, simple functions should be translated via rule_translate
        let tmp = tempfile::tempdir().unwrap();
        let c_file = tmp.path().join("test.c");
        std::fs::write(&c_file, "int add(int a, int b) { return a + b; }").unwrap();

        let unit = migrate_file_sync(&c_file).unwrap();
        assert_eq!(unit.name, "test");
        assert!(!unit.c_source.is_empty());
        // Rule-based translation succeeds for simple functions
        assert!(unit.rust_output.is_some());
        assert!(unit.rust_output.as_ref().unwrap().contains("fn add"));
    }

    #[test]
    fn test_migrate_file_sync_complex_without_c2rust() {
        // Complex functions that rule_translate can't handle should stay Extracted
        let tmp = tempfile::tempdir().unwrap();
        let c_file = tmp.path().join("test.c");
        std::fs::write(
            &c_file,
            "void* alloc(int n) { return malloc(n * sizeof(int)); }",
        )
        .unwrap();

        let unit = migrate_file_sync(&c_file).unwrap();
        assert_eq!(unit.name, "test");
        assert_eq!(unit.state, MigrationState::Extracted);
        assert!(unit.rust_output.is_none());
    }

    #[test]
    fn test_migrate_directory_sync() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.c"), "int a() { return 1; }").unwrap();
        std::fs::write(tmp.path().join("b.c"), "int b() { return 2; }").unwrap();

        let project = migrate_directory_sync(tmp.path()).unwrap();
        assert_eq!(project.units.len(), 2);
    }

    #[test]
    fn test_migrate_directory_sync_no_c_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("readme.txt"), "hello").unwrap();

        let result = migrate_directory_sync(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_migration_config_default() {
        let config = MigrationConfig::default();
        assert_eq!(config.max_repair_iterations, 5);
        assert_eq!(config.min_idiomatic_score, 60);
        assert!(config.generate_tests);
    }

    #[test]
    fn test_provider_config_from_migration_config() {
        let config = MigrationConfig {
            anthropic_api_key: Some("test-key".to_string()),
            ollama_url: Some("http://custom:1234".to_string()),
            max_repair_iterations: 3,
            min_idiomatic_score: 70,
            generate_tests: false,
            ..Default::default()
        };
        let pc = ProviderConfig::from(&config);
        assert_eq!(pc.anthropic_api_key, Some("test-key".to_string()));
        assert_eq!(pc.ollama_url, "http://custom:1234");
    }

    #[tokio::test]
    async fn test_migrate_file_async_no_llm_falls_back() {
        // With no API key and no Ollama, the async function should fall back to sync
        let tmp = tempfile::tempdir().unwrap();
        let c_file = tmp.path().join("test.c");
        std::fs::write(&c_file, "int add(int a, int b) { return a + b; }").unwrap();

        let config = MigrationConfig {
            anthropic_api_key: None,
            ollama_url: None,
            max_repair_iterations: 5,
            min_idiomatic_score: 60,
            generate_tests: false,
            ..Default::default()
        };

        let unit = migrate_file(&c_file, &config).await.unwrap();
        assert_eq!(unit.name, "test");
        assert!(!unit.c_source.is_empty());
        // Falls back to sync; rule_translate handles simple functions
        assert!(unit.rust_output.is_some());
        assert!(unit.rust_output.as_ref().unwrap().contains("fn add"));
    }

    #[tokio::test]
    async fn test_migrate_directory_async_no_llm() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.c"), "int a() { return 1; }").unwrap();
        std::fs::write(tmp.path().join("b.c"), "int b() { return 2; }").unwrap();

        let config = MigrationConfig {
            anthropic_api_key: None,
            ollama_url: None,
            max_repair_iterations: 5,
            min_idiomatic_score: 60,
            generate_tests: false,
            ..Default::default()
        };

        let project = migrate_directory(tmp.path(), &config).await.unwrap();
        assert_eq!(project.units.len(), 2);
    }

    #[tokio::test]
    async fn test_migrate_directory_async_no_c_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("readme.txt"), "hello").unwrap();

        let config = MigrationConfig::default();
        let result = migrate_directory(tmp.path(), &config).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_rust_signatures() {
        let rust = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn helper(x: i32) -> i32 {
    x * 2
}

fn main() {
    println!("{}", add(1, 2));
}
"#;
        let sigs = extract_rust_signatures(rust);
        assert_eq!(sigs.len(), 3);
        assert!(sigs[0].contains("pub fn add(a: i32, b: i32) -> i32"));
        assert!(sigs[1].contains("fn helper(x: i32) -> i32"));
        assert!(sigs[2].contains("fn main()"));
    }

    #[test]
    fn test_extract_rust_signatures_no_functions() {
        let rust = "let x = 5;\nstruct Foo { bar: i32 }";
        let sigs = extract_rust_signatures(rust);
        assert!(sigs.is_empty());
    }
}
