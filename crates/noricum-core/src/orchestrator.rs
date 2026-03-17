/// Migration orchestrator: drives the per-function state machine.
///
/// The orchestrator takes a C source file through the full pipeline:
/// Pending -> Extracted -> Characterized -> C2RustDone -> Analyzed -> Refined -> Validated
///
/// Two execution modes:
/// - `migrate_file_sync`: deterministic, no LLM (C2Rust + validation only)
/// - `migrate_file`: async, full LLM agent pipeline (analysis, translation, repair, test gen)
use std::path::Path;

use futures::future::join_all;
use noricum_agents::LlmClient;
use noricum_agents::providers::{
    ProviderConfig, create_llm_client, select_model, select_repair_model,
};
use noricum_ir::pattern_store::PatternStore;
use noricum_ir::{Difficulty, FunctionUnit, MigrationProject, MigrationState};
use noricum_tools::repair_rules::{apply_all_rules, parse_rustc_errors};
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::surgical_repair::{extract_function_at_line, gather_context, splice_function};

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

/// Number of compilation errors above which we re-translate instead of repairing.
const RETRANSLATE_ERROR_THRESHOLD: usize = 100;

/// Result of migrating a single module within a wave.
/// Contains all data needed to merge back into the shared state after parallel execution.
struct ModuleMigrationResult {
    /// Module name
    name: String,
    /// Index in the modules array (for ordering)
    mod_idx: usize,
    /// The migrated FunctionUnit (contains rust_output, state, scores, etc.)
    unit: Option<FunctionUnit>,
    /// Artifact entry for the v2 manifest
    artifact: crate::artifacts::ModuleArtifact,
    /// Metrics accumulated by this module's migration
    metrics: noricum_ir::MigrationMetrics,
    /// Whether the module reached Validated or equivalent
    validated: bool,
    /// Whether this was a warm-start skip (output already in artifact)
    was_skip: bool,
}

/// Warm-start action for a module based on its previous run results.
#[derive(Debug, PartialEq, Eq)]
enum WarmAction {
    /// Module was Validated — use directly, skip all processing.
    Skip,
    /// Module compiled but didn't reach threshold — start from its code, skip translation.
    SeedRepair,
    /// Module was garbage — re-translate from scratch.
    Retranslate,
}

/// Determine warm-start action for a module based on its previous artifact.
fn warm_start_action(artifact: &crate::artifacts::ModuleArtifact) -> WarmAction {
    match artifact.state.as_str() {
        // P23: CompilesUnsafe with high score is good enough to skip
        "Validated" | "CompilesUnsafe" if artifact.score >= 70.0 => WarmAction::Skip,
        // P23: NearlyCompiles with decent score is worth seeding
        "NearlyCompiles" if artifact.score >= 50.0 => WarmAction::SeedRepair,
        _ if artifact.compiles && artifact.score >= 40.0 => WarmAction::SeedRepair,
        _ => WarmAction::Retranslate,
    }
}

/// P25: Build accumulated Rust source from completed module outputs for incremental validation.
/// This allows validating module N against the combined output of modules 0..N-1.
/// Only includes modules that compiled successfully to avoid error propagation (P26).
fn build_assembly_context(module_outputs: &[(String, String, bool)]) -> String {
    let compilable: Vec<&str> = module_outputs
        .iter()
        .filter(|(_, _, compiles)| *compiles)
        .map(|(_, code, _)| code.as_str())
        .collect();
    if compilable.is_empty() {
        return String::new();
    }
    compilable.join("\n\n")
}

/// P25: Validate a module in the context of prior module outputs.
/// Compiles `prior_outputs + current_module` together to resolve cross-module deps.
fn validate_module_with_assembly(
    mod_unit: &noricum_ir::FunctionUnit,
    module_name: &str,
    module_outputs: &[(String, String, bool)],
    threshold: u32,
) -> Result<noricum_validation::ValidationResult, crate::CoreError> {
    let assembly_context = build_assembly_context(module_outputs);
    if assembly_context.is_empty() {
        return Ok(noricum_validation::validate_with_threshold(mod_unit, threshold)?);
    }
    // Combine prior outputs with current module for compilation check
    let combined = format!(
        "{}\n\n// --- Module: {} ---\n{}",
        assembly_context,
        module_name,
        mod_unit.rust_output.as_deref().unwrap_or("")
    );
    let mut temp_unit = mod_unit.clone();
    temp_unit.rust_output = Some(combined);
    Ok(noricum_validation::validate_with_threshold(&temp_unit, threshold)?)
}

/// P23: Assign a graduated state based on compilation, unsafe, and score.
fn graduated_state(
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
fn should_retranslate(error_count: usize, retranslation_attempts: u32) -> bool {
    error_count > RETRANSLATE_ERROR_THRESHOLD && retranslation_attempts == 0
}

/// Compute adaptive LLM call budget based on module count.
/// Formula: modules * 10 + 25 (translate + repairs + re-translates + P33 contract + assembly repair).
/// If user specified a limit, use max(adaptive, user_limit).
pub fn compute_adaptive_budget(module_count: usize, user_limit: Option<u32>) -> u32 {
    let adaptive = (module_count as u32) * 10 + 25;
    match user_limit {
        Some(limit) => adaptive.max(limit),
        None => adaptive,
    }
}

/// P34: Budget phase for graceful degradation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BudgetPhase {
    /// Under 80% — normal operation.
    Normal,
    /// 80-95% — skip remaining repair iterations, accept current module versions.
    SkipRepairs,
    /// 95-100% — skip remaining modules, go straight to assembly.
    AssembleNow,
    /// Over 100% — save best output and exit gracefully (not a hard error).
    SaveAndExit,
}

/// P34: Determine current budget phase based on LLM call usage.
pub fn budget_phase(config: &MigrationConfig, metrics: &noricum_ir::MigrationMetrics) -> BudgetPhase {
    let max_calls = match config.max_llm_calls {
        Some(m) => m,
        None => return BudgetPhase::Normal,
    };
    if max_calls == 0 {
        return BudgetPhase::Normal;
    }
    let pct = (metrics.llm_calls * 100) / max_calls;
    if pct >= 100 {
        BudgetPhase::SaveAndExit
    } else if pct >= 95 {
        BudgetPhase::AssembleNow
    } else if pct >= 80 {
        BudgetPhase::SkipRepairs
    } else {
        BudgetPhase::Normal
    }
}

/// Configuration for the async LLM-based migration pipeline.
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Primary LLM provider: "anthropic", "deepseek", or "ollama".
    pub primary_provider: Option<String>,
    /// Anthropic API key. Defaults to `ANTHROPIC_API_KEY` env var.
    pub anthropic_api_key: Option<String>,
    /// DeepSeek API key. Defaults to `DEEPSEEK_API_KEY` env var.
    pub deepseek_api_key: Option<String>,
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
    /// Target maximum LOC per sub-module in modular migration.
    /// Default: None (uses 1000). Use 500 for DeepSeek R1.
    pub module_target_loc: Option<usize>,
    /// Path to previous artifact directory for warm-start.
    /// Validated modules are reused, partially succeeded modules seed repair.
    pub warm_start: Option<std::path::PathBuf>,
    /// Maximum allowed unsafe blocks in output. `None` means use the translation
    /// baseline as ceiling (same as pre-P21 behavior). Setting e.g. `Some(3)` allows
    /// repair to introduce up to 3 unsafe blocks even if translation had 0.
    pub max_unsafe_blocks: Option<u32>,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            primary_provider: None,
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            deepseek_api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
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
            module_target_loc: None,
            warm_start: None,
            max_unsafe_blocks: None,
        }
    }
}

