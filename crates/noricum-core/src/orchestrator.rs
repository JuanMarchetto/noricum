/// Migration orchestrator: drives the per-function state machine.
///
/// The orchestrator takes a C source file through the full pipeline:
/// Pending -> Extracted -> Characterized -> C2RustDone -> Analyzed -> Refined -> Validated
///
/// Two execution modes:
/// - `migrate_file_sync`: deterministic, no LLM (C2Rust + validation only)
/// - `migrate_file`: async, full LLM agent pipeline (analysis, translation, repair, test gen)
use std::path::Path;

use noricum_agents::providers::{
    ProviderConfig, ProviderKind, create_anthropic_client, create_anthropic_client_with_key,
    select_model,
};
use noricum_ir::{FunctionUnit, MigrationProject, MigrationState};
use rig::providers::anthropic;
use tracing::{debug, info, warn};

use crate::CoreError;

/// Configuration for the async LLM-based migration pipeline.
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Anthropic API key. Defaults to `ANTHROPIC_API_KEY` env var.
    pub anthropic_api_key: Option<String>,
    /// Ollama URL override. If `None`, uses the default `http://localhost:11434`.
    pub ollama_url: Option<String>,
    /// Maximum number of repair iterations before falling back to unsafe.
    pub max_repair_iterations: u32,
    /// Minimum idiomatic score (0-100) required to pass validation.
    pub min_idiomatic_score: u32,
    /// Whether to generate equivalence tests after successful migration.
    pub generate_tests: bool,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            ollama_url: None,
            max_repair_iterations: 5,
            min_idiomatic_score: 60,
            generate_tests: true,
        }
    }
}

impl MigrationConfig {
    /// Build a `ProviderConfig` from this migration config.
    fn to_provider_config(&self) -> ProviderConfig {
        ProviderConfig {
            anthropic_api_key: self.anthropic_api_key.clone(),
            ollama_url: self
                .ollama_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434".to_string()),
            ..ProviderConfig::default()
        }
    }

