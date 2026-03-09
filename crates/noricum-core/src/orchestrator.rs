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
const MAX_C_SOURCE_SIZE: usize = 10 * 1024 * 1024;

/// LOC threshold above which chunked translation is used.
const MEDIUM_FILE_LOC: usize = 800;
/// LOC threshold above which repair iterations are reduced to save tokens.
const LARGE_FILE_LOC: usize = 1000;
/// LOC threshold above which repair iterations are further reduced and larger chunk targets apply.
const VERY_LARGE_FILE_LOC: usize = 2000;
/// LOC threshold for very large files where we use aggressive chunking and P11 signature agreement.
const MASSIVE_FILE_LOC: usize = 5000;
/// LOC threshold above which modular (per-module) migration is used instead of chunked.
const MODULAR_FILE_LOC: usize = 2000;

use crate::CoreError;
use crate::audit::{AuditEvent, SharedAuditTrail, audit_log, create_shared_audit};

/// Configuration for the async LLM-based migration pipeline.
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Anthropic API key. Defaults to `ANTHROPIC_API_KEY` env var.
    pub anthropic_api_key: Option<String>,
    /// Ollama URL override. If `None`, uses the default `http://localhost:11434`.
    pub ollama_url: Option<String>,
    /// Ollama model name override. Defaults to "qwen2.5-coder:32b".
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
    /// Skip C2Rust transpilation entirely. Useful when the LLM produces better
    /// translations directly from C source, saving tokens and time.
    pub skip_c2rust: bool,
    /// Directory for pipeline artifact persistence. Every intermediate output
    /// is saved here for debugging and recovery. Defaults to `.noricum-artifacts/`.
    pub artifacts_dir: std::path::PathBuf,
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
            max_tokens_budget: Some(2_000_000),
            max_llm_calls: Some(50),
            skip_c2rust: false,
            artifacts_dir: std::path::PathBuf::from(".noricum-artifacts"),
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
                .unwrap_or_else(|| "qwen2.5-coder:32b".to_string()),
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
    let effective_iters = effective_repair_iterations(config.max_repair_iterations, c_lines, false);
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

/// Compute effective max repair iterations based on file size and chunking.
///
/// Large files use fewer iterations to conserve tokens — each repair
/// iteration sends the full Rust + C source, which is expensive.
/// Chunked translations get full iterations (minimum 8) since each chunk is small.
fn effective_repair_iterations(configured_max: u32, c_lines: usize, was_chunked: bool) -> u32 {
    if was_chunked {
        // Chunked files need more repair iterations since each iteration repairs
        // the combined output of all chunks
        return configured_max.max(8);
    }
    if c_lines > MASSIVE_FILE_LOC {
        configured_max.min(3) // Still allow some repair for 5000+ LOC
    } else if c_lines > VERY_LARGE_FILE_LOC {
        configured_max.min(2)
    } else if c_lines > LARGE_FILE_LOC {
        configured_max.min(3)
    } else {
        configured_max
    }
}

/// Check if Rust output has substance relative to C source (not empty stubs).
fn has_substance(rust_source: &str, c_source: &str) -> bool {
    let c_nl = c_source.lines().filter(|l| !l.trim().is_empty()).count();
    let r_lines = rust_source.lines().filter(|l| !l.trim().is_empty()).count();
    let efn = noricum_validation::count_empty_functions(rust_source);
    let tfn = noricum_validation::count_total_functions(rust_source);
    !(c_nl > 20 && r_lines < c_nl / 4 || tfn > 3 && efn as f32 / tfn as f32 > 0.3)
}

/// Number of consecutive stalled iterations before triggering re-translation.
const STALL_THRESHOLD: u32 = 2;