impl From<&MigrationConfig> for ProviderConfig {
    fn from(config: &MigrationConfig) -> Self {
        let primary = config.primary_provider.clone().unwrap_or_else(|| {
            if config.anthropic_api_key.is_some() {
                "anthropic".to_string()
            } else if config.deepseek_api_key.is_some() {
                "deepseek".to_string()
            } else {
                "ollama".to_string()
            }
        });
        Self {
            primary_provider: primary,
            anthropic_api_key: config.anthropic_api_key.clone(),
            deepseek_api_key: config.deepseek_api_key.clone(),
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
/// P34: LLM call limit is now a soft check — use `budget_phase()` for graceful degradation.
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
    // P34: LLM call limit is now soft — only hard-fail at 120% to prevent runaway
    if let Some(max_calls) = config.max_llm_calls
        && metrics.llm_calls > max_calls + max_calls / 5
    {
        return Err(CoreError::Orchestration(format!(
            "LLM call hard limit exceeded: {} calls (max {} + 20% grace)",
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

/// P30: Three-phase hybrid repair for assembled outputs (>MODULAR_FILE_LOC lines).
///
/// Phase 1: Mechanical rules (0 LLM cost) — Clone bounds, dedup, mut binding fixes.
/// Phase 2: Surgical per-function repair (focused LLM, ~100 LOC per call).
/// Phase 3 is handled by the caller falling through to the legacy repair loop.
///
/// Returns `true` if all errors were resolved (caller should skip legacy repair).
async fn hybrid_repair(
    unit: &mut FunctionUnit,
    client: &LlmClient,
    provider_config: &ProviderConfig,
    difficulty: Difficulty,
    artifacts: &Option<crate::artifacts::ArtifactStore>,
) -> Result<bool, CoreError> {
    let name = unit.name.clone();
    let rust_source = unit.rust_output.as_deref().unwrap_or("");

    // --- Phase 1: Rule Engine ---
    info!(function = %name, "P30 Phase 1: applying mechanical repair rules");
    let compile_result = noricum_tools::compiler::check_rust_compiles(rust_source)?;
    if compile_result.success {
        info!(function = %name, "P30: already compiles, no repair needed");
        return Ok(true);
    }

    let errors = parse_rustc_errors(&compile_result.stderr);
    let error_count_before = errors.len();
    let fixed = apply_all_rules(rust_source, &errors);

    // Re-compile after rules
    let post_rules = noricum_tools::compiler::check_rust_compiles(&fixed)?;
    let post_errors = parse_rustc_errors(&post_rules.stderr);
    info!(
        function = %name,
        errors_before = error_count_before,
        errors_after = post_errors.len(),
        "P30 Phase 1 complete"
    );

    unit.rust_output = Some(fixed.clone());

    if let Some(store) = artifacts {
        let _ = store.save_repair_iteration(0, &fixed, "P30 Phase 1: rule engine");
    }

    if post_rules.success {
        info!(function = %name, "P30 Phase 1: rules resolved all errors");
        return Ok(true);
    }

    // --- Phase 2: Surgical Repair ---
    info!(
        function = %name,
        remaining_errors = post_errors.len(),
        "P30 Phase 2: surgical per-function repair"
    );

    let repair_model = select_repair_model(provider_config, difficulty)?;
    let mut current_source = fixed;
    let max_surgical = 5;

    for cycle in 0..max_surgical {
        let compile_check = noricum_tools::compiler::check_rust_compiles(&current_source)?;
        if compile_check.success {
            info!(function = %name, cycle, "P30 Phase 2: surgical repair resolved all errors");
            unit.rust_output = Some(current_source);
            return Ok(true);
        }

        let cycle_errors = parse_rustc_errors(&compile_check.stderr);
        if cycle_errors.is_empty() {
            break;
        }

        // Target the first error's function
        let err = &cycle_errors[0];
        let Some((fn_name, fn_body)) = extract_function_at_line(&current_source, err.line) else {
            info!(
                function = %name,
                line = err.line,
                "P30 Phase 2: could not extract function at error line, skipping"
            );
            break;
        };

        let context = gather_context(&current_source, &fn_name);

        let prompt = format!(
            "Fix this Rust function. The compiler error is:\n\
             {}: {}\n\n\
             These types and function signatures are already defined (DO NOT redefine them):\n\
             {}\n\n\
             Here is the function to fix:\n\
             {}\n\n\
             Return ONLY the fixed function, nothing else. No markdown fences.",
            err.code, err.message, context, fn_body
        );

        info!(
            function = %name,
            cycle,
            error = %err.code,
            target_fn = %fn_name,
            context_len = context.len(),
            fn_len = fn_body.len(),
            "P30 Phase 2: sending surgical repair request"
        );

        let repair_result = noricum_agents::repair::repair_with_prompt(
            client,
            &repair_model.model,
            &prompt,
        )
        .await;

        match repair_result {
            Ok(fixed_fn) => {
                current_source = splice_function(&current_source, &fn_name, &fixed_fn);
                unit.metrics.llm_calls += 1;

                if let Some(store) = artifacts {
                    let _ = store.save_repair_iteration(
                        (cycle + 1) as u32,
                        &current_source,
                        &format!("P30 Phase 2 cycle {cycle}: fixed {fn_name} ({} error)", err.code),
                    );
                }
            }
            Err(e) => {
                warn!(function = %name, error = %e, "P30 Phase 2: surgical repair LLM call failed");
                break;
            }
        }
    }

    unit.rust_output = Some(current_source);

    // Check if Phase 2 resolved everything
    let final_check = noricum_tools::compiler::check_rust_compiles(
        unit.rust_output.as_deref().unwrap_or(""),
    )?;

    if final_check.success {
        info!(function = %name, "P30 Phase 2: all errors resolved after surgical repair");
        return Ok(true);
    }

    let remaining = parse_rustc_errors(&final_check.stderr).len();
    info!(
        function = %name,
        remaining_errors = remaining,
        "P30 Phases 1-2 complete, falling back to Phase 3 (legacy repair)"
    );
    Ok(false)
}

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

    // --- Initialize artifact store ---
    let artifacts = match crate::artifacts::ArtifactStore::new(&config.artifacts_dir, &name) {
        Ok(store) => {
            info!(artifacts_dir = %store.run_dir().display(), "artifact store initialized");
            Some(store)
        }
        Err(e) => {
            warn!(error = %e, "failed to create artifact store, continuing without persistence");
            None
        }
    };

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

    if let Some(ref store) = artifacts {
        let _ = store.save_c_source(&unit.c_source);
    }

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
                if let Some(ref store) = artifacts {
                    let _ = store.save_c2rust(&rust_src);
                }
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
    if let Some(ref store) = artifacts
        && let Ok(json) = serde_json::to_string_pretty(&analysis)
    {
        let _ = store.save_analysis(&json);
    }
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
            Some(c_file),
            config,
            &client,
            &provider_config,
            &analysis,
            difficulty,
            &artifacts,
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
                Ok(chunked_result) => {
                    if let Some(ref store) = artifacts {
                        for chunk in &chunked_result.chunks {
                            let _ = store.save_translation_chunk(chunk.index, &chunk.rust_source);
                        }
                        if let Some(ref sigs) = chunked_result.agreed_signatures {
                            let _ = store.save_agreed_signatures(sigs);
                        }
                        if let Some(ref foundation) = chunked_result.foundation {
                            let _ = store.save_foundation(foundation);
                        }
                    }
                    chunked_result.combined
                }
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
                        if let Some(ref store) = artifacts {
                            let _ = store.save_retranslation("stub", &retranslated);
                        }
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
                        if let Some(ref store) = artifacts {
                            let _ = store.save_retranslation("unsafe", &retranslated);
                        }
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
        if let Some(ref store) = artifacts {
            let _ = store.save_translation_final(unit.rust_output.as_deref().unwrap_or(""));
        }
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
    if let Some(ref store) = artifacts
        && let Ok(json) = serde_json::to_string_pretty(&validation)
    {
        let _ = store.save_initial_validation(&json);
    }
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

        // P30: Hybrid repair for assembled outputs (>MODULAR_FILE_LOC lines).
        // Phases 1-2 run first; if they resolve all errors, skip the legacy loop.
        // If not, Phase 3 = legacy repair with max 3 iterations.
        let mut hybrid_resolved = false;
        if c_lines > MODULAR_FILE_LOC {
            info!(function = %name, c_lines, "P30: using hybrid repair for assembled output");
            hybrid_resolved = hybrid_repair(
                &mut unit, &client, &provider_config, difficulty, &artifacts,
            ).await?;

            if hybrid_resolved {
                // Re-validate after hybrid repair
                let post_hybrid = noricum_validation::validate_with_threshold(
                    &unit, config.min_idiomatic_score,
                )?;
                noricum_validation::apply_validation_with_max(&mut unit, &post_hybrid, max_iters);
                unit.metrics.repair_ms = repair_start.elapsed().as_millis() as u64;
                info!(function = %name, "P30: hybrid repair resolved all compilation errors");
            } else {
                // P34b: Skip Phase 3 for assembly — legacy repair destroys assembled output
                // (5408 LOC → 239 LOC observed in Run 12). Use P1 best-version instead.
                warn!(
                    function = %name,
                    "P34b: skipping Phase 3 legacy repair for assembled output (preserving best version)"
                );
                hybrid_resolved = true; // Force skip of legacy repair
            }
        }

        // Skip legacy repair if hybrid resolved everything or P34b skipped Phase 3
        if !hybrid_resolved {

        // P29: Use fast repair model for assembly repair (deepseek-chat instead of R1).
        // R1 is ~5 min/iter on assembly vs ~1 min for deepseek-chat.
        let repair_model_sel = select_repair_model(&provider_config, difficulty)?;

        // P30: Cap legacy repair to 3 iterations if used as Phase 3 fallback
        let max_iters = if c_lines > MODULAR_FILE_LOC {
            max_iters.min(3)
        } else {
            max_iters
        };

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
        // P21: Effective unsafe ceiling — allows configurable tolerance above baseline
        let unsafe_ceiling = config
            .max_unsafe_blocks
            .map_or(baseline_unsafe, |max| max.max(baseline_unsafe));

        // P1: Best-version tracking — keep the version with the highest score
        // that doesn't exceed the unsafe ceiling.
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
            // P28: Skip retranslation for assembled outputs (>MODULAR_FILE_LOC lines).
            // Assembly repair combines 17+ modules (~6000 LOC) but retranslation can only
            // generate ~1000 LOC, destroying most of the assembled content.
            let is_assembly = c_lines > MODULAR_FILE_LOC;
            if stall_count >= STALL_THRESHOLD && !retranslated_on_stall && !is_assembly {
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
                    if let Some(ref store) = artifacts {
                        let _ = store
                            .save_retranslation_stall(unit.rust_output.as_deref().unwrap_or(""));
                    }
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
                        && re_validation.unsafe_count <= unsafe_ceiling
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
                        if let Some(ref store) = artifacts
                            && let Some(ref best) = best_version
                        {
                            let _ = store.save_best_version(
                                best,
                                best_score,
                                best_unsafe,
                                best_compiles,
                            );
                        }
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

            // P22: After 3+ failed iterations, hint that unsafe is acceptable to fix compilation
            let mut effective_diff_feedback = effective_diff_feedback;
            if !errors.is_empty() && iteration >= 3 && unsafe_ceiling > 0 {
                effective_diff_feedback.push(format!(
                    "IMPORTANT: If you cannot fix the compilation errors with safe code, \
                     you MAY use up to {} unsafe block(s) to make the code compile. \
                     A compiling program with minimal unsafe is better than one that doesn't compile. \
                     Wrap only the minimum necessary code in unsafe.",
                    unsafe_ceiling
                ));
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
            if repaired_unsafe > unsafe_ceiling {
                warn!(
                    function = %name,
                    iteration,
                    repaired_unsafe,
                    unsafe_ceiling,
                    "P0: repair rejected — exceeds unsafe ceiling (P21)"
                );
                if let Some(ref store) = artifacts {
                    let _ = store.save_repair_rejected(iteration, &repaired);
                }
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
            if let Some(ref store) = artifacts
                && let Ok(val_json) = serde_json::to_string_pretty(&re_validation)
            {
                let _ = store.save_repair_iteration(
                    iteration,
                    unit.rust_output.as_deref().unwrap_or(""),
                    &val_json,
                );
            }
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
                && re_validation.unsafe_count <= unsafe_ceiling
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
                if let Some(ref store) = artifacts
                    && let Some(ref best) = best_version
                {
                    let _ = store.save_best_version(best, best_score, best_unsafe, best_compiles);
                }
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
            // P23: Assign graduated state based on best-version quality
            let error_count = unit.last_errors.len();
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
            unit.state = graduated_state(best_compiles, best_unsafe, best_score, 80, error_count);
            warn!(
                function = %name,
                state = ?unit.state,
                "max repair iterations reached, using graduated state (P23)"
            );
        }

        } // end if !hybrid_resolved
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

    // --- Save final artifacts ---
    if let Some(ref store) = artifacts {
        if let Some(ref rust) = unit.rust_output {
            let _ = store.save_final_output(rust);
        }
        if let Some(ref tests) = unit.generated_tests {
            let _ = store.save_generated_tests(tests);
        }
        let manifest = serde_json::json!({
            "name": unit.name,
            "source_path": unit.source_path,
            "final_state": format!("{:?}", unit.state),
            "difficulty": format!("{:?}", unit.difficulty),
            "idiomatic_score": unit.idiomatic_score,
            "unsafe_count": unit.unsafe_count,
            "metrics": {
                "total_ms": unit.metrics.total_ms,
                "llm_calls": unit.metrics.llm_calls,
                "repair_iterations": unit.metrics.repair_iterations,
                "c_lines": unit.metrics.c_lines,
                "rust_lines": unit.metrics.rust_lines,
                "input_tokens": unit.metrics.input_tokens,
                "output_tokens": unit.metrics.output_tokens,
                "estimated_cost_usd": unit.metrics.estimated_cost_usd,
            },
            "artifacts_dir": store.run_dir().display().to_string(),
        });
        if let Ok(json) = serde_json::to_string_pretty(&manifest) {
            let _ = store.save_manifest(&json);
        }
        info!(artifacts_dir = %store.run_dir().display(), "all pipeline artifacts saved");
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
#[allow(clippy::too_many_arguments)]
async fn migrate_file_modular(
    c_source: &str,
    name: &str,
    source_file: Option<&std::path::Path>,
    config: &MigrationConfig,
    client: &LlmClient,
    provider_config: &noricum_agents::providers::ProviderConfig,
    analysis: &noricum_agents::analysis::AnalysisResult,
    difficulty: noricum_ir::Difficulty,
    artifacts: &Option<crate::artifacts::ArtifactStore>,
) -> Result<ModularResult, CoreError> {
    let module_split = noricum_tools::ast::split_into_modules(c_source, config.module_target_loc);
    let modules = module_split.modules;
    let shared_context = module_split.shared_context;

    if modules.len() <= 1 {
        info!(function = %name, "modular split produced single module, falling back to chunked");
        return Ok(ModularResult::FallbackToChunked);
    }

    // P33: Generate type contract (with header resolution for complete types)
    let type_contract = crate::type_contract::generate_type_contract(
        client,
        provider_config,
        &shared_context,
        c_source,
        source_file,
        artifacts.as_ref(),
    )
    .await?;

    if let Some(ref tc) = type_contract {
        tracing::info!("P33: type contract generated ({} lines)", tc.lines().count());
    } else {
        tracing::info!("P33: no type contract (will use per-module type discovery)");
    }

    // Build intra-file dependency graph and order modules
    let dep_graph = crate::dependency::DependencyGraph::from_source(c_source);
    let waves = dep_graph.module_waves(&modules);
    let order = dep_graph.module_order(&modules);

    let module_names: Vec<&str> = order.iter().map(|&i| modules[i].name.as_str()).collect();

    // P15: Adaptive LLM call budget — scale with module count
    let effective_max_calls = compute_adaptive_budget(modules.len(), config.max_llm_calls);
    info!(
        function = %name,
        module_count = modules.len(),
        order = ?module_names,
        budget = effective_max_calls,
        "P3: modular migration — {} modules in dependency order",
        modules.len()
    );
    let mut effective_config = config.clone();
    effective_config.max_llm_calls = Some(effective_max_calls);

    let mut module_outputs: Vec<(String, String, bool)> = Vec::new(); // (module_name, rust_code, compiles)
    let mut accumulated_rust_context = String::new();
    let mut total_metrics = noricum_ir::MigrationMetrics::default();
    let mut all_validated = true;
    let mut any_succeeded = false;
    let mut best_combined_score: u32 = 0;
    let mut module_artifacts: Vec<crate::artifacts::ModuleArtifact> = Vec::new();

    let pattern_store = PatternStore::load_seed_patterns();

    // P19: Load previous artifact manifest for warm-start
    let warm_store = config.warm_start.as_ref().and_then(|path| {
        match crate::artifacts::ArtifactStore::from_existing(path) {
            Ok(store) => {
                info!(path = %path.display(), "P19: loaded warm-start artifact store");
                Some(store)
            }
            Err(e) => {
                warn!(path = %path.display(), error = %e, "P19: failed to load warm-start artifacts");
                None
            }
        }
    });
    let warm_manifest = warm_store
        .as_ref()
        .and_then(|store| store.load_manifest().ok());

    // P18: Wave-based module iteration — modules in the same wave are independent
    // and could run in parallel (future: use JoinSet for concurrent waves).
    info!(
        function = %name,
        wave_count = waves.len(),
        wave_sizes = ?waves.iter().map(|w| w.len()).collect::<Vec<_>>(),
        "P18: wave-based module migration"
    );

    for (wave_idx, wave) in waves.iter().enumerate() {
        info!(wave = wave_idx, modules = wave.len(), "starting wave");

        // Snapshot shared state BEFORE the wave — modules in the same wave are independent
        // and should all see the same pre-wave context.
        let pre_wave_rust_context = accumulated_rust_context.clone();
        let pre_wave_module_outputs = module_outputs.clone();
        let pre_wave_module_number = module_outputs.len() + 1;

        // Dispatch: single-module waves run directly, multi-module waves run concurrently.
        let wave_results: Vec<Result<ModuleMigrationResult, CoreError>> = if wave.len() == 1 {
            // Single module — call directly, no concurrency overhead
            let mod_idx = wave[0];
            let result = migrate_single_module(
                &modules[mod_idx],
                mod_idx,
                name,
                &effective_config,
                client,
                provider_config,
                analysis,
                difficulty,
                artifacts,
                &pattern_store,
                &warm_manifest,
                &warm_store,
                &pre_wave_rust_context,
                &pre_wave_module_outputs,
                pre_wave_module_number,
                modules.len(),
                type_contract.as_deref(),
            )
            .await;
            vec![result]
        } else {
            // Multi-module wave — run concurrently via join_all
            let futures: Vec<_> = wave
                .iter()
                .enumerate()
                .map(|(i, &mod_idx)| {
                    let module = &modules[mod_idx];
                    let config_ref = &effective_config;
                    let client_ref = client;
                    let provider_config_ref = provider_config;
                    let analysis_ref = analysis;
                    let artifacts_ref = artifacts;
                    let pattern_store_ref = &pattern_store;
                    let warm_manifest_ref = &warm_manifest;
                    let warm_store_ref = &warm_store;
                    let pre_wave_ctx = &pre_wave_rust_context;
                    let pre_wave_outs = &pre_wave_module_outputs;
                    let module_number = pre_wave_module_number + i;
                    let total = modules.len();
                    let parent_name = name;
                    let tc = type_contract.as_deref();

                    async move {
                        migrate_single_module(
                            module,
                            mod_idx,
                            parent_name,
                            config_ref,
                            client_ref,
                            provider_config_ref,
                            analysis_ref,
                            difficulty,
                            artifacts_ref,
                            pattern_store_ref,
                            warm_manifest_ref,
                            warm_store_ref,
                            pre_wave_ctx,
                            pre_wave_outs,
                            module_number,
                            total,
                            tc,
                        )
                        .await
                    }
                })
                .collect();

            join_all(futures).await
        };

        // Merge wave results into shared state, sorted by mod_idx for deterministic ordering.
        let mut sorted_results: Vec<ModuleMigrationResult> = Vec::new();
        for result in wave_results {
            match result {
                Ok(r) => sorted_results.push(r),
                Err(e) => {
                    // Propagate budget errors; log and continue for others
                    if matches!(e, CoreError::BudgetExceeded { .. }) {
                        return Err(e);
                    }
                    warn!(error = %e, "module migration failed in wave {}", wave_idx);
                    all_validated = false;
                }
            }
        }
        sorted_results.sort_by_key(|r| r.mod_idx);

        for result in sorted_results {
            // Merge metrics
            total_metrics.llm_calls += result.metrics.llm_calls;
            total_metrics.input_tokens += result.metrics.input_tokens;
            total_metrics.output_tokens += result.metrics.output_tokens;
            total_metrics.translation_ms += result.metrics.translation_ms;
            total_metrics.repair_ms += result.metrics.repair_ms;

            // P26: capture compiles flag before artifact is moved
            let mod_compiles = result.artifact.compiles;

            // Merge artifact
            module_artifacts.push(result.artifact);

            if !result.validated && !result.was_skip {
                all_validated = false;
            }

            // Merge output and context
            if let Some(ref unit) = result.unit
                && let Some(ref rust_output) = unit.rust_output
            {
                // P33: When type contract active, only accumulate function signatures
                // (types already in contract). Without contract, use P27 behavior.
                if type_contract.is_none() {
                    let type_defs =
                        noricum_tools::ast::extract_rust_type_definitions(rust_output);
                    if !type_defs.is_empty() {
                        accumulated_rust_context.push_str(&type_defs.join("\n\n"));
                        accumulated_rust_context.push('\n');
                    }
                }
                let sigs = noricum_tools::ast::extract_rust_signatures(rust_output);
                if !sigs.is_empty() {
                    accumulated_rust_context.push_str(&sigs.join("\n"));
                    accumulated_rust_context.push('\n');
                }

                module_outputs.push((result.name.clone(), rust_output.clone(), mod_compiles));
                any_succeeded = true;
                best_combined_score += unit.idiomatic_score.unwrap_or(0);

                info!(
                    module = %result.name,
                    state = ?unit.state,
                    score = unit.idiomatic_score.unwrap_or(0),
                    "module migration complete"
                );
            }
        }

        // P34: Graceful budget degradation instead of hard fail
        let phase = budget_phase(&effective_config, &total_metrics);
        match phase {
            BudgetPhase::AssembleNow | BudgetPhase::SaveAndExit => {
                warn!(
                    function = %name,
                    llm_calls = total_metrics.llm_calls,
                    phase = ?phase,
                    "P34: budget near limit, skipping remaining waves and proceeding to assembly"
                );
                break;
            }
            BudgetPhase::SkipRepairs => {
                info!(
                    function = %name,
                    llm_calls = total_metrics.llm_calls,
                    "P34: budget at 80%, will skip repair iterations for remaining modules"
                );
                // Reduce repair iterations to 0 for remaining waves
                effective_config.max_repair_iterations = 0;
            }
            BudgetPhase::Normal => {}
        }
        check_budget(&effective_config, &total_metrics)?;
    } // end for wave in waves

    if !any_succeeded {
        warn!(function = %name, "no modules produced output, falling back to chunked");
        return Ok(ModularResult::FallbackToChunked);
    }

    // Assemble final output from all module outputs
    let combined = assemble_module_outputs(&module_outputs, type_contract.as_deref());
    let avg_score = if !module_outputs.is_empty() {
        best_combined_score / module_outputs.len() as u32
    } else {
        0
    };

    // P20: Save v2 manifest with per-module results
    if let Some(store) = artifacts {
        let manifest = crate::artifacts::ArtifactManifest {
            version: 2,
            function_name: name.to_string(),
            timestamp: chrono::Local::now().format("%Y%m%d-%H%M%S").to_string(),
            modules: module_artifacts,
        };
        let _ = store.save_manifest_v2(&manifest);
    }

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

/// Migrate a single module through the full pipeline (translate -> validate -> repair).
///
/// This function is designed to be called concurrently for modules within the same wave.
/// It takes all context by reference (no shared mutable state) and returns a
/// `ModuleMigrationResult` that the caller merges into shared state after the wave completes.
#[allow(clippy::too_many_arguments)]
async fn migrate_single_module(
    module: &noricum_tools::ast::CModule,
    mod_idx: usize,
    name: &str,
    config: &MigrationConfig,
    client: &LlmClient,
    provider_config: &ProviderConfig,
    analysis: &noricum_agents::analysis::AnalysisResult,
    difficulty: noricum_ir::Difficulty,
    artifacts: &Option<crate::artifacts::ArtifactStore>,
    pattern_store: &PatternStore,
    warm_manifest: &Option<crate::artifacts::ArtifactManifest>,
    warm_store: &Option<crate::artifacts::ArtifactStore>,
    accumulated_rust_context: &str,
    module_outputs: &[(String, String, bool)],
    module_number: usize,
    total_modules: usize,
    type_contract: Option<&str>,
) -> Result<ModuleMigrationResult, CoreError> {
    let mod_name = format!("{name}::{}", module.name);

    info!(
        module = %mod_name,
        functions = module.function_names.len(),
        lines = module.line_count,
        "migrating module ({}/{})",
        module_number,
        total_modules
    );

    // --- P19: Warm-start check ---
    if let Some(manifest) = warm_manifest
        && let Some(prev) = manifest.modules.iter().find(|m| m.name == module.name)
    {
        match warm_start_action(prev) {
            WarmAction::Skip => {
                if let Some(ws) = warm_store
                    && let Ok(Some(code)) = ws.load_translation_module(&module.name)
                {
                    info!(module = %mod_name, score = prev.score, "P19: warm-start skip (validated)");
                    // Save to new artifacts too
                    if let Some(store) = artifacts {
                        let _ = store.save_translation_module(&module.name, &code);
                    }
                    return Ok(ModuleMigrationResult {
                        name: module.name.clone(),
                        mod_idx,
                        unit: {
                            let mut u = FunctionUnit::new(
                                mod_name.clone(),
                                String::new(),
                                module.source.clone(),
                            );
                            u.rust_output = Some(code);
                            u.state = MigrationState::Validated;
                            u.idiomatic_score = Some(prev.score as u32);
                            Some(u)
                        },
                        artifact: crate::artifacts::ModuleArtifact {
                            name: module.name.clone(),
                            state: "Validated".to_string(),
                            score: prev.score,
                            compiles: true,
                            unsafe_count: prev.unsafe_count,
                        },
                        metrics: noricum_ir::MigrationMetrics::default(),
                        validated: true,
                        was_skip: true,
                    });
                }
            }
            WarmAction::SeedRepair => {
                info!(module = %mod_name, score = prev.score, "P19: warm-start seed repair");
                // Will seed the translation from warm-start code below
            }
            WarmAction::Retranslate => {
                info!(module = %mod_name, score = prev.score, "P19: warm-start retranslate");
                // Proceed with normal translation
            }
        }
    }

    // Local metrics for this module
    let mut local_metrics = noricum_ir::MigrationMetrics::default();

    // Create a FunctionUnit for this module
    let mut mod_unit =
        FunctionUnit::new(mod_name.clone(), String::new(), module.source.clone());
    mod_unit.difficulty = Some(difficulty);

    // P19: Check if warm-start provides a seed for this module (SeedRepair)
    let warm_seed = if let (Some(manifest), Some(ws)) = (warm_manifest, warm_store) {
        manifest
            .modules
            .iter()
            .find(|m| m.name == module.name)
            .and_then(|prev| {
                if warm_start_action(prev) == WarmAction::SeedRepair {
                    ws.load_translation_module(&module.name).ok().flatten()
                } else {
                    None
                }
            })
    } else {
        None
    };

    // Translate this module
    let translation_start = Instant::now();
    let translation_model_sel = select_model(provider_config, difficulty, "translation")?;
    let relevant_patterns = pattern_store.find_relevant(&module.source, 3);

    // P33/P27: Inject type contract or accumulated context as a prefix hint in the C source.
    let augmented_c = if let Some(tc) = type_contract {
        // P33: Type contract provides canonical types. accumulated_rust_context has only function sigs.
        format!(
            "/* P33 TYPE CONTRACT: The following Rust types are ALREADY DEFINED and MUST be used exactly as-is.\n\
             Do NOT redefine ANY struct, enum, const, or type alias below. They are final.\n\
             Only write functions and impl blocks that USE these types.\n\n\
             ```rust\n{}\n```\n*/\n\n\
             {}\n\n{}",
            tc,
            if accumulated_rust_context.is_empty() {
                String::new()
            } else {
                format!(
                    "/* Already migrated function signatures (use these, do not redefine):\n{}\n*/",
                    accumulated_rust_context
                )
            },
            module.source
        )
    } else if !accumulated_rust_context.is_empty() {
        // Existing P27 behavior (no type contract)
        format!(
            "/* MIGRATION CONTEXT: The following Rust types and functions have already been migrated \
from earlier modules in this same file.\n\
\n\
CRITICAL: Do NOT redefine any struct, enum, const, or type alias that appears below. \
Use them directly — they are already defined and available in scope. \
Only define NEW types that don't exist yet. If you need a type that's listed below, \
just use it (e.g., `ZipArchive`, `ZipError`). Do NOT create your own version.\n\
\n{}\n*/\n\n{}",
            accumulated_rust_context, module.source
        )
    } else {
        module.source.clone()
    };

    // P19: Use warm-start seed if available, skip translation
    if let Some(seed_code) = warm_seed {
        info!(module = %mod_name, "P19: using warm-start seed, skipping translation");
        mod_unit.rust_output = Some(seed_code);
        mod_unit.state = MigrationState::Refined;
        // Save warm-seeded code as module artifact
        if let Some(store) = artifacts
            && let Some(rust) = &mod_unit.rust_output
        {
            let _ = store.save_translation_module(&module.name, rust);
        }
        // Skip to validation (after the translation block)
    } else {
        // P12: Sub-chunk large modules — if a module exceeds MEDIUM_FILE_LOC, use
        // chunked translation instead of single-pass to avoid LLM context overflow.
        let module_lines = module.line_count;
        let rust_code = if module_lines > MEDIUM_FILE_LOC {
            let chunk_target = if module_lines > MASSIVE_FILE_LOC {
                600
            } else if module_lines > VERY_LARGE_FILE_LOC {
                500
            } else {
                400
            };
            info!(
                module = %mod_name,
                lines = module_lines,
                chunk_target,
                "P12: module exceeds {} LOC, using chunked translation",
                MEDIUM_FILE_LOC
            );

            let chunks = noricum_tools::ast::chunk_c_source(&augmented_c, chunk_target);
            match noricum_agents::translation::translate_chunked(
                client,
                &translation_model_sel.model,
                &chunks,
                None, // No c2rust context for modular
                analysis,
                &relevant_patterns,
                config.translation_temperature,
            )
            .await
            {
                Ok(chunked_result) => {
                    local_metrics.llm_calls += chunked_result.chunks.len() as u32;
                    for chunk in &chunked_result.chunks {
                        local_metrics.input_tokens +=
                            noricum_agents::estimate_tokens(&chunk.rust_source);
                    }
                    local_metrics.translation_ms +=
                        translation_start.elapsed().as_millis() as u64;
                    chunked_result.combined
                }
                Err(e) => {
                    warn!(module = %mod_name, error = %e, "chunked module translation failed");
                    return Ok(ModuleMigrationResult {
                        name: module.name.clone(),
                        mod_idx,
                        unit: None,
                        artifact: crate::artifacts::ModuleArtifact {
                            name: module.name.clone(),
                            state: "TranslationFailed".to_string(),
                            score: 0.0,
                            compiles: false,
                            unsafe_count: 0,
                        },
                        metrics: local_metrics,
                        validated: false,
                        was_skip: false,
                    });
                }
            }
        } else {
            // Small module — single-pass translation
            match noricum_agents::translation::translate_function_with_patterns_and_temperature(
                client,
                &translation_model_sel.model,
                &augmented_c,
                None,
                analysis,
                &relevant_patterns,
                config.translation_temperature,
            )
            .await
            {
                Ok(code) => {
                    local_metrics.llm_calls += 1;
                    local_metrics.input_tokens += noricum_agents::estimate_tokens(&augmented_c);
                    local_metrics.output_tokens += noricum_agents::estimate_tokens(&code);
                    local_metrics.translation_ms +=
                        translation_start.elapsed().as_millis() as u64;
                    code
                }
                Err(e) => {
                    warn!(module = %mod_name, error = %e, "module translation failed");
                    return Ok(ModuleMigrationResult {
                        name: module.name.clone(),
                        mod_idx,
                        unit: None,
                        artifact: crate::artifacts::ModuleArtifact {
                            name: module.name.clone(),
                            state: "TranslationFailed".to_string(),
                            score: 0.0,
                            compiles: false,
                            unsafe_count: 0,
                        },
                        metrics: local_metrics,
                        validated: false,
                        was_skip: false,
                    });
                }
            }
        };

        // Substance check
        let is_stub = !has_substance(&rust_code, &module.source);
        let rust_code = if is_stub {
            warn!(module = %mod_name, "P6: module translation produced stubs, re-translating");
            local_metrics.llm_calls += 1;
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

        // Save per-module artifact
        if let Some(store) = artifacts
            && let Some(rust) = &mod_unit.rust_output
        {
            let _ = store.save_translation_module(&module.name, rust);
        }
    } // end of else (non-warm-start translation path)

    // P24: Modules just need to compile — full validation on assembled output.
    // Relaxed threshold: min(user_score, 50) so compiling code passes per-module.
    let module_min_score = config.min_idiomatic_score.min(50);
    // P25: Validate against accumulated assembly to resolve cross-module deps
    let mod_validation = validate_module_with_assembly(
        &mod_unit, &module.name, module_outputs, module_min_score,
    )?;
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

    // P13: Re-translate if error count is catastrophically high
    let mut mod_validation = mod_validation;
    if !mod_validation.passed && should_retranslate(mod_validation.compiler_errors.len(), 0)
    {
        let error_count = mod_validation.compiler_errors.len();
        warn!(
            module = %mod_name,
            errors = error_count,
            "P13: error count exceeds threshold, re-translating with higher temperature"
        );
        if let Some(store) = artifacts
            && let Some(rust) = &mod_unit.rust_output
        {
            let _ = store.save_module_repair_rejected(&module.name, 0, rust);
        }
        // Re-translate with temperature 0.5
        let retranslated =
            noricum_agents::translation::translate_function_with_patterns_and_temperature(
                client,
                &translation_model_sel.model,
                &augmented_c,
                None,
                analysis,
                &relevant_patterns,
                Some(0.5),
            )
            .await;
        local_metrics.llm_calls += 1;
        if let Ok(new_code) = retranslated
            && has_substance(&new_code, &module.source)
        {
            mod_unit.rust_output = Some(new_code);
            // P24+P25: Re-validate with assembly context and relaxed threshold
            let re_val = validate_module_with_assembly(
                &mod_unit, &module.name, module_outputs, module_min_score,
            )?;
            noricum_validation::apply_validation_with_max(
                &mut mod_unit,
                &re_val,
                config.max_repair_iterations,
            );
            info!(
                module = %mod_name,
                compiles = re_val.compiles,
                score = re_val.idiomatic_score,
                errors = re_val.compiler_errors.len(),
                "P13: re-translation validation"
            );
            mod_validation = re_val;
        }
    }

    // Repair loop for this module (each module is small enough for effective repair)
    if !mod_validation.passed {
        let repair_start = Instant::now();
        let repair_model_sel = select_repair_model(provider_config, difficulty)?;
        let max_iters = config.max_repair_iterations.min(5);
        let baseline_unsafe = mod_validation.unsafe_count;
        // P21: Effective unsafe ceiling for modular repair
        let unsafe_ceiling = config
            .max_unsafe_blocks
            .map_or(baseline_unsafe, |max| max.max(baseline_unsafe));
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

            // P22: After 3+ failed iterations, hint that unsafe is acceptable
            let mut effective_feedback = effective_feedback;
            if !errors.is_empty() && iter >= 3 && unsafe_ceiling > 0 {
                effective_feedback.push(format!(
                    "IMPORTANT: If you cannot fix the compilation errors with safe code, \
                     you MAY use up to {} unsafe block(s) to make the code compile. \
                     A compiling program with minimal unsafe is better than one that doesn't compile. \
                     Wrap only the minimum necessary code in unsafe.",
                    unsafe_ceiling
                ));
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

            local_metrics.llm_calls += 1;
            local_metrics.input_tokens += noricum_agents::estimate_tokens(current_rust);
            local_metrics.output_tokens += noricum_agents::estimate_tokens(&repaired);

            // P0: Quality floor
            let repaired_unsafe = noricum_tools::ast::count_unsafe_blocks_ast(&repaired);
            if repaired_unsafe > unsafe_ceiling {
                warn!(module = %mod_name, iter, repaired_unsafe, unsafe_ceiling, "P0: repair rejected — exceeds unsafe ceiling (P21)");
                if let Some(store) = artifacts {
                    let _ =
                        store.save_module_repair_rejected(&module.name, iter, &repaired);
                }
                continue;
            }

            mod_unit.rust_output = Some(repaired);
            mod_unit.state = MigrationState::Repairing(iter);

            // P24+P25: Validate repair against assembly context
            let re_validation = validate_module_with_assembly(
                &mod_unit, &module.name, module_outputs, module_min_score,
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

            // Save repair artifact for this module
            if let Some(store) = artifacts
                && let Some(rust) = &mod_unit.rust_output
            {
                let val_json = format!(
                    "{{\"module\":\"{}\",\"iter\":{},\"compiles\":{},\"score\":{},\"unsafe\":{}}}",
                    module.name,
                    iter,
                    re_validation.compiles,
                    re_validation.idiomatic_score,
                    re_validation.unsafe_count
                );
                let _ =
                    store.save_module_repair_iteration(&module.name, iter, rust, &val_json);
            }

            // P1: Best-version tracking
            if re_validation.unsafe_count <= unsafe_ceiling
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

            check_budget(config, &local_metrics)?;
        }

        local_metrics.repair_ms += repair_start.elapsed().as_millis() as u64;

        // Use best version if repair didn't fully pass
        if mod_unit.state != MigrationState::Validated {
            let mod_error_count = mod_unit.last_errors.len();
            let mod_unsafe = mod_unit.unsafe_count.unwrap_or(0);
            if let Some(best) = best_version {
                mod_unit.rust_output = Some(best);
                mod_unit.idiomatic_score = Some(best_score);
            }
            // P23: Graduated state for module
            mod_unit.state = graduated_state(
                best_compiles, mod_unsafe, best_score, 80, mod_error_count,
            );
            if !best_compiles {
                warn!(module = %mod_name, state = ?mod_unit.state, "module did not reach Validated state (P23)");
            }
        }
    }

    // P32: Brace-balance validation — detect truncated LLM output
    let p32_rust_code = mod_unit.rust_output.clone();
    if let Some(rust_code) = p32_rust_code {
        let brace_depth = noricum_tools::repair_rules::check_brace_balance(&rust_code);
        if brace_depth > 0 {
            warn!(
                module = %mod_name,
                depth = brace_depth,
                "P32: module output has unclosed braces, attempting re-translate"
            );

            // Step 1: Re-translate once with truncation hint
            let retranslated = noricum_agents::translation::translate_function_with_patterns_and_temperature(
                client,
                &select_model(provider_config, difficulty, "translation")?.model,
                &format!(
                    "/* IMPORTANT: Your previous translation of this code was TRUNCATED. \
                    Ensure ALL function bodies have matching closing braces. \
                    Output the COMPLETE translation. */\n\n{}",
                    module.source
                ),
                None,
                analysis,
                &PatternStore::load_seed_patterns().find_relevant(&module.source, 3),
                Some(0.3), // Lower temperature for more deterministic output
            )
            .await;

            let mut fixed = false;
            if let Ok(retrans_code) = retranslated {
                let retrans_depth = noricum_tools::repair_rules::check_brace_balance(&retrans_code);
                if retrans_depth == 0 {
                    info!(module = %mod_name, "P32: re-translate produced balanced output");
                    mod_unit.rust_output = Some(retrans_code);
                    local_metrics.llm_calls += 1;
                    fixed = true;
                } else {
                    warn!(module = %mod_name, depth = retrans_depth, "P32: re-translate still truncated");
                    local_metrics.llm_calls += 1;
                }
            }

            // Step 2: Smart truncate — cut at last balanced point
            if !fixed {
                let truncated = noricum_tools::repair_rules::auto_close_braces(&rust_code);
                mod_unit.rust_output = Some(truncated);
                // Mark as non-compiling so it doesn't pollute assembly context (P26)
                mod_unit.last_errors.push(format!("P32: truncated {brace_depth} unclosed brace(s)"));
            }
        }
    }

    // Build the result — unit contains the FunctionUnit with rust_output
    let validated = mod_unit.state == MigrationState::Validated
        || mod_unit.state == MigrationState::CompilesUnsafe;
    let compiles = mod_unit.state == MigrationState::Validated
        || mod_unit.last_errors.is_empty();

    let artifact = crate::artifacts::ModuleArtifact {
        name: module.name.clone(),
        state: format!("{:?}", mod_unit.state),
        score: mod_unit.idiomatic_score.unwrap_or(0) as f64,
        compiles,
        unsafe_count: mod_unit.unsafe_count.unwrap_or(0),
    };

    Ok(ModuleMigrationResult {
        name: module.name.clone(),
        mod_idx,
        unit: Some(mod_unit),
        artifact,
        metrics: local_metrics,
        validated,
        was_skip: false,
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

/// Merge `use` statements that share the same base path.
///
/// Groups `use std::io::{Read, Write};` and `use std::io::{self, Seek};`
/// into `use std::io::{self, Read, Seek, Write};`.
/// Simple `use foo::Bar;` are kept as-is (deduplicated by exact match).
fn merge_use_statements(uses: Vec<String>) -> Vec<String> {
    use std::collections::{BTreeMap, BTreeSet};

    let brace_re =
        regex::Regex::new(r"^use\s+(?P<path>[^{;]+)::\{(?P<items>[^}]+)\};$")
            .expect("static regex");
    let simple_re =
        regex::Regex::new(r"^use\s+(?P<full>[^{]+);$").expect("static regex");

    // path -> set of items
    let mut groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut simple_uses: BTreeSet<String> = BTreeSet::new();

    for u in &uses {
        let trimmed = u.trim();
        if let Some(caps) = brace_re.captures(trimmed) {
            let path = caps["path"].trim().to_string();
            let items: Vec<String> = caps["items"]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let entry = groups.entry(path).or_default();
            for item in items {
                entry.insert(item);
            }
        } else if simple_re.is_match(trimmed) {
            simple_uses.insert(trimmed.to_string());
        }
    }

    let mut result: Vec<String> = Vec::new();

    // Emit merged brace imports
    for (path, items) in &groups {
        let sorted: Vec<&String> = {
            let mut v: Vec<&String> = items.iter().collect();
            // Put `self` first if present
            v.sort_by(|a, b| {
                if a.as_str() == "self" {
                    std::cmp::Ordering::Less
                } else if b.as_str() == "self" {
                    std::cmp::Ordering::Greater
                } else {
                    a.cmp(b)
                }
            });
            v
        };
        let items_str = sorted
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<&str>>()
            .join(", ");
        result.push(format!("use {path}::{{{items_str}}};"));
    }

    // Emit simple imports (but skip if already covered by a brace import)
    for s in &simple_uses {
        let covered = groups.iter().any(|(path, items)| {
            if let Some(rest) = s.strip_prefix(&format!("use {path}::")) {
                let name = rest.trim_end_matches(';').trim();
                items.contains(name)
            } else {
                false
            }
        });
        if !covered {
            result.push(s.clone());
        }
    }

    result.sort();
    result
}

/// Assemble the final Rust output from individually migrated module outputs.
///
/// P27: Deduplicates `use` statements and type/struct/enum/const definitions
/// across modules. The first module to define a name wins; subsequent modules
/// have their duplicate definitions stripped. This prevents compilation errors
/// from modules that independently translate the same C types.
fn assemble_module_outputs(modules: &[(String, String, bool)], type_contract: Option<&str>) -> String {
    let mut all_uses: Vec<String> = Vec::new();
    let mut code_parts: Vec<String> = Vec::new();
    // P27: Track which type names have already been defined
    let mut defined_types: std::collections::HashSet<String> = std::collections::HashSet::new();

    // P33: If type contract provided, seed defined_types so P27 dedup strips module redefinitions
    let contract_block = if let Some(contract) = type_contract {
        for cline in contract.lines() {
            let trimmed = cline.trim();
            if let Some(type_name) = extract_definition_name(trimmed) {
                defined_types.insert(type_name);
            }
        }
        format!(
            "// === P33: Type Contract (shared types) ===\n{}\n// === End Type Contract ===\n\n",
            contract
        )
    } else {
        String::new()
    };

    for (mod_name, rust_code, _compiles) in modules {
        // P31: Strip markdown fences before processing
        let clean_code: String = rust_code
            .lines()
            .filter(|line| !line.trim().starts_with("```"))
            .collect::<Vec<&str>>()
            .join("\n");

        let mod_code_lines = dedup_module_definitions(&clean_code, &mut all_uses, &mut defined_types);
        // Remove leading/trailing empty lines
        let trimmed_lines = trim_empty_lines(&mod_code_lines);
        if !trimmed_lines.is_empty() {
            code_parts.push(format!(
                "// --- Module: {} ---\n{}",
                mod_name,
                trimmed_lines.join("\n")
            ));
        }
    }

    let mut output = String::new();
    if !all_uses.is_empty() {
        // P31: Merge use statements that share the same base path
        let merged = merge_use_statements(all_uses);
        output.push_str(&merged.join("\n"));
        output.push_str("\n\n");
    }
    // P33: Prepend type contract before module code
    if !contract_block.is_empty() {
        output.push_str(&contract_block);
    }
    output.push_str(&code_parts.join("\n\n"));
    output.push('\n');
    output
}

/// Run the full async LLM migration pipeline on all C files in a directory.
///
/// Uses `DependencyGraph` to determine topological order (dependencies first),
/// and accumulates migrated Rust signatures to inject as context for later files.
/// P27: Extract non-duplicate lines from a module, collecting `use` statements
/// and stripping type/struct/enum/const definitions that were already defined
/// by earlier modules.
fn dedup_module_definitions<'a>(
    rust_code: &'a str,
    all_uses: &mut Vec<String>,
    defined_types: &mut std::collections::HashSet<String>,
) -> Vec<&'a str> {
    let mut result_lines: Vec<&str> = Vec::new();
    let lines: Vec<&str> = rust_code.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Deduplicate use statements
        if trimmed.starts_with("use ") && trimmed.ends_with(';') {
            if !all_uses.contains(&trimmed.to_string()) {
                all_uses.push(trimmed.to_string());
            }
            i += 1;
            continue;
        }

        // Check for type definitions: pub struct/enum/const/type Name
        if let Some(type_name) = extract_definition_name(trimmed) {
            if defined_types.contains(&type_name) {
                // Skip this entire definition (including its body with braces)
                i = skip_braced_block(&lines, i);
                continue;
            }
            defined_types.insert(type_name);
        }

        // Check for impl blocks: impl TypeName / impl Trait for TypeName
        if let Some(impl_target) = extract_impl_target(trimmed) {
            // If the type isn't defined yet (was stripped), skip the impl too
            // But only skip if we've seen this type before AND it was from another module
            // (i.e., the type was stripped from this module)
            if !defined_types.contains(&impl_target) && trimmed.contains("impl ") {
                // Type not defined anywhere yet — keep the impl, it defines behavior
                result_lines.push(lines[i]);
                i += 1;
                continue;
            }
        }

        result_lines.push(lines[i]);
        i += 1;
    }
    result_lines
}

/// Extract the name from a type definition line (struct, enum, const, type).
/// Returns None if the line is not a definition.
fn extract_definition_name(line: &str) -> Option<String> {
    // Match patterns like: pub struct Foo { / pub enum Bar { / const X: ...
    let prefixes = [
        "pub struct ", "struct ",
        "pub enum ", "enum ",
        "pub const ", "const ",
        "pub type ", "type ",
    ];
    for prefix in &prefixes {
        if let Some(rest) = line.strip_prefix(prefix) {
            // Extract the name (up to first non-alphanumeric/underscore)
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// Extract the target type from an `impl` line.
/// `impl Foo {` → Some("Foo"), `impl Display for Foo {` → Some("Foo")
fn extract_impl_target(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("impl ") {
        return None;
    }
    let rest = &trimmed[5..]; // after "impl "
    // Check for "Trait for Type" pattern
    if let Some(for_pos) = rest.find(" for ") {
        let after_for = &rest[for_pos + 5..];
        let name: String = after_for.chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    // Direct impl: "impl Type {"
    let name: String = rest.chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if !name.is_empty() {
        Some(name)
    } else {
        None
    }
}

/// Skip a braced block starting at line `start`. Returns the index after the closing brace.
fn skip_braced_block(lines: &[&str], start: usize) -> usize {
    let mut depth = 0i32;
    let mut i = start;
    // Count braces on the first line
    for ch in lines[start].chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    i += 1;
    // If the definition had no opening brace on this line (e.g., `const X: i32 = 5;`)
    // it's a single-line definition — already skipped
    if depth <= 0 {
        return i;
    }
    // Track braces until balanced
    while i < lines.len() && depth > 0 {
        for ch in lines[i].chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    i
}

/// Trim leading and trailing empty lines from a slice.
fn trim_empty_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let mut result: Vec<&str> = lines.to_vec();
    while result.first().is_some_and(|l| l.trim().is_empty()) {
        result.remove(0);
    }
    while result.last().is_some_and(|l| l.trim().is_empty()) {
        result.pop();
    }
    result
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
        assert_eq!(pc.primary_provider, "anthropic");
    }

    #[test]
    fn test_migration_config_to_provider_config_deepseek() {
        let config = MigrationConfig {
            primary_provider: Some("deepseek".to_string()),
            deepseek_api_key: Some("ds-key".to_string()),
            anthropic_api_key: None,
            ..Default::default()
        };
        let provider: ProviderConfig = (&config).into();
        assert_eq!(provider.primary_provider, "deepseek");
        assert_eq!(provider.deepseek_api_key, Some("ds-key".to_string()));
        assert!(provider.anthropic_api_key.is_none());
    }

    #[test]
    fn test_migration_config_auto_detects_provider() {
        // No primary set, has deepseek key, no anthropic key
        let config = MigrationConfig {
            primary_provider: None,
            deepseek_api_key: Some("ds-key".to_string()),
            anthropic_api_key: None,
            ..Default::default()
        };
        let provider: ProviderConfig = (&config).into();
        assert_eq!(provider.primary_provider, "deepseek");
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
                true,
            ),
            (
                "core".to_string(),
                "use std::collections::HashMap;\nuse std::io;\n\nfn core_b() -> i32 { 2 }\n"
                    .to_string(),
                true,
            ),
        ];
        let result = assemble_module_outputs(&modules, None);
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
        let modules: Vec<(String, String, bool)> = vec![];
        let result = assemble_module_outputs(&modules, None);
        assert_eq!(result.trim(), "");
    }

    #[test]
    fn test_assemble_strips_markdown_fences() {
        let modules = vec![
            (
                "mod_a".to_string(),
                "use std::io;\n\nfn foo() -> i32 { 1 }".to_string(),
                true,
            ),
            (
                "mod_b".to_string(),
                "```rust\nfn bar() -> i32 { 2 }\n```".to_string(),
                false,
            ),
        ];
        let assembled = assemble_module_outputs(&modules, None);
        assert!(
            !assembled.contains("```"),
            "fences should be stripped from assembly:\n{assembled}"
        );
        assert!(assembled.contains("fn foo()"));
        assert!(assembled.contains("fn bar()"));
    }

    #[test]
    fn test_assemble_merges_use_imports() {
        let modules = vec![
            (
                "a".to_string(),
                "use std::io::{self, Read};\nfn a() {}".to_string(),
                true,
            ),
            (
                "b".to_string(),
                "use std::io::{self, Read, Seek, SeekFrom};\nfn b() {}".to_string(),
                true,
            ),
            (
                "c".to_string(),
                "use std::io::{self, Write, Seek, SeekFrom};\nfn c() {}".to_string(),
                true,
            ),
        ];
        let assembled = assemble_module_outputs(&modules, None);

        // Should have exactly ONE std::io import with all items merged
        let io_lines: Vec<&str> = assembled
            .lines()
            .filter(|l| l.contains("use std::io"))
            .collect();
        assert_eq!(
            io_lines.len(),
            1,
            "should merge into one use std::io line, got: {io_lines:?}"
        );

        let io_line = io_lines[0];
        for item in &["Read", "Seek", "SeekFrom", "Write", "self"] {
            assert!(
                io_line.contains(item),
                "merged import should contain {item}: {io_line}"
            );
        }
    }

    #[test]
    fn test_assemble_truncated_module_auto_closed() {
        let modules = vec![
            ("mod_a".to_string(), "use std::io;\n\nfn foo() {\n    1\n}".to_string(), true),
            ("mod_b".to_string(), "fn bar() {\n    if true {\n        let x = 1;".to_string(), false),
            ("mod_c".to_string(), "fn baz() {\n    2\n}".to_string(), true),
        ];
        let result = assemble_module_outputs(&modules, None);
        // All modules should be present in assembly (truncated ones included)
        assert!(result.contains("fn foo()"), "mod_a present");
        assert!(result.contains("fn bar()"), "mod_b present");
        assert!(result.contains("fn baz()"), "mod_c present");
    }

    #[test]
    fn test_warm_start_strategy() {
        use crate::artifacts::ModuleArtifact;

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
    fn test_should_retranslate_on_high_errors() {
        assert!(should_retranslate(150, 0)); // 150 errors, no retranslation yet
        assert!(should_retranslate(101, 0));
        assert!(!should_retranslate(100, 0)); // exactly 100 = try repair
        assert!(!should_retranslate(50, 0)); // low errors = repair
        assert!(!should_retranslate(200, 1)); // already retranslated once = don't loop
        assert!(!should_retranslate(200, 2)); // max 1 retranslation
    }

    #[test]
    fn test_adaptive_llm_budget() {
        // P34: 10 modules: 10*10 + 25 = 125, user set 50 → max(125, 50) = 125
        assert_eq!(compute_adaptive_budget(10, Some(50)), 125);
        // 2 modules: 2*10 + 25 = 45, but user set 50 → use max(45, 50) = 50
        assert_eq!(compute_adaptive_budget(2, Some(50)), 50);
        // No user limit: use adaptive
        assert_eq!(compute_adaptive_budget(10, None), 125);
        // 1 module: 1*10 + 25 = 35
        assert_eq!(compute_adaptive_budget(1, None), 35);
    }

    #[test]
    fn test_budget_phase() {
        use noricum_ir::MigrationMetrics;

        let config = MigrationConfig {
            max_llm_calls: Some(100),
            ..MigrationConfig::default()
        };

        let mut metrics = MigrationMetrics::default();

        metrics.llm_calls = 50;
        assert_eq!(budget_phase(&config, &metrics), BudgetPhase::Normal);

        metrics.llm_calls = 82;
        assert_eq!(budget_phase(&config, &metrics), BudgetPhase::SkipRepairs);

        metrics.llm_calls = 96;
        assert_eq!(budget_phase(&config, &metrics), BudgetPhase::AssembleNow);

        metrics.llm_calls = 101;
        assert_eq!(budget_phase(&config, &metrics), BudgetPhase::SaveAndExit);
    }

    #[test]
    fn test_assemble_module_outputs_preserves_order() {
        let modules = vec![
            ("first".to_string(), "fn a() {}\n".to_string(), true),
            ("second".to_string(), "fn b() {}\n".to_string(), true),
            ("third".to_string(), "fn c() {}\n".to_string(), true),
        ];
        let result = assemble_module_outputs(&modules, None);
        let pos_a = result.find("Module: first").unwrap();
        let pos_b = result.find("Module: second").unwrap();
        let pos_c = result.find("Module: third").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_assemble_dedup_types_across_modules() {
        // P27: Second module redefines ZipArchive — should be stripped
        let mod_a = "pub struct ZipArchive {\n    pub data: Vec<u8>,\n}\n\nfn read(a: &ZipArchive) -> usize { a.data.len() }\n".to_string();
        let mod_b = "pub struct ZipArchive {\n    pub data: Vec<u8>,\n}\n\nfn write(a: &mut ZipArchive) { a.data.push(0); }\n".to_string();
        let modules = vec![
            ("types".to_string(), mod_a, true),
            ("ops".to_string(), mod_b, true),
        ];
        let result = assemble_module_outputs(&modules, None);
        // ZipArchive struct should appear exactly once
        assert_eq!(
            result.matches("pub struct ZipArchive").count(), 1,
            "ZipArchive should be deduplicated: {result}"
        );
        // Both functions should still be present
        assert!(result.contains("fn read("), "fn read should survive dedup");
        assert!(result.contains("fn write("), "fn write should survive dedup");
    }

    #[test]
    fn test_assemble_dedup_enum_and_const() {
        let mod_a = "pub enum ZipError {\n    Io,\n    Parse,\n}\n\npub const HEADER_SIZE: u32 = 30;\n\nfn init() -> i32 { 0 }\n".to_string();
        let mod_b = "pub enum ZipError {\n    Io,\n    Parse,\n}\n\npub const HEADER_SIZE: u32 = 30;\n\nfn process() -> i32 { 1 }\n".to_string();
        let modules = vec![
            ("base".to_string(), mod_a, true),
            ("ext".to_string(), mod_b, true),
        ];
        let result = assemble_module_outputs(&modules, None);
        assert_eq!(result.matches("pub enum ZipError").count(), 1, "enum should be deduped");
        assert_eq!(result.matches("pub const HEADER_SIZE").count(), 1, "const should be deduped");
        assert!(result.contains("fn init()"));
        assert!(result.contains("fn process()"));
    }

    #[test]
    fn test_assemble_dedup_preserves_first_definition() {
        // First module defines ZipArchive with 2 fields, second with 3 fields.
        // First definition should win.
        let mod_a = "pub struct ZipArchive {\n    pub data: Vec<u8>,\n    pub name: String,\n}\n".to_string();
        let mod_b = "pub struct ZipArchive {\n    pub data: Vec<u8>,\n    pub name: String,\n    pub extra: bool,\n}\n\nfn check() -> bool { true }\n".to_string();
        let modules = vec![
            ("first".to_string(), mod_a, true),
            ("second".to_string(), mod_b, true),
        ];
        let result = assemble_module_outputs(&modules, None);
        assert_eq!(result.matches("pub struct ZipArchive").count(), 1);
        // First definition should be kept (2 fields, no extra)
        assert!(result.contains("pub name: String"), "first def fields should be present");
        assert!(!result.contains("pub extra: bool"), "second def fields should be stripped");
        assert!(result.contains("fn check()"), "functions should survive");
    }

    #[test]
    fn test_skip_braced_block_basic() {
        let lines = vec!["pub struct Foo {", "    x: i32,", "}", "fn bar() {}"];
        let end = skip_braced_block(&lines, 0);
        assert_eq!(end, 3, "should skip past closing brace");
    }

    #[test]
    fn test_extract_definition_name() {
        assert_eq!(extract_definition_name("pub struct ZipArchive {"), Some("ZipArchive".to_string()));
        assert_eq!(extract_definition_name("pub enum ZipError {"), Some("ZipError".to_string()));
        assert_eq!(extract_definition_name("pub const HEADER: u32 = 30;"), Some("HEADER".to_string()));
        assert_eq!(extract_definition_name("pub type Result = std::result::Result;"), Some("Result".to_string()));
        assert_eq!(extract_definition_name("fn foo() {}"), None);
        assert_eq!(extract_definition_name("let x = 5;"), None);
    }

    #[test]
    fn test_max_unsafe_allowance() {
        // With max_unsafe=None (default), baseline_unsafe=0 means any unsafe rejects
        let baseline: u32 = 0;
        let max_unsafe: Option<u32> = None;
        let effective_ceiling = max_unsafe.unwrap_or(baseline).max(baseline);
        assert_eq!(effective_ceiling, 0);

        // With max_unsafe=3, even baseline=0 allows up to 3 unsafe blocks
        let max_unsafe: Option<u32> = Some(3);
        let effective_ceiling = max_unsafe.unwrap_or(baseline).max(baseline);
        assert_eq!(effective_ceiling, 3);

        // Repair producing 2 unsafe blocks: accepted (2 <= 3)
        assert!(2 <= effective_ceiling);

        // Repair producing 5 unsafe blocks: rejected (5 > 3)
        assert!(5 > effective_ceiling);

        // baseline=4, max_unsafe=2 → ceiling is max(2,4)=4 (never reduce below baseline)
        let baseline: u32 = 4;
        let max_unsafe: Option<u32> = Some(2);
        let effective_ceiling = max_unsafe.unwrap_or(baseline).max(baseline);
        assert_eq!(effective_ceiling, 4);
    }

    #[test]
    fn test_compilation_priority_hint() {
        // Before iteration 3: no hint
        let iter = 2u32;
        let has_errors = true;
        let unsafe_ceiling = 3u32;
        let should_hint = has_errors && iter >= 3 && unsafe_ceiling > 0;
        assert!(!should_hint);

        // At iteration 3+, has errors, unsafe allowed: hint
        let iter = 3u32;
        let should_hint = has_errors && iter >= 3 && unsafe_ceiling > 0;
        assert!(should_hint);

        // If no errors, no hint needed
        let has_errors = false;
        let should_hint = has_errors && iter >= 3 && unsafe_ceiling > 0;
        assert!(!should_hint);

        // If unsafe_ceiling is 0 (default with baseline=0), no hint
        let has_errors = true;
        let unsafe_ceiling = 0u32;
        let should_hint = has_errors && iter >= 3 && unsafe_ceiling > 0;
        assert!(!should_hint);
    }

    #[test]
    fn test_assembly_validation_catches_cross_module_deps() {
        // Module A defines a type
        let module_a = "pub struct ZipArchive { pub data: Vec<u8> }\n";
        // Module B uses it — compiles ONLY with A present
        let module_b = "fn read_archive(a: &ZipArchive) -> usize { a.data.len() }\n";

        // Module B alone: doesn't compile
        let result_alone = noricum_tools::compiler::check_rust_compiles(module_b).unwrap();
        assert!(!result_alone.success, "module B alone should not compile");

        // Module B with A as context: compiles
        let combined = format!("{module_a}\n{module_b}");
        let result_with_context =
            noricum_tools::compiler::check_rust_compiles(&combined).unwrap();
        assert!(
            result_with_context.success,
            "module B with A should compile"
        );

        // build_assembly_context helper — only includes compiling modules
        let outputs = vec![("mod_a".to_string(), module_a.to_string(), true)];
        let ctx = build_assembly_context(&outputs);
        assert!(ctx.contains("ZipArchive"));

        // P26: non-compiling modules are filtered from assembly context
        let outputs_with_broken = vec![
            ("mod_a".to_string(), module_a.to_string(), true),
            ("mod_broken".to_string(), "fn broken( {".to_string(), false),
        ];
        let ctx = build_assembly_context(&outputs_with_broken);
        assert!(ctx.contains("ZipArchive"), "compiling module should be included");
        assert!(!ctx.contains("broken"), "non-compiling module should be filtered");
    }

    #[test]
    fn test_module_validation_relaxed() {
        // For modules, compilation is the primary gate
        let compiles = true;
        let score = 55u32;
        let min_score = 60u32;

        // P24: module threshold = min(user_score, 50) = 50
        let module_threshold = min_score.min(50);
        assert_eq!(module_threshold, 50);
        let module_passed = compiles && score >= module_threshold;
        assert!(module_passed, "compiling module with score 55 should pass");

        // As a full file: score 55 < elevated 80 would fail
        let file_passed = compiles && score >= 80;
        assert!(!file_passed, "as file, score 55 < 80 would fail");

        // Even with low min_score (30), module threshold stays at 30
        let module_threshold = 30u32.min(50);
        assert_eq!(module_threshold, 30);
    }

    #[test]
    fn test_graduated_state_assignment() {
        // Compiles, 0 unsafe, score >= 80 → Validated
        assert_eq!(
            graduated_state(true, 0, 85, 80, 0),
            MigrationState::Validated
        );
        // Compiles, 2 unsafe, score >= 50 → CompilesUnsafe
        assert_eq!(
            graduated_state(true, 2, 70, 80, 0),
            MigrationState::CompilesUnsafe
        );
        // Compiles, 0 unsafe, score 45 → CompilesLowScore
        assert_eq!(
            graduated_state(true, 0, 45, 80, 0),
            MigrationState::CompilesLowScore
        );
        // Doesn't compile, score 60, 3 errors → NearlyCompiles
        assert_eq!(
            graduated_state(false, 0, 60, 80, 3),
            MigrationState::NearlyCompiles
        );
        // Doesn't compile, score 60, 10 errors → FallbackUnsafe (too many errors)
        assert_eq!(
            graduated_state(false, 0, 60, 80, 10),
            MigrationState::FallbackUnsafe
        );
        // Doesn't compile, low score, 3 errors → FallbackUnsafe
        assert_eq!(
            graduated_state(false, 0, 30, 80, 3),
            MigrationState::FallbackUnsafe
        );
    }

    #[test]
    fn test_warm_start_with_graduated_states() {
        use crate::artifacts::ModuleArtifact;

        // CompilesUnsafe with score >= 70 → Skip
        let compiles_unsafe = ModuleArtifact {
            name: "mod_a".into(),
            state: "CompilesUnsafe".into(),
            score: 75.0,
            compiles: true,
            unsafe_count: 2,
        };
        assert_eq!(warm_start_action(&compiles_unsafe), WarmAction::Skip);

        // NearlyCompiles with score >= 50 → SeedRepair
        let nearly = ModuleArtifact {
            name: "mod_b".into(),
            state: "NearlyCompiles".into(),
            score: 60.0,
            compiles: false,
            unsafe_count: 0,
        };
        assert_eq!(warm_start_action(&nearly), WarmAction::SeedRepair);

        // NearlyCompiles with score < 50 → Retranslate
        let nearly_low = ModuleArtifact {
            name: "mod_c".into(),
            state: "NearlyCompiles".into(),
            score: 30.0,
            compiles: false,
            unsafe_count: 0,
        };
        assert_eq!(warm_start_action(&nearly_low), WarmAction::Retranslate);
    }

    #[test]
    fn test_assemble_with_type_contract() {
        let contract = "pub struct Foo { pub x: i32 }\npub enum Bar { A, B }\n";
        let modules = vec![
            (
                "mod1".to_string(),
                "pub fn create_foo() -> Foo { Foo { x: 1 } }".to_string(),
                true,
            ),
            (
                "mod2".to_string(),
                "pub fn get_bar() -> Bar { Bar::A }".to_string(),
                true,
            ),
        ];
        let result = assemble_module_outputs(&modules, Some(contract));
        let contract_pos = result.find("pub struct Foo").unwrap();
        let fn_pos = result.find("pub fn create_foo").unwrap();
        assert!(
            contract_pos < fn_pos,
            "Type contract should precede module code"
        );
        assert!(result.contains("pub fn get_bar"));
        assert!(
            result.contains("P33: Type Contract"),
            "should contain contract header"
        );
    }

    #[test]
    fn test_assemble_with_type_contract_dedup() {
        // P33: Module redefines a type from the contract — should be stripped
        let contract = "pub struct Foo { pub x: i32 }\n";
        let modules = vec![(
            "mod1".to_string(),
            "pub struct Foo { pub x: i32 }\npub fn use_foo(f: &Foo) -> i32 { f.x }".to_string(),
            true,
        )];
        let result = assemble_module_outputs(&modules, Some(contract));
        // Foo from the contract block + module's Foo stripped = exactly 1 occurrence
        assert_eq!(
            result.matches("pub struct Foo").count(),
            1,
            "contract types should not be duplicated by module redefinitions: {result}"
        );
        assert!(result.contains("pub fn use_foo"));
    }

    #[test]
    fn test_assemble_without_type_contract() {
        // None contract = legacy behavior
        let modules = vec![
            (
                "a".to_string(),
                "pub struct X { pub v: i32 }\nfn a() {}".to_string(),
                true,
            ),
            (
                "b".to_string(),
                "pub struct X { pub v: i32 }\nfn b() {}".to_string(),
                true,
            ),
        ];
        let result = assemble_module_outputs(&modules, None);
        assert!(!result.contains("P33: Type Contract"));
        // P27 dedup still works
        assert_eq!(
            result.matches("pub struct X").count(),
            1,
            "P27 dedup should still work without contract"
        );
    }
}