    /// Try to create an Anthropic client from the configured API key.
    fn create_client(&self) -> Option<anthropic::Client> {
        if let Some(ref key) = self.anthropic_api_key {
            match create_anthropic_client_with_key(key) {
                Ok(client) => Some(client),
                Err(e) => {
                    warn!(error = %e, "failed to create Anthropic client from configured key");
                    None
                }
            }
        } else {
            match create_anthropic_client() {
                Ok(client) => Some(client),
                Err(e) => {
                    debug!(error = %e, "no Anthropic client available");
                    None
                }
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

    let name = c_file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    info!(function = %name, file = %source_path, "starting async migration");

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

    let provider_config = config.to_provider_config();

    let mut unit = FunctionUnit::new(name.clone(), source_path, c_source);
    unit.state = MigrationState::Extracted;
    info!(function = %name, state = ?unit.state, "state -> Extracted");

    // --- Stage 2: Classify difficulty ---
    let difficulty = crate::router::classify_difficulty(&unit.c_source);
    unit.difficulty = Some(difficulty);
    info!(function = %name, ?difficulty, "difficulty classified");

    // --- Stage 3: Try C2Rust transpilation ---
    match noricum_tools::c2rust::transpile(c_file) {
        Ok(output) => {
            unit.c2rust_output = Some(output.rust_source.clone());
            unit.rust_output = Some(output.rust_source);
            unit.state = MigrationState::C2RustDone;
            info!(function = %name, state = ?unit.state, "state -> C2RustDone");
        }
        Err(e) => {
            debug!(function = %name, error = %e, "c2rust not available, will translate from scratch");
        }
    }

    // --- Stage 4: Analysis agent ---
    let analysis_model_sel = select_model(&provider_config, difficulty, "analysis");
    info!(
        function = %name,
        model = %analysis_model_sel.model,
        provider = ?analysis_model_sel.provider,
        "calling analysis agent"
    );
    let analysis = match analysis_model_sel.provider {
        ProviderKind::Anthropic => {
            noricum_agents::analysis::analyze_function(
                &client,
                &analysis_model_sel.model,
                &unit.c_source,
                &name,
            )
            .await?
        }
        ProviderKind::Ollama => {
            // Ollama not yet supported for analysis; fall back to sync
            warn!(function = %name, "Ollama not yet supported for analysis agent");
            return migrate_file_sync(c_file);
        }
    };
    unit.state = MigrationState::Analyzed;
    info!(
        function = %name,
        state = ?unit.state,
        patterns = ?analysis.patterns,
        strategy = %analysis.strategy,
        "state -> Analyzed"
    );

    // --- Stage 5: Translation agent ---
    let translation_model_sel = select_model(&provider_config, difficulty, "translation");
    info!(
        function = %name,
        model = %translation_model_sel.model,
        "calling translation agent"
    );
    let rust_code = match translation_model_sel.provider {
        ProviderKind::Anthropic => {
            noricum_agents::translation::translate_function(
                &client,
                &translation_model_sel.model,
                &unit.c_source,
                unit.c2rust_output.as_deref(),
                &analysis,
            )
            .await?
        }
        ProviderKind::Ollama => {
            warn!(function = %name, "Ollama not yet supported for translation agent");
            return migrate_file_sync(c_file);
        }
    };
    unit.rust_output = Some(rust_code);
    unit.state = MigrationState::Refined;
    info!(function = %name, state = ?unit.state, "state -> Refined");

    // --- Stage 6: Validate ---
    let validation = noricum_validation::validate(&unit)?;
    noricum_validation::apply_validation(&mut unit, &validation);
    info!(
        function = %name,
        state = ?unit.state,
        compiles = validation.compiles,
        idiomatic_score = validation.idiomatic_score,
        unsafe_count = validation.unsafe_count,
        "validation result"
    );

    // --- Stage 7: Repair loop ---
    if !validation.passed {
        let repair_model_sel = select_model(&provider_config, difficulty, "repair");

        let mut iteration = 1u32;
        while iteration <= config.max_repair_iterations {
            info!(
                function = %name,
                iteration,
                max = config.max_repair_iterations,
                model = %repair_model_sel.model,
                "entering repair iteration"
            );

            let current_rust = unit.rust_output.as_deref().unwrap_or("");
            let errors = &unit.last_errors;

            if errors.is_empty() && unit.idiomatic_score.unwrap_or(0) >= config.min_idiomatic_score
            {
                // Compiles fine but score was below threshold on first check; might pass now
                debug!(function = %name, "no compiler errors remaining, re-validating");
            }

            let repaired = match repair_model_sel.provider {
                ProviderKind::Anthropic => {
                    noricum_agents::repair::repair_function(
                        &client,
                        &repair_model_sel.model,
                        current_rust,
                        errors,
                        &unit.c_source,
                    )
                    .await?
                }
                ProviderKind::Ollama => {
                    warn!(function = %name, "Ollama not yet supported for repair agent");
                    break;
                }
            };

            unit.rust_output = Some(repaired);
            unit.state = MigrationState::Repairing(iteration);

            let re_validation = noricum_validation::validate(&unit)?;
            noricum_validation::apply_validation(&mut unit, &re_validation);
            info!(
                function = %name,
                iteration,
                state = ?unit.state,
                compiles = re_validation.compiles,
                idiomatic_score = re_validation.idiomatic_score,
                unsafe_count = re_validation.unsafe_count,
                "repair iteration result"
            );

            if re_validation.passed {
                info!(function = %name, "repair succeeded, validated");
                break;
            }

            iteration += 1;
        }

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
        let test_model_sel = select_model(&provider_config, difficulty, "test_gen");
        info!(
            function = %name,
            model = %test_model_sel.model,
            "calling test generation agent"
        );

        match test_model_sel.provider {
            ProviderKind::Anthropic => {
                match noricum_agents::test_gen::generate_tests(
                    &client,
                    &test_model_sel.model,
                    &unit.c_source,
                    unit.rust_output.as_deref().unwrap_or(""),
                    &name,
                )
                .await
                {
                    Ok(test_code) => {
                        info!(
                            function = %name,
                            test_code_len = test_code.len(),
                            "test generation succeeded"
                        );
                        // Store generated tests alongside the rust output
                        // The caller can write these to a file
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
            ProviderKind::Ollama => {
                debug!(function = %name, "Ollama not yet supported for test gen, skipping");
            }
        }
    }

    info!(
        function = %name,
        state = ?unit.state,
        idiomatic_score = ?unit.idiomatic_score,
        unsafe_count = ?unit.unsafe_count,
        "async migration complete"
    );

    Ok(unit)
}

/// Run the full async LLM migration pipeline on all C files in a directory.
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
    let mut entries: Vec<_> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "c") {
            found_any = true;
            entries.push(path);
        }
    }

    if !found_any {
        return Err(CoreError::NoSourceFiles(dir_str));
    }

    // Process files sequentially (LLM calls are the bottleneck, not I/O)
    for path in &entries {
        info!(file = %path.display(), "migrating file in directory");
        let unit = migrate_file(path, config).await?;
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
        };
        let pc = config.to_provider_config();
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
}