/// Derive pattern tags from C source for RAG indexing.
///
/// Scans for common C patterns (pointers, malloc, structs, etc.) and returns
/// matching tag strings for `PatternStore` relevance scoring.
fn derive_pattern_tags(c_source: &str) -> Vec<String> {
    let keywords = [
        ("malloc", "memory"),
        ("free", "memory"),
        ("calloc", "memory"),
        ("realloc", "memory"),
        ("struct ", "struct"),
        ("enum ", "enum"),
        ("typedef", "typedef"),
        ("FILE", "file_io"),
        ("fopen", "file_io"),
        ("printf", "printf"),
        ("strcmp", "string"),
        ("strcpy", "string"),
        ("strlen", "string"),
        ("strdup", "string"),
        ("NULL", "null"),
        ("->", "pointer"),
        ("*)", "pointer"),
        ("void*", "pointer"),
        ("for (", "loop"),
        ("while (", "loop"),
        ("switch (", "switch"),
    ];

    let mut tags: Vec<String> = keywords
        .iter()
        .filter(|(kw, _)| c_source.contains(kw))
        .map(|(_, tag)| tag.to_string())
        .collect();

    tags.sort();
    tags.dedup();
    tags
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
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(c_source.as_bytes());
        format!("{:064x}", hash)
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

            // Prefer rule-based translation over c2rust when available, since
            // rule_translate produces safe idiomatic Rust while c2rust wraps
            // everything in unsafe.
            match noricum_tools::rule_translate::try_translate(&unit.c_source, &unit.name) {
                Some(rust_code) => {
                    unit.rust_output = Some(rust_code);
                    unit.state = MigrationState::Refined;
                    info!(function = %unit.name, "rule-based translation preferred over c2rust");
                }
                None => {
                    unit.rust_output = Some(output.rust_source);
                    unit.state = MigrationState::C2RustDone;
                    info!(function = %unit.name, "c2rust transpilation succeeded (rule-based not applicable)");
                }
            }
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
    let c_source = tokio::fs::read_to_string(c_file).await?;

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
    // P4: Skip c2rust when configured — the LLM often produces better translations
    // directly from C source, and c2rust output wastes tokens for large files.
    if config.skip_c2rust {
        debug!(function = %name, "skipping c2rust (skip_c2rust=true)");
    } else {
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
    }

    // --- Stage 4: Analysis agent ---
    let analysis_start = Instant::now();
    let analysis_model_sel = select_model(&provider_config, difficulty, "analysis")?;
    // Record provider/model in metrics (first model selection)
    unit.metrics.provider = Some(format!("{:?}", analysis_model_sel.provider));
    unit.metrics.model = Some(analysis_model_sel.model.clone());
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

    // --- P3: Try modular migration for large files ---
    let c_lines_total = unit.c_source.lines().count();
    if unit.state != MigrationState::Refined && c_lines_total > MODULAR_FILE_LOC {
        info!(
            function = %name,
            c_lines = c_lines_total,
            threshold = MODULAR_FILE_LOC,
            "P3: file exceeds modular threshold, attempting per-module migration"
        );
        match migrate_file_modular(
            &unit.c_source,
            &name,
            config,
            &client,
            &provider_config,
            &analysis,
            difficulty,
        )
        .await?
        {
            ModularResult::Success {
                rust_code,
                metrics,
                all_validated,
            } => {
                unit.rust_output = Some(rust_code);
                unit.state = MigrationState::Refined;
                unit.metrics.llm_calls += metrics.llm_calls;
                unit.metrics.input_tokens += metrics.input_tokens;
                unit.metrics.output_tokens += metrics.output_tokens;
                unit.metrics.translation_ms += metrics.translation_ms;
                unit.metrics.repair_ms += metrics.repair_ms;
                info!(
                    function = %name,
                    llm_calls = metrics.llm_calls,
                    all_validated,
                    "P3: modular migration produced combined output"
                );
            }
            ModularResult::FallbackToChunked => {
                info!(function = %name, "P3: modular split not useful, falling back to chunked");
            }
        }
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
        let c_lines_for_chunk = unit.c_source.lines().count();
        let use_chunked = c_lines_for_chunk > MEDIUM_FILE_LOC;
        let rust_code = if use_chunked {
            let chunk_target = if c_lines_for_chunk > MASSIVE_FILE_LOC {
                600 // Larger chunks for massive files — fewer API calls, more context per chunk
            } else if c_lines_for_chunk > VERY_LARGE_FILE_LOC {
                500
            } else {
                400
            };
            // Use structural chunking when data model patterns are detected
            let has_data_model = analysis.patterns.iter().any(|p| {
                p.contains("struct") || p.contains("linked_list") || p.contains("recursive")
            });
            let chunks = if has_data_model {
                let structural =
                    noricum_tools::ast::chunk_c_source_structural(&unit.c_source, chunk_target);
                if structural.len() > 1 {
                    info!(
                        function = %name,
                        "using structural chunking (data model first)"
                    );
                    structural
                } else {
                    noricum_tools::ast::chunk_c_source(&unit.c_source, chunk_target)
                }
            } else {
                noricum_tools::ast::chunk_c_source(&unit.c_source, chunk_target)
            };
            info!(
                function = %name,
                chunks = chunks.len(),
                c_lines = c_lines_for_chunk,
                chunk_target,
                "using multi-pass chunked translation"
            );
            match noricum_agents::translation::translate_chunked(
                &client,
                &translation_model_sel.model,
                &chunks,
                unit.c2rust_output.as_deref(),
                &analysis,
                &relevant_patterns,
                config.translation_temperature,
            )
            .await
            {
                Ok(code) => code,
                Err(e) => {
                    warn!(function = %name, error = %e, "chunked translation failed, falling back to sync");
                    return migrate_file_sync(c_file);
                }
            }
        } else {
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
            }
        };

        // P6: Substance gate — reject translations that are mostly empty stubs.
        // This catches the case where the LLM returns function signatures with empty bodies.
        let c_lines_nonempty = unit
            .c_source
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count();
        let rust_lines_nonempty = rust_code.lines().filter(|l| !l.trim().is_empty()).count();
        let empty_fn_count = noricum_validation::count_empty_functions(&rust_code);
        let total_fn_count = noricum_validation::count_total_functions(&rust_code);
        let is_stub = (c_lines_nonempty > 20 && rust_lines_nonempty < c_lines_nonempty / 4)
            || (total_fn_count > 3 && empty_fn_count as f32 / total_fn_count as f32 > 0.3);

        let rust_code = if is_stub {
            warn!(
                function = %name,
                c_lines = c_lines_nonempty,
                rust_lines = rust_lines_nonempty,
                empty_fns = empty_fn_count,
                total_fns = total_fn_count,
                "P6: translation produced empty stubs, re-translating with temperature 0.5"
            );
            unit.metrics.llm_calls += 1;
            match noricum_agents::translation::translate_function_with_patterns_and_temperature(
                &client,
                &translation_model_sel.model,
                &unit.c_source,
                unit.c2rust_output.as_deref(),
                &analysis,
                &relevant_patterns,
                Some(0.5),
            )
            .await
            {
                Ok(retranslated) => {
                    let new_lines = retranslated
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .count();
                    let new_empty = noricum_validation::count_empty_functions(&retranslated);
                    let new_total = noricum_validation::count_total_functions(&retranslated);
                    let still_stub = (c_lines_nonempty > 20 && new_lines < c_lines_nonempty / 4)
                        || (new_total > 3 && new_empty as f32 / new_total as f32 > 0.3);
                    if !still_stub {
                        info!(
                            function = %name,
                            old_lines = rust_lines_nonempty,
                            new_lines,
                            "P6: re-translation produced substantial code"
                        );
                        retranslated
                    } else {
                        warn!(function = %name, "P6: re-translation still produced stubs");
                        rust_code
                    }
                }
                Err(e) => {
                    warn!(function = %name, error = %e, "P6: re-translation failed");
                    rust_code
                }
            }
        } else {
            rust_code
        };

        // Quality gate: if initial translation has >5 unsafe blocks, re-translate
        let unsafe_count = noricum_tools::ast::count_unsafe_blocks_ast(&rust_code);
        let rust_code = if unsafe_count > 5 {
            warn!(
                function = %name,
                unsafe_count,
                "initial translation has too many unsafe blocks, re-translating with temperature 0.5"
            );
            unit.metrics.llm_calls += 1;
            match noricum_agents::translation::translate_function_with_patterns_and_temperature(
                &client,
                &translation_model_sel.model,
                &unit.c_source,
                unit.c2rust_output.as_deref(),
                &analysis,
                &relevant_patterns,
                Some(0.5),
            )
            .await
            {
                Ok(retranslated) => {
                    let new_unsafe = noricum_tools::ast::count_unsafe_blocks_ast(&retranslated);
                    if new_unsafe < unsafe_count {
                        info!(
                            function = %name,
                            old_unsafe = unsafe_count,
                            new_unsafe,
                            "re-translation reduced unsafe blocks"
                        );
                        retranslated
                    } else {
                        info!(
                            function = %name,
                            "re-translation did not improve, keeping original"
                        );
                        rust_code
                    }
                }
                Err(e) => {
                    warn!(function = %name, error = %e, "re-translation failed, keeping original");
                    rust_code
                }
            }
        } else {
            rust_code
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
    let was_chunked = c_lines > MEDIUM_FILE_LOC;
    let max_iters = effective_repair_iterations(config.max_repair_iterations, c_lines, was_chunked);

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
    // P0: Track baseline unsafe count from translation to enforce quality floor.
    // P1: Track best version (highest score with acceptable unsafe count).
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

        // P0: Baseline unsafe count from the initial translation.
        // Repair must NEVER produce more unsafe blocks than this.
        let baseline_unsafe = validation.unsafe_count;

        // P1: Best-version tracking — keep the version with the highest score
        // that doesn't exceed the baseline unsafe count.
        // P6b: Only seed best version if it has substance (not empty stubs).
        let initial_has_substance = unit
            .rust_output
            .as_deref()
            .is_some_and(|r| has_substance(r, &unit.c_source));
        let mut best_version: Option<String> = if initial_has_substance {
            unit.rust_output.clone()
        } else {
            warn!(function = %name, "P6b: initial translation is stub, not seeding as best version");
            None
        };
        let mut best_score: u32 = if initial_has_substance {
            validation.idiomatic_score
        } else {
            0
        };
        let mut best_unsafe: u32 = baseline_unsafe;
        let mut best_compiles: bool = if initial_has_substance {
            validation.compiles
        } else {
            false
        };

        let mut iteration = 1u32;
        let mut prev_error_count: Option<usize> = None;
        let mut stall_count: u32 = 0;
        let mut retranslated_on_stall = false;

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

            // P5: When code compiles and diff passes but score is below threshold,
            // generate idiomatic improvement hints so the repair agent has actionable feedback
            // instead of returning the code unchanged.
            let idiomatic_hints = if errors.is_empty()
                && diff_feedback.is_empty()
                && unit.idiomatic_score.unwrap_or(0) < config.min_idiomatic_score
            {
                let hints = noricum_validation::generate_idiomatic_hints(current_rust);
                if !hints.is_empty() {
                    info!(
                        function = %name,
                        score = unit.idiomatic_score.unwrap_or(0),
                        target = config.min_idiomatic_score,
                        hint_count = hints.len(),
                        "P5: injecting idiomatic improvement hints"
                    );
                }
                hints
            } else {
                Vec::new()
            };
            let effective_diff_feedback = if idiomatic_hints.is_empty() {
                diff_feedback.clone()
            } else {
                idiomatic_hints
            };

            if errors.is_empty()
                && effective_diff_feedback.is_empty()
                && unit.idiomatic_score.unwrap_or(0) >= config.min_idiomatic_score
            {
                debug!(function = %name, "no errors or diff feedback remaining, re-validating");
            }

            // --- Stall detection ---
            let current_error_count = errors.len() + effective_diff_feedback.len();
            if let Some(prev) = prev_error_count {
                if current_error_count == prev && current_error_count > 0 {
                    stall_count += 1;
                    warn!(
                        function = %name,
                        stall_count,
                        error_count = current_error_count,
                        "repair stalled — error count unchanged"
                    );
                } else {
                    stall_count = 0;
                }
            }
            prev_error_count = Some(current_error_count);

            // --- Re-translate on stall ---
            // If repair is stuck for STALL_THRESHOLD iterations, try a fresh translation
            // with higher temperature instead of continuing to patch the same broken code.
            if stall_count >= STALL_THRESHOLD && !retranslated_on_stall {
                retranslated_on_stall = true;
                warn!(
                    function = %name,
                    stall_count,
                    "repair stalled — attempting re-translation with temperature 0.7"
                );

                let stall_patterns = PatternStore::load_seed_patterns();
                let stall_relevant = stall_patterns.find_relevant(&unit.c_source, 3);
                let retranslate_result =
                    noricum_agents::translation::translate_function_with_patterns_and_temperature(
                        &client,
                        &repair_model_sel.model,
                        &unit.c_source,
                        unit.c2rust_output.as_deref(),
                        &analysis,
                        &stall_relevant,
                        Some(0.7),
                    )
                    .await;

                if let Ok(retranslated) = retranslate_result {
                    unit.rust_output = Some(retranslated);
                    unit.metrics.llm_calls += 1;
                    stall_count = 0;
                    prev_error_count = None;
                    info!(function = %name, "re-translation complete, resetting repair loop");
                    // Re-validate with the new translation
                    let re_validation = noricum_validation::validate_with_threshold(
                        &unit,
                        config.min_idiomatic_score,
                    )?;
                    noricum_validation::apply_validation_with_max(
                        &mut unit,
                        &re_validation,
                        max_iters,
                    );

                    // P1+P6b: Update best version if this re-translation is better and has substance
                    let retrans_has_substance = unit
                        .rust_output
                        .as_deref()
                        .is_some_and(|r| has_substance(r, &unit.c_source));
                    if retrans_has_substance
                        && re_validation.unsafe_count <= baseline_unsafe
                        && (re_validation.idiomatic_score > best_score
                            || (re_validation.compiles && !best_compiles))
                    {
                        best_version = unit.rust_output.clone();
                        best_score = re_validation.idiomatic_score;
                        best_unsafe = re_validation.unsafe_count;
                        best_compiles = re_validation.compiles;
                        info!(
                            function = %name,
                            best_score,
                            best_unsafe,
                            best_compiles,
                            "new best version from re-translation"
                        );
                    }

                    if re_validation.passed {
                        info!(function = %name, "re-translation passed validation directly");
                        break;
                    }
                    iteration += 1;
                    continue;
                } else {
                    warn!(function = %name, "re-translation failed, continuing repair");
                }
            }

            if let Some(ref trail) = audit {
                audit_log(
                    trail,
                    AuditEvent::RepairIteration {
                        function_name: name.clone(),
                        iteration,
                        max_iterations: max_iters,
                        error_count: errors.len(),
                        diff_feedback_count: effective_diff_feedback.len(),
                    },
                );
            }

            // Pass full C source for files <1500 LOC, abbreviated for larger ones
            let c_abbrev_limit = if c_lines < 1500 { None } else { Some(500) };
            let repair_result = noricum_agents::repair::repair_function_full(
                &client,
                &repair_model_sel.model,
                current_rust,
                errors,
                &effective_diff_feedback,
                &unit.c_source,
                iteration,
                max_iters,
                config.repair_base_temperature,
                c_abbrev_limit,
            )
            .await;

            let repaired = match repair_result {
                Ok(r) => r,
                Err(e) => {
                    warn!(
                        function = %name,
                        iteration,
                        error = %e,
                        "repair LLM call failed (transient error), skipping iteration"
                    );
                    unit.metrics.llm_calls += 1;
                    unit.metrics.repair_iterations = iteration;
                    iteration += 1;
                    continue;
                }
            };

            // P0: Quality floor — reject repair if it introduces more unsafe blocks
            let repaired_unsafe = noricum_tools::ast::count_unsafe_blocks_ast(&repaired);
            if repaired_unsafe > baseline_unsafe {
                warn!(
                    function = %name,
                    iteration,
                    repaired_unsafe,
                    baseline_unsafe,
                    "P0: repair rejected — introduces more unsafe blocks than translation baseline"
                );
                // Don't apply this repair; keep the current version and continue
                unit.metrics.llm_calls += 1;
                unit.metrics.repair_iterations = iteration;
                iteration += 1;
                continue;
            }

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

            // P1+P6b: Update best version if this repair is better AND has substance
            let repair_has_substance = unit
                .rust_output
                .as_deref()
                .is_some_and(|r| has_substance(r, &unit.c_source));
            if repair_has_substance
                && re_validation.unsafe_count <= baseline_unsafe
                && (re_validation.idiomatic_score > best_score
                    || (re_validation.compiles && !best_compiles))
            {
                best_version = unit.rust_output.clone();
                best_score = re_validation.idiomatic_score;
                best_unsafe = re_validation.unsafe_count;
                best_compiles = re_validation.compiles;
                info!(
                    function = %name,
                    iteration,
                    best_score,
                    best_unsafe,
                    best_compiles,
                    "new best version from repair"
                );
            }

            if re_validation.passed {
                info!(function = %name, "repair succeeded, validated");
                break;
            }

            iteration += 1;
        }

        unit.metrics.repair_ms = repair_start.elapsed().as_millis() as u64;

        // --- Stage 8: Fallback ---
        // P1: Use best-tracked version instead of falling back to raw c2rust output.
        // This preserves the highest-quality translation even if it didn't fully pass.
        if unit.state != MigrationState::Validated {
            unit.state = MigrationState::FallbackUnsafe;
            if let Some(best) = best_version {
                info!(
                    function = %name,
                    best_score,
                    best_unsafe,
                    best_compiles,
                    "P1: using best-tracked version instead of c2rust fallback"
                );
                unit.rust_output = Some(best);
                unit.idiomatic_score = Some(best_score);
                unit.unsafe_count = Some(best_unsafe);
            } else if let Some(ref c2rust) = unit.c2rust_output {
                unit.rust_output = Some(c2rust.clone());
            }
            warn!(
                function = %name,
                state = ?unit.state,
                "max repair iterations reached, falling back"
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

    // --- Cache successful translations + auto-add to RAG store ---
    if unit.state == MigrationState::Validated
        && let Some(ref rust) = unit.rust_output
    {
        cache::put(&unit.c_source, rust);

        // Auto-add successful migration as a RAG pattern for future context
        let tags = derive_pattern_tags(&unit.c_source);
        let pattern = noricum_ir::pattern_store::MigrationPattern {
            name: name.clone(),
            c_pattern: unit.c_source.clone(),
            rust_pattern: rust.clone(),
            tags,
            usage_count: 0,
        };
        let mut store = PatternStore::load_seed_patterns();
        store.add_pattern(pattern);
        debug!(function = %name, "added successful migration to RAG pattern store");
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

// ---------------------------------------------------------------------------
// Modular migration (P3: per-module pipeline for large files)
// ---------------------------------------------------------------------------

/// Migrate a large C file by splitting into semantic modules and migrating each independently.
///
/// Instead of translating the entire file as one unit (which overwhelms the repair loop),
/// this function:
/// 1. Splits the file into semantic modules by function prefix
/// 2. Orders modules by inter-module call dependencies (leaves first)
/// 3. Migrates each module through the full pipeline (translate → validate → repair)
/// 4. Accumulates compiled Rust context from completed modules
/// 5. Assembles the final output from all module outputs
///
/// Each module gets its own repair loop with reasonable context size, solving the
/// core problem where repair of 4000+ LOC Rust is too large for the LLM to handle.
async fn migrate_file_modular(
    c_source: &str,
    name: &str,
    config: &MigrationConfig,
    client: &LlmClient,
    provider_config: &noricum_agents::providers::ProviderConfig,
    analysis: &noricum_agents::analysis::AnalysisResult,
    difficulty: noricum_ir::Difficulty,
) -> Result<ModularResult, CoreError> {
    let modules = noricum_tools::ast::split_into_modules(c_source);

    if modules.len() <= 1 {
        info!(function = %name, "modular split produced single module, falling back to chunked");
        return Ok(ModularResult::FallbackToChunked);
    }

    // Build intra-file dependency graph and order modules
    let dep_graph = crate::dependency::DependencyGraph::from_source(c_source);
    let order = dep_graph.module_order(&modules);

    let module_names: Vec<&str> = order.iter().map(|&i| modules[i].name.as_str()).collect();
    info!(
        function = %name,
        module_count = modules.len(),
        order = ?module_names,
        "P3: modular migration — {} modules in dependency order",
        modules.len()
    );

    let mut module_outputs: Vec<(String, String)> = Vec::new(); // (module_name, rust_code)
    let mut accumulated_rust_context = String::new();
    let mut total_metrics = noricum_ir::MigrationMetrics::default();
    let mut all_validated = true;
    let mut any_succeeded = false;
    let mut best_combined_score: u32 = 0;

    let pattern_store = PatternStore::load_seed_patterns();

    for &mod_idx in &order {
        let module = &modules[mod_idx];
        let mod_name = format!("{name}::{}", module.name);

        info!(
            module = %mod_name,
            functions = module.function_names.len(),
            lines = module.line_count,
            "migrating module ({}/{})",
            module_outputs.len() + 1,
            modules.len()
        );

        // Create a FunctionUnit for this module
        let mut mod_unit =
            FunctionUnit::new(mod_name.clone(), String::new(), module.source.clone());
        mod_unit.difficulty = Some(difficulty);

        // Build translation context: accumulated Rust from prior modules
        let context_note = if !accumulated_rust_context.is_empty() {
            format!(
                "\n// === Already migrated modules (use these types/functions) ===\n{}\n// === End migrated context ===\n",
                accumulated_rust_context
            )
        } else {
            String::new()
        };

        // Translate this module
        let translation_start = Instant::now();
        let translation_model_sel = select_model(provider_config, difficulty, "translation")?;
        let relevant_patterns = pattern_store.find_relevant(&module.source, 3);

        // For modules, always use single-pass translation (each module is ≤ ~1000 LOC)
        // Inject accumulated context as a prefix hint in the C source
        let augmented_c = if context_note.is_empty() {
            module.source.clone()
        } else {
            format!(
                "/* MIGRATION CONTEXT: The following Rust code has already been migrated from earlier modules in this file. \
                 Use compatible types and function signatures.\n{}\n*/\n\n{}",
                accumulated_rust_context, module.source
            )
        };

        let rust_code =
            match noricum_agents::translation::translate_function_with_patterns_and_temperature(
                client,
                &translation_model_sel.model,
                &augmented_c,
                None, // No c2rust context for modular
                analysis,
                &relevant_patterns,
                config.translation_temperature,
            )
            .await
            {
                Ok(code) => code,
                Err(e) => {
                    warn!(module = %mod_name, error = %e, "module translation failed");
                    all_validated = false;
                    continue;
                }
            };

        total_metrics.llm_calls += 1;
        total_metrics.input_tokens += noricum_agents::estimate_tokens(&augmented_c);
        total_metrics.output_tokens += noricum_agents::estimate_tokens(&rust_code);
        total_metrics.translation_ms += translation_start.elapsed().as_millis() as u64;

        // Substance check
        let is_stub = !has_substance(&rust_code, &module.source);
        let rust_code = if is_stub {
            warn!(module = %mod_name, "P6: module translation produced stubs, re-translating");
            total_metrics.llm_calls += 1;
            match noricum_agents::translation::translate_function_with_patterns_and_temperature(
                client,
                &translation_model_sel.model,
                &augmented_c,
                None,
                analysis,
                &relevant_patterns,
                Some(0.5),
            )
            .await
            {
                Ok(retranslated) if has_substance(&retranslated, &module.source) => retranslated,
                _ => rust_code,
            }
        } else {
            rust_code
        };

        mod_unit.rust_output = Some(rust_code);
        mod_unit.state = MigrationState::Refined;

        // Validate this module (compile check + scoring, no diff test for modules)
        let mod_validation =
            noricum_validation::validate_with_threshold(&mod_unit, config.min_idiomatic_score)?;
        noricum_validation::apply_validation_with_max(
            &mut mod_unit,
            &mod_validation,
            config.max_repair_iterations,
        );

        info!(
            module = %mod_name,
            compiles = mod_validation.compiles,
            score = mod_validation.idiomatic_score,
            unsafe_count = mod_validation.unsafe_count,
            "module validation"
        );

        // Repair loop for this module (each module is small enough for effective repair)
        if !mod_validation.passed {
            let repair_start = Instant::now();
            let repair_model_sel = select_model(provider_config, difficulty, "repair")?;
            let max_iters = config.max_repair_iterations.min(5);
            let baseline_unsafe = mod_validation.unsafe_count;
            let mut best_version = mod_unit.rust_output.clone();
            let mut best_score = mod_validation.idiomatic_score;
            let mut best_compiles = mod_validation.compiles;

            for iter in 1..=max_iters {
                let current_rust = mod_unit.rust_output.as_deref().unwrap_or("");
                let errors = &mod_unit.last_errors;
                let diff_feedback = &mod_unit.last_diff_feedback;

                let idiomatic_hints = if errors.is_empty()
                    && diff_feedback.is_empty()
                    && mod_unit.idiomatic_score.unwrap_or(0) < config.min_idiomatic_score
                {
                    noricum_validation::generate_idiomatic_hints(current_rust)
                } else {
                    Vec::new()
                };
                let effective_feedback = if idiomatic_hints.is_empty() {
                    diff_feedback.clone()
                } else {
                    idiomatic_hints
                };

                if errors.is_empty()
                    && effective_feedback.is_empty()
                    && mod_unit.idiomatic_score.unwrap_or(0) >= config.min_idiomatic_score
                {
                    break;
                }

                let repaired = match noricum_agents::repair::repair_function_full(
                    client,
                    &repair_model_sel.model,
                    current_rust,
                    errors,
                    &effective_feedback,
                    &module.source,
                    iter,
                    max_iters,
                    config.repair_base_temperature,
                    None, // No abbreviation needed — modules are small
                )
                .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        warn!(module = %mod_name, iter, error = %e, "module repair failed");
                        break;
                    }
                };

                total_metrics.llm_calls += 1;
                total_metrics.input_tokens += noricum_agents::estimate_tokens(current_rust);
                total_metrics.output_tokens += noricum_agents::estimate_tokens(&repaired);

                // P0: Quality floor
                let repaired_unsafe = noricum_tools::ast::count_unsafe_blocks_ast(&repaired);
                if repaired_unsafe > baseline_unsafe {
                    warn!(module = %mod_name, iter, "P0: repair rejected — unsafe increased");
                    continue;
                }

                mod_unit.rust_output = Some(repaired);
                mod_unit.state = MigrationState::Repairing(iter);

                let re_validation = noricum_validation::validate_with_threshold(
                    &mod_unit,
                    config.min_idiomatic_score,
                )?;
                noricum_validation::apply_validation_with_max(
                    &mut mod_unit,
                    &re_validation,
                    max_iters,
                );

                info!(
                    module = %mod_name,
                    iter,
                    compiles = re_validation.compiles,
                    score = re_validation.idiomatic_score,
                    "module repair iteration"
                );

                // P1: Best-version tracking
                if re_validation.unsafe_count <= baseline_unsafe
                    && (re_validation.idiomatic_score > best_score
                        || (re_validation.compiles && !best_compiles))
                {
                    best_version = mod_unit.rust_output.clone();
                    best_score = re_validation.idiomatic_score;
                    best_compiles = re_validation.compiles;
                }

                if re_validation.passed {
                    info!(module = %mod_name, "module repair succeeded");
                    break;
                }

                check_budget(config, &total_metrics)?;
            }

            total_metrics.repair_ms += repair_start.elapsed().as_millis() as u64;

            // Use best version if repair didn't fully pass
            if mod_unit.state != MigrationState::Validated {
                if let Some(best) = best_version {
                    mod_unit.rust_output = Some(best);
                    mod_unit.idiomatic_score = Some(best_score);
                }
                if !best_compiles {
                    all_validated = false;
                    warn!(module = %mod_name, "module did not reach Validated state");
                }
            }
        }

        // Accumulate this module's output for context
        if let Some(ref rust_output) = mod_unit.rust_output {
            let sigs = noricum_tools::ast::extract_rust_signatures(rust_output);
            if !sigs.is_empty() {
                accumulated_rust_context.push_str(&sigs.join("\n"));
                accumulated_rust_context.push('\n');
            }

            module_outputs.push((module.name.clone(), rust_output.clone()));
            any_succeeded = true;
            best_combined_score += mod_unit.idiomatic_score.unwrap_or(0);

            info!(
                module = %mod_name,
                state = ?mod_unit.state,
                score = mod_unit.idiomatic_score.unwrap_or(0),
                "module migration complete"
            );
        }

        check_budget(config, &total_metrics)?;
    }

    if !any_succeeded {
        warn!(function = %name, "no modules produced output, falling back to chunked");
        return Ok(ModularResult::FallbackToChunked);
    }

    // Assemble final output from all module outputs
    let combined = assemble_module_outputs(&module_outputs);
    let avg_score = if !module_outputs.is_empty() {
        best_combined_score / module_outputs.len() as u32
    } else {
        0
    };

    info!(
        function = %name,
        modules_total = modules.len(),
        modules_succeeded = module_outputs.len(),
        all_validated,
        avg_score,
        "P3: modular migration complete"
    );

    Ok(ModularResult::Success {
        rust_code: combined,
        metrics: total_metrics,
        all_validated,
    })
}

/// Result of modular migration attempt.
enum ModularResult {
    /// All modules migrated successfully.
    Success {
        rust_code: String,
        metrics: noricum_ir::MigrationMetrics,
        all_validated: bool,
    },
    /// Modular split was not useful, fall back to chunked translation.
    FallbackToChunked,
}

/// Assemble the final Rust output from individually migrated module outputs.
///
/// Deduplicates `use` statements and joins module code in order.
fn assemble_module_outputs(modules: &[(String, String)]) -> String {
    let mut all_uses: Vec<String> = Vec::new();
    let mut code_parts: Vec<String> = Vec::new();

    for (mod_name, rust_code) in modules {
        let mut mod_code_lines = Vec::new();
        for line in rust_code.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("use ") && trimmed.ends_with(';') {
                if !all_uses.contains(&trimmed.to_string()) {
                    all_uses.push(trimmed.to_string());
                }
            } else {
                mod_code_lines.push(line);
            }
        }
        // Remove leading/trailing empty lines
        while mod_code_lines.first().is_some_and(|l| l.trim().is_empty()) {
            mod_code_lines.remove(0);
        }
        while mod_code_lines.last().is_some_and(|l| l.trim().is_empty()) {
            mod_code_lines.pop();
        }
        if !mod_code_lines.is_empty() {
            code_parts.push(format!(
                "// --- Module: {} ---\n{}",
                mod_name,
                mod_code_lines.join("\n")
            ));
        }
    }

    let mut output = String::new();
    if !all_uses.is_empty() {
        all_uses.sort();
        output.push_str(&all_uses.join("\n"));
        output.push_str("\n\n");
    }
    output.push_str(&code_parts.join("\n\n"));
    output.push('\n');
    output
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
            let sigs = noricum_tools::ast::extract_rust_signatures(rust_output);
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
        assert!(
            matches!(
                unit.state,
                MigrationState::Extracted | MigrationState::Repairing(_)
            ),
            "complex function should stay Extracted or enter Repairing, got {:?}",
            unit.state
        );
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
        let sigs = noricum_tools::ast::extract_rust_signatures(rust);
        assert_eq!(sigs.len(), 3);
        assert!(sigs[0].contains("pub fn add(a: i32, b: i32) -> i32"));
        assert!(sigs[1].contains("fn helper(x: i32) -> i32"));
        assert!(sigs[2].contains("fn main()"));
    }

    #[test]
    fn test_extract_rust_signatures_no_functions() {
        let rust = "let x = 5;\nstruct Foo { bar: i32 }";
        let sigs = noricum_tools::ast::extract_rust_signatures(rust);
        assert!(sigs.is_empty());
    }

    #[test]
    fn test_effective_small() {
        assert_eq!(effective_repair_iterations(5, 500, false), 5);
    }

    #[test]
    fn test_effective_large() {
        assert_eq!(effective_repair_iterations(5, 1500, false), 3);
    }

    #[test]
    fn test_effective_very_large() {
        assert_eq!(effective_repair_iterations(5, 3000, false), 2);
    }

    #[test]
    fn test_effective_already_low() {
        assert_eq!(effective_repair_iterations(1, 3000, false), 1);
    }

    #[test]
    fn test_effective_boundary_1000() {
        // 1000 is NOT > LARGE_FILE_LOC (1000), so no reduction
        assert_eq!(effective_repair_iterations(5, 1000, false), 5);
    }

    #[test]
    fn test_effective_boundary_2001() {
        // 2001 > VERY_LARGE_FILE_LOC (2000), so min(5, 2) = 2
        assert_eq!(effective_repair_iterations(5, 2001, false), 2);
    }

    #[test]
    fn test_effective_chunked_gets_minimum_8() {
        // Chunked translations get at least 8 iterations (max of configured, 8)
        assert_eq!(effective_repair_iterations(5, 1500, true), 8);
        assert_eq!(effective_repair_iterations(5, 3000, true), 8);
        assert_eq!(effective_repair_iterations(3, 5000, true), 8);
        // If configured is higher than 8, use configured
        assert_eq!(effective_repair_iterations(10, 1500, true), 10);
    }

    #[test]
    fn test_medium_file_triggers_chunking() {
        // Files >800 LOC should trigger chunked translation
        assert!(800 < LARGE_FILE_LOC);
        assert!(MEDIUM_FILE_LOC == 800);
        // 900 LOC > MEDIUM_FILE_LOC, so chunking is used
        let c_lines = 900;
        assert!(c_lines > MEDIUM_FILE_LOC);
        // With was_chunked=true, minimum 8 iterations
        assert_eq!(effective_repair_iterations(5, c_lines, true), 8);
    }

    #[test]
    fn test_stall_threshold_constant() {
        assert_eq!(
            STALL_THRESHOLD, 2,
            "stall triggers after 2 unchanged iterations"
        );
    }

    #[test]
    fn test_cache_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        // Run test in the temp directory so .noricum-cache is isolated
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let src = "int cache_test_unique_123() { return 42; }";

        // Miss
        assert!(cache::get(src).is_none(), "should miss on empty cache");

        // Put + Hit
        cache::put(src, "fn cache_test_unique_123() -> i32 { 42 }");
        let hit = cache::get(src);
        assert!(hit.is_some(), "should hit after put");
        assert_eq!(hit.unwrap(), "fn cache_test_unique_123() -> i32 { 42 }");

        // Different key = miss
        assert!(cache::get("int other() { return 0; }").is_none());

        // Empty value = miss (cache::get skips empty)
        cache::put("int empty_val() {}", "");
        assert!(cache::get("int empty_val() {}").is_none(), "empty = miss");

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_migration_config_skip_c2rust_default() {
        let config = MigrationConfig::default();
        assert!(!config.skip_c2rust, "skip_c2rust should default to false");
    }

    #[test]
    fn test_migration_config_skip_c2rust_set() {
        let config = MigrationConfig {
            skip_c2rust: true,
            ..Default::default()
        };
        assert!(config.skip_c2rust);
    }

    #[test]
    fn test_modular_threshold_constant() {
        assert_eq!(
            MODULAR_FILE_LOC, 2000,
            "modular migration triggers at 2000 LOC"
        );
        assert!(MODULAR_FILE_LOC > MEDIUM_FILE_LOC);
    }

    #[test]
    fn test_assemble_module_outputs_dedup_uses() {
        let modules = vec![
            (
                "utils".to_string(),
                "use std::collections::HashMap;\n\nfn util_a() -> i32 { 1 }\n".to_string(),
            ),
            (
                "core".to_string(),
                "use std::collections::HashMap;\nuse std::io;\n\nfn core_b() -> i32 { 2 }\n"
                    .to_string(),
            ),
        ];
        let result = assemble_module_outputs(&modules);
        // Each use should appear exactly once
        assert_eq!(
            result.matches("use std::collections::HashMap;").count(),
            1,
            "HashMap use should be deduplicated"
        );
        assert!(result.contains("use std::io;"));
        assert!(result.contains("fn util_a()"));
        assert!(result.contains("fn core_b()"));
        assert!(result.contains("// --- Module: utils ---"));
        assert!(result.contains("// --- Module: core ---"));
    }

    #[test]
    fn test_assemble_module_outputs_empty() {
        let modules: Vec<(String, String)> = vec![];
        let result = assemble_module_outputs(&modules);
        assert_eq!(result.trim(), "");
    }

    #[test]
    fn test_assemble_module_outputs_preserves_order() {
        let modules = vec![
            ("first".to_string(), "fn a() {}\n".to_string()),
            ("second".to_string(), "fn b() {}\n".to_string()),
            ("third".to_string(), "fn c() {}\n".to_string()),
        ];
        let result = assemble_module_outputs(&modules);
        let pos_a = result.find("Module: first").unwrap();
        let pos_b = result.find("Module: second").unwrap();
        let pos_c = result.find("Module: third").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }
}
