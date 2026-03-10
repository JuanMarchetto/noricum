# Pipeline Optimization P13-P20 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Optimize the modular migration pipeline for DeepSeek R1 by adding error-based re-translation, API retry, adaptive budgets, wave-based parallelization, and warm-start from previous artifacts.

**Architecture:** Changes span 4 crates: `noricum-core` (orchestrator, artifacts, dependency, config), `noricum-agents` (translation retry), `noricum-tools` (module config), and `noricum-cli` (new flags). The parallelization uses `tokio::JoinSet` for concurrent module translation within dependency waves. Warm-start loads previous artifact outputs to skip/seed modules.

**Tech Stack:** Rust 2024, tokio (JoinSet for parallelism), serde_json (artifact manifest), anyhow/thiserror

---

### Task 1: P15 — Adaptive LLM Call Budget

The pipeline dies at 50 calls for 10 modules. Budget must scale with module count.

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs:1585-1620` (migrate_file_modular)
- Test: `crates/noricum-core/src/orchestrator.rs` (unit test at bottom)

**Step 1: Write the test**

Add to `#[cfg(test)] mod tests` in `orchestrator.rs`:

```rust
#[test]
fn test_adaptive_llm_budget() {
    // 10 modules: each needs ~1 translate + up to 5 repairs + 1 analysis = 7 per module + 10 buffer
    assert_eq!(compute_adaptive_budget(10, Some(50)), 80);
    // 2 modules: 2*7 + 10 = 24, but user set 50 → use max(24, 50) = 50
    assert_eq!(compute_adaptive_budget(2, Some(50)), 50);
    // No user limit: use adaptive
    assert_eq!(compute_adaptive_budget(10, None), 80);
    // 1 module: 1*7 + 10 = 17
    assert_eq!(compute_adaptive_budget(1, None), 17);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p noricum-core test_adaptive_llm_budget`
Expected: FAIL — `compute_adaptive_budget` not defined

**Step 3: Implement**

Add function near top of `orchestrator.rs` (after the constants around line 35):

```rust
/// Compute adaptive LLM call budget based on module count.
/// Formula: modules * 7 + 10 (1 translate + up to 5 repairs + 1 buffer per module, plus 10 global).
/// If user specified a limit, use max(adaptive, user_limit).
pub fn compute_adaptive_budget(module_count: usize, user_limit: Option<u32>) -> u32 {
    let adaptive = (module_count as u32) * 7 + 10;
    match user_limit {
        Some(limit) => adaptive.max(limit),
        None => adaptive,
    }
}
```

Then in `migrate_file_modular()`, after the modules are split (around line 1620, after `module_order` is computed), add:

```rust
let effective_max_calls = compute_adaptive_budget(modules.len(), config.max_llm_calls);
info!(modules = modules.len(), budget = effective_max_calls, "adaptive LLM budget");
```

Then use `effective_max_calls` in the budget check function for this run. The simplest approach: create a local config copy with the adjusted limit:

```rust
let mut effective_config = config.clone();
effective_config.max_llm_calls = Some(effective_max_calls);
```

Use `&effective_config` for all `check_budget()` calls within `migrate_file_modular()`.

**Step 4: Run test to verify it passes**

Run: `cargo test -p noricum-core test_adaptive_llm_budget`
Expected: PASS

**Step 5: Run full test suite**

Run: `cargo test --workspace`
Expected: All pass

**Step 6: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs
git commit -m "feat: P15 adaptive LLM call budget for modular migration"
```

---

### Task 2: P13 — Re-translate on High Error Count

If initial translation produces >100 compilation errors, discard and re-translate with higher temperature instead of entering the costly repair loop. This would have saved ~40 min on mz_p8 (251 errors).

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs:1794-1810` (before repair loop in migrate_file_modular)
- Test: `crates/noricum-core/src/orchestrator.rs` (unit test)

**Step 1: Write the test**

```rust
#[test]
fn test_should_retranslate_on_high_errors() {
    assert!(should_retranslate(150, 0)); // 150 errors, no retranslation yet
    assert!(should_retranslate(101, 0));
    assert!(!should_retranslate(100, 0)); // exactly 100 = try repair
    assert!(!should_retranslate(50, 0));  // low errors = repair
    assert!(!should_retranslate(200, 1)); // already retranslated once = don't loop
    assert!(!should_retranslate(200, 2)); // max 1 retranslation
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p noricum-core test_should_retranslate_on_high_errors`
Expected: FAIL

**Step 3: Implement helper**

```rust
const RETRANSLATE_ERROR_THRESHOLD: usize = 100;

/// Returns true if the module should be re-translated instead of repaired.
/// Criteria: more than 100 compilation errors AND hasn't been re-translated yet.
fn should_retranslate(error_count: usize, retranslation_attempts: u32) -> bool {
    error_count > RETRANSLATE_ERROR_THRESHOLD && retranslation_attempts == 0
}
```

**Step 4: Integrate into migrate_file_modular()**

In the module loop, after initial validation (`mod_validation`) and before the repair loop (around line 1794), add:

```rust
// P13: Re-translate if error count is catastrophically high
if !mod_validation.passed {
    let error_count = mod_validation.errors.len();
    if should_retranslate(error_count, 0) {
        warn!(
            module = %module_label,
            errors = error_count,
            "P13: error count exceeds threshold, re-translating with higher temperature"
        );
        if let Some(ref store) = artifact_store {
            store.save_retranslation_stall(0, &rust_code);
        }
        // Re-translate with temperature 0.5
        let retranslated = if module_lines > MEDIUM_FILE_LOC {
            let chunks = noricum_tools::ast::chunk_c_source(&augmented_c, chunk_target);
            match noricum_agents::translation::translate_chunked(
                &chunks, &module.name, difficulty, config, &patterns, 0.5,
            ).await {
                Ok(result) => { total_metrics.llm_calls += result.llm_calls as u32; Some(result.combined) }
                Err(_) => None,
            }
        } else {
            match noricum_agents::translation::translate_function_with_patterns_and_temperature(
                &augmented_c, &module.name, difficulty, config, &patterns, 0.5,
            ).await {
                Ok((code, _)) => { total_metrics.llm_calls += 1; Some(code) }
                Err(_) => None,
            }
        };
        if let Some(new_code) = retranslated {
            rust_code = new_code;
            // Re-validate
            mod_validation = noricum_validation::validate(/* same args */);
            info!(
                module = %module_label,
                compiles = mod_validation.compiles,
                score = mod_validation.score,
                errors = mod_validation.errors.len(),
                "P13: re-translation validation"
            );
        }
    }
}
```

Note: The exact validation call parameters should match the existing pattern at the initial validation site. Copy the same arguments.

**Step 5: Run tests**

Run: `cargo test -p noricum-core test_should_retranslate_on_high_errors && cargo check --workspace`
Expected: All pass, compiles clean

**Step 6: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs
git commit -m "feat: P13 re-translate modules with >100 compilation errors"
```

---

### Task 3: P14 — API Error Retry with Backoff

Add retry logic for transient API errors (HTTP decode errors, 502/503). Currently mz_p2 was lost entirely due to one decode error.

**Files:**
- Modify: `crates/noricum-agents/src/translation.rs:76-170` (translate_function_with_patterns_and_temperature)
- Modify: `crates/noricum-agents/src/translation.rs:174-480` (translate_chunked)
- Test: `crates/noricum-agents/src/translation.rs`

**Step 1: Write retry helper**

Add to `crates/noricum-agents/src/translation.rs`:

```rust
/// Check if an error is transient (retryable).
fn is_transient_error(err: &anyhow::Error) -> bool {
    let msg = format!("{err:?}");
    msg.contains("502") || msg.contains("503") || msg.contains("504")
        || msg.contains("Bad Gateway") || msg.contains("Service Unavailable")
        || msg.contains("error decoding response body")
        || msg.contains("connection reset")
        || msg.contains("timed out")
}

/// Retry delays in seconds: [5, 15, 30]
const RETRY_DELAYS: &[u64] = &[5, 15, 30];
```

**Step 2: Write the test**

```rust
#[test]
fn test_is_transient_error() {
    let e502 = anyhow::anyhow!("HttpError: 502 Bad Gateway");
    assert!(is_transient_error(&e502));
    let decode = anyhow::anyhow!("Http client error: error decoding response body");
    assert!(is_transient_error(&decode));
    let auth = anyhow::anyhow!("401 Unauthorized");
    assert!(!is_transient_error(&auth));
    let budget = anyhow::anyhow!("token budget exceeded");
    assert!(!is_transient_error(&budget));
}
```

**Step 3: Run test to verify it fails, then passes**

Run: `cargo test -p noricum-agents test_is_transient_error`

**Step 4: Wrap translation calls with retry**

In `translate_function_with_patterns_and_temperature()`, wrap the core LLM call in a retry loop:

```rust
let mut last_err = None;
for attempt in 0..=RETRY_DELAYS.len() {
    match /* existing LLM call */ {
        Ok(response) => { /* existing success handling */ }
        Err(e) => {
            if attempt < RETRY_DELAYS.len() && is_transient_error(&e) {
                let delay = RETRY_DELAYS[attempt];
                warn!(attempt = attempt + 1, delay_s = delay, error = %e, "transient error, retrying");
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                last_err = Some(e);
                continue;
            }
            return Err(e);
        }
    }
    break;
}
```

Apply the same pattern in `translate_chunked()` around the per-chunk LLM call (line ~385).

Also apply in `crates/noricum-agents/src/repair.rs` for the repair LLM call.

**Step 5: Run full test suite**

Run: `cargo test --workspace && cargo check --workspace`
Expected: All pass

**Step 6: Commit**

```bash
git add crates/noricum-agents/src/translation.rs crates/noricum-agents/src/repair.rs
git commit -m "feat: P14 retry transient API errors with backoff"
```

---

### Task 4: P17 — Configurable Sub-Module Target Size

Make the sub-module target LOC configurable via `MigrationConfig` instead of hardcoded 600. DeepSeek R1 needs ~500 LOC targets while Claude can handle ~700.

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs:38-87` (MigrationConfig)
- Modify: `crates/noricum-tools/src/ast.rs:868-890` (split_into_modules MAX_MODULE_LOC and target)
- Modify: `crates/noricum-tools/src/ast.rs:945` (sub_split_function_group)
- Modify: `crates/noricum-cli/src/main.rs` (new CLI flag)
- Test: `crates/noricum-tools/src/ast.rs`

**Step 1: Write the test**

In `crates/noricum-tools/src/ast.rs` tests:

```rust
#[test]
fn test_split_into_modules_custom_target() {
    // Create a source with many functions to test custom target sizing
    let mut source = String::new();
    for i in 0..20 {
        source.push_str(&format!(
            "void mz_func{}(int x) {{\n{}\n}}\n\n",
            i,
            "    int a = 1;\n".repeat(40) // ~40 lines each = 800 total
        ));
    }
    // With target 200 lines, should produce more modules than default 600
    let modules_small = split_into_modules(&source, Some(200));
    let modules_default = split_into_modules(&source, None);
    assert!(modules_small.len() > modules_default.len(),
        "smaller target should produce more modules: {} vs {}",
        modules_small.len(), modules_default.len());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p noricum-tools test_split_into_modules_custom_target`
Expected: FAIL — `split_into_modules` doesn't accept a target parameter

**Step 3: Implement**

Change `split_into_modules()` signature in `ast.rs` from:
```rust
pub fn split_into_modules(c_source: &str) -> Vec<CModule>
```
to:
```rust
pub fn split_into_modules(c_source: &str, target_module_loc: Option<usize>) -> Vec<CModule>
```

Inside the function, replace the hardcoded constants:
```rust
let max_module_loc = target_module_loc.unwrap_or(1000);
let sub_target = (max_module_loc * 3) / 5; // 60% of max, e.g. 600 for 1000, 300 for 500
```

Update all callers of `split_into_modules()` (in `orchestrator.rs`) to pass the config value.

Add to `MigrationConfig`:
```rust
/// Target maximum LOC per sub-module in modular migration (default: 1000)
pub module_target_loc: Option<usize>,
```

Default: `module_target_loc: None` (uses 1000).

Add CLI flag in `MigrateOpts`:
```rust
/// Target LOC per module for modular migration (default: 1000, use 500 for DeepSeek R1)
#[arg(long)]
module_target_loc: Option<usize>,
```

**Step 4: Run tests**

Run: `cargo test --workspace`
Expected: All pass

**Step 5: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs crates/noricum-tools/src/ast.rs crates/noricum-cli/src/main.rs crates/noricum-cli/src/commands/migrate.rs
git commit -m "feat: P17 configurable sub-module target LOC"
```

---

### Task 5: P18 — Wave-Based Parallel Module Translation

Translate independent modules concurrently using tokio::JoinSet. Modules at the same dependency level can run in parallel.

**Files:**
- Modify: `crates/noricum-core/src/dependency.rs:205-267` (add module_waves)
- Modify: `crates/noricum-core/src/orchestrator.rs:1624-1950` (replace sequential loop with wave-based)
- Test: `crates/noricum-core/src/dependency.rs`

**Step 1: Write the test for module_waves()**

In `dependency.rs` tests:

```rust
#[test]
fn test_module_waves() {
    // Module 0 depends on nothing, modules 1-2 depend on 0, module 3 depends on 1
    let mut graph = DependencyGraph::new();
    // ... build graph with known dependencies ...
    let modules = vec![/* 4 test modules */];
    let waves = graph.module_waves(&modules);
    // Wave 0: [0] (no deps)
    // Wave 1: [1, 2] (both depend on 0, parallel)
    // Wave 2: [3] (depends on 1)
    assert_eq!(waves.len(), 3);
    assert_eq!(waves[0].len(), 1);
    assert_eq!(waves[1].len(), 2);
    assert_eq!(waves[2].len(), 1);
}
```

**Step 2: Implement module_waves()**

Add to `DependencyGraph` in `dependency.rs`:

```rust
/// Group modules into waves of independent modules that can be processed in parallel.
/// Each wave contains modules whose dependencies are all satisfied by previous waves.
pub fn module_waves(&self, modules: &[CModule]) -> Vec<Vec<usize>> {
    let order = self.module_order(modules);
    if order.is_empty() {
        return vec![];
    }

    // Build adjacency: which modules does each module depend on?
    let n = modules.len();
    let mut in_degree = vec![0u32; n];
    let mut dependents: Vec<Vec<usize>> = vec![vec![]; n];

    for (i, mi) in modules.iter().enumerate() {
        for (j, mj) in modules.iter().enumerate() {
            if i == j { continue; }
            // Check if module i calls any function in module j
            let i_calls_j = mi.function_names.iter().any(|fname| {
                // Check if any function in j is called by functions in i
                // Use the dependency graph edges
                mj.function_names.iter().any(|target| {
                    self.calls(fname, target)
                })
            });
            if i_calls_j {
                in_degree[i] += 1;
                dependents[j].push(i);
            }
        }
    }

    let mut waves = vec![];
    let mut remaining: Vec<bool> = vec![true; n];
    let mut current_in_degree = in_degree.clone();

    loop {
        let wave: Vec<usize> = (0..n)
            .filter(|&i| remaining[i] && current_in_degree[i] == 0)
            .collect();
        if wave.is_empty() { break; }
        for &idx in &wave {
            remaining[idx] = false;
            for &dep in &dependents[idx] {
                current_in_degree[dep] -= 1;
            }
        }
        waves.push(wave);
    }
    waves
}
```

Note: This is a sketch. Adapt the dependency lookup to use the actual `DependencyGraph` API (check how `calls()` or edge lookup works — may need to iterate `self.edges` or similar).

**Step 3: Implement wave-based loop in orchestrator**

Replace the sequential module loop in `migrate_file_modular()` with:

```rust
let waves = dep_graph.module_waves(&modules);
info!(wave_count = waves.len(), "P18: wave-based parallel migration");

let mut accumulated_rust_context = String::new();
let mut module_results: Vec<Option<ModuleResult>> = vec![None; modules.len()];

for (wave_idx, wave) in waves.iter().enumerate() {
    info!(wave = wave_idx, modules = wave.len(), "starting wave");

    if wave.len() == 1 {
        // Single module: run inline (no spawn overhead)
        let mod_idx = wave[0];
        let result = migrate_single_module(/* args */).await;
        module_results[mod_idx] = Some(result);
    } else {
        // Multiple modules: run in parallel with JoinSet
        let mut join_set = tokio::task::JoinSet::new();
        for &mod_idx in wave {
            let module = modules[mod_idx].clone();
            let config = effective_config.clone();
            let context = accumulated_rust_context.clone();
            let patterns = patterns.clone();
            // ... clone other needed data ...
            join_set.spawn(async move {
                let result = migrate_single_module_inner(/* args */).await;
                (mod_idx, result)
            });
        }
        while let Some(res) = join_set.join_next().await {
            let (mod_idx, result) = res?;
            module_results[mod_idx] = Some(result);
        }
    }

    // After wave completes, accumulate context from all modules in this wave
    for &mod_idx in wave {
        if let Some(ref result) = module_results[mod_idx] {
            // Append validated signatures to accumulated_rust_context
            accumulated_rust_context.push_str(&result.signatures);
        }
    }
}
```

This requires extracting the per-module migration logic into a separate async function `migrate_single_module()` that takes all needed context by value (for `Send + 'static`). This is the main refactoring effort.

**Step 4: Extract per-module migration function**

Create a new function (in orchestrator.rs):

```rust
struct ModuleResult {
    rust_code: String,
    state: MigrationState,
    score: f64,
    unsafe_count: u32,
    signatures: String,
    llm_calls: u32,
}

async fn migrate_single_module(
    module: &CModule,
    module_label: &str,
    difficulty: Difficulty,
    config: &MigrationConfig,
    patterns: &[Pattern],
    accumulated_context: &str,
    analysis_strategy: &str,
    artifact_store: Option<&ArtifactStore>,
) -> Result<ModuleResult> {
    // Move existing per-module logic (lines ~1640-1940) here
    // Return ModuleResult with the outputs
}
```

**Step 5: Run tests**

Run: `cargo test --workspace`
Expected: All pass

**Step 6: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs crates/noricum-core/src/dependency.rs
git commit -m "feat: P18 wave-based parallel module translation"
```

---

### Task 6: P20 — Artifact Load API

Add `load_*` methods to `ArtifactStore` and a manifest with module scores/states.

**Files:**
- Modify: `crates/noricum-core/src/artifacts.rs`
- Test: `crates/noricum-core/src/artifacts.rs`

**Step 1: Define manifest struct and test**

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArtifactManifest {
    pub version: u32,
    pub function_name: String,
    pub timestamp: String,
    pub modules: Vec<ModuleArtifact>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModuleArtifact {
    pub name: String,
    pub state: String,       // "Validated", "FallbackUnsafe", "Skipped"
    pub score: f64,
    pub compiles: bool,
    pub unsafe_count: u32,
}
```

**Step 2: Write test for save + load roundtrip**

```rust
#[test]
fn test_artifact_manifest_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(dir.path()).unwrap();

    let manifest = ArtifactManifest {
        version: 2,
        function_name: "test_func".to_string(),
        timestamp: "20260310-120000".to_string(),
        modules: vec![
            ModuleArtifact {
                name: "if".to_string(),
                state: "Validated".to_string(),
                score: 100.0,
                compiles: true,
                unsafe_count: 0,
            },
        ],
    };
    store.save_manifest_v2(&manifest).unwrap();
    let loaded = store.load_manifest().unwrap();
    assert_eq!(loaded.modules.len(), 1);
    assert_eq!(loaded.modules[0].name, "if");
    assert_eq!(loaded.modules[0].score, 100.0);
}

#[test]
fn test_load_module_translation() {
    let dir = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(dir.path()).unwrap();
    store.save_translation_module("mz_p1", "fn hello() {}").unwrap();
    let code = store.load_translation_module("mz_p1").unwrap();
    assert_eq!(code, Some("fn hello() {}".to_string()));
    let missing = store.load_translation_module("mz_p99").unwrap();
    assert_eq!(missing, None);
}
```

**Step 3: Implement load methods**

```rust
impl ArtifactStore {
    pub fn save_manifest_v2(&self, manifest: &ArtifactManifest) -> Result<()> {
        let path = self.run_dir.join("manifest.json");
        let json = serde_json::to_string_pretty(manifest)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    pub fn load_manifest(&self) -> Result<ArtifactManifest> {
        let path = self.run_dir.join("manifest.json");
        let content = std::fs::read_to_string(&path)?;
        let manifest: ArtifactManifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    pub fn load_translation_module(&self, module_name: &str) -> Result<Option<String>> {
        let path = self.run_dir.join("03-translation").join(format!("module-{module_name}.rs"));
        if path.exists() {
            Ok(Some(std::fs::read_to_string(&path)?))
        } else {
            Ok(None)
        }
    }

    /// Load artifact store from an existing directory (for warm-start).
    pub fn from_existing(path: &std::path::Path) -> Result<Self> {
        if !path.exists() {
            anyhow::bail!("artifact directory not found: {}", path.display());
        }
        Ok(Self { run_dir: path.to_path_buf() })
    }
}
```

**Step 4: Update save_manifest calls in orchestrator**

At the end of `migrate_file_modular()`, save the v2 manifest with module results:

```rust
if let Some(ref store) = artifact_store {
    let module_artifacts: Vec<ModuleArtifact> = module_results.iter()
        .filter_map(|r| r.as_ref())
        .map(|r| ModuleArtifact {
            name: r.name.clone(),
            state: format!("{:?}", r.state),
            score: r.score,
            compiles: r.compiles,
            unsafe_count: r.unsafe_count,
        })
        .collect();
    let manifest = ArtifactManifest {
        version: 2,
        function_name: function_name.to_string(),
        timestamp: chrono::Local::now().format("%Y%m%d-%H%M%S").to_string(),
        modules: module_artifacts,
    };
    store.save_manifest_v2(&manifest)?;
}
```

**Step 5: Run tests**

Run: `cargo test -p noricum-core test_artifact_manifest_roundtrip test_load_module_translation`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/noricum-core/src/artifacts.rs crates/noricum-core/src/orchestrator.rs
git commit -m "feat: P20 artifact load API with v2 manifest"
```

---

### Task 7: P19 — Warm-Start from Previous Artifacts

Add `--warm-start <dir>` flag that loads previous module results and skips/seeds accordingly.

**Files:**
- Modify: `crates/noricum-cli/src/main.rs` (CLI flag)
- Modify: `crates/noricum-cli/src/commands/migrate.rs` (pass to config)
- Modify: `crates/noricum-core/src/orchestrator.rs:38-87` (MigrationConfig field)
- Modify: `crates/noricum-core/src/orchestrator.rs:1585-1950` (warm-start logic)
- Test: `crates/noricum-core/src/orchestrator.rs`

**Step 1: Add config field and CLI flag**

In `MigrationConfig` (orchestrator.rs ~line 87):
```rust
/// Path to previous artifact directory for warm-start
pub warm_start: Option<std::path::PathBuf>,
```

Default: `warm_start: None`

In `MigrateOpts` (main.rs):
```rust
/// Warm-start from previous artifact directory (skip validated modules, seed repairs)
#[arg(long)]
warm_start: Option<PathBuf>,
```

Wire through in `commands/migrate.rs`.

**Step 2: Write test for warm-start decision logic**

```rust
#[test]
fn test_warm_start_strategy() {
    use crate::artifacts::ModuleArtifact;

    let validated = ModuleArtifact {
        name: "if".into(), state: "Validated".into(),
        score: 100.0, compiles: true, unsafe_count: 0,
    };
    assert_eq!(warm_start_action(&validated), WarmAction::Skip);

    let good_fallback = ModuleArtifact {
        name: "mz_p6".into(), state: "FallbackUnsafe".into(),
        score: 87.0, compiles: true, unsafe_count: 0,
    };
    assert_eq!(warm_start_action(&good_fallback), WarmAction::SeedRepair);

    let mid_fallback = ModuleArtifact {
        name: "mz_p3".into(), state: "FallbackUnsafe".into(),
        score: 50.0, compiles: true, unsafe_count: 0,
    };
    assert_eq!(warm_start_action(&mid_fallback), WarmAction::SeedRepair);

    let bad_fallback = ModuleArtifact {
        name: "mz_p8".into(), state: "FallbackUnsafe".into(),
        score: 5.0, compiles: false, unsafe_count: 0,
    };
    assert_eq!(warm_start_action(&bad_fallback), WarmAction::Retranslate);

    let skipped = ModuleArtifact {
        name: "mz_p2".into(), state: "Skipped".into(),
        score: 0.0, compiles: false, unsafe_count: 0,
    };
    assert_eq!(warm_start_action(&skipped), WarmAction::Retranslate);
}
```

**Step 3: Implement warm-start logic**

```rust
#[derive(Debug, PartialEq)]
enum WarmAction {
    Skip,         // Module was Validated — use directly
    SeedRepair,   // Module compiled but score < threshold — start from its code
    Retranslate,  // Module was garbage — re-translate from scratch
}

fn warm_start_action(artifact: &ModuleArtifact) -> WarmAction {
    if artifact.state == "Validated" && artifact.score >= 70.0 {
        WarmAction::Skip
    } else if artifact.compiles && artifact.score >= 40.0 {
        WarmAction::SeedRepair
    } else {
        WarmAction::Retranslate
    }
}
```

**Step 4: Integrate into module loop**

At the start of each module migration in `migrate_file_modular()`:

```rust
// P19: Warm-start check
if let Some(ref warm_store) = warm_artifact_store {
    if let Ok(manifest) = warm_store.load_manifest() {
        if let Some(prev) = manifest.modules.iter().find(|m| m.name == module.name) {
            match warm_start_action(prev) {
                WarmAction::Skip => {
                    if let Ok(Some(code)) = warm_store.load_translation_module(&module.name) {
                        info!(module = %module.name, score = prev.score, "P19: warm-start skip (validated)");
                        // Use previous code directly, skip to accumulation
                        module_results[mod_idx] = Some(ModuleResult {
                            rust_code: code,
                            state: MigrationState::Validated,
                            score: prev.score,
                            /* ... */
                        });
                        continue; // Skip this module entirely
                    }
                }
                WarmAction::SeedRepair => {
                    if let Ok(Some(code)) = warm_store.load_translation_module(&module.name) {
                        info!(module = %module.name, score = prev.score, "P19: warm-start seed repair");
                        rust_code = code;
                        // Skip translation, go directly to repair loop
                    }
                }
                WarmAction::Retranslate => {
                    info!(module = %module.name, score = prev.score, "P19: warm-start retranslate");
                    // Proceed with normal translation
                }
            }
        }
    }
}
```

**Step 5: Run tests**

Run: `cargo test --workspace`
Expected: All pass

**Step 6: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs crates/noricum-cli/src/main.rs crates/noricum-cli/src/commands/migrate.rs
git commit -m "feat: P19 warm-start from previous artifacts"
```

---

### Task 8: P16 — Faster Model for Repair

Use `deepseek-chat` instead of `deepseek-reasoner` for repair calls when the primary provider is DeepSeek. R1 reasoning is overkill for "fix these 7 compilation errors."

**Files:**
- Modify: `crates/noricum-agents/src/providers.rs:274-325` (select_model)
- Modify: `crates/noricum-agents/src/providers.rs` (add select_repair_model)
- Modify: `crates/noricum-core/src/orchestrator.rs` (use repair model in repair loop)
- Test: `crates/noricum-agents/src/providers.rs`

**Step 1: Write test**

```rust
#[test]
fn test_select_repair_model_uses_fast_model() {
    let config = MigrationConfig {
        primary_provider: Some("deepseek".to_string()),
        deepseek_api_key: Some("test".to_string()),
        ..Default::default()
    };
    let (model_name, _) = select_repair_model(&config, Difficulty::Hard);
    // Repair should use deepseek-chat even for Hard difficulty
    assert_eq!(model_name, "deepseek-chat");
}
```

**Step 2: Implement select_repair_model()**

```rust
/// Select model for repair calls — prefers faster models since repair
/// is about fixing compilation errors, not deep reasoning.
pub fn select_repair_model(config: &MigrationConfig, difficulty: Difficulty) -> (String, LlmClient) {
    // For DeepSeek: always use deepseek-chat for repair (fast, cheap)
    // For Anthropic: use Sonnet for repair (fast enough)
    // For Ollama: same as translation (no choice)
    match config.primary_provider.as_deref() {
        Some("deepseek") => {
            // Use deepseek-chat for all repair regardless of difficulty
            let client = build_deepseek_client(config, models::DEEPSEEK_CHAT);
            (models::DEEPSEEK_CHAT.to_string(), client)
        }
        _ => {
            // Fallback to normal model selection for non-DeepSeek providers
            select_model(config, difficulty)
        }
    }
}
```

**Step 3: Wire into repair calls in orchestrator**

In the modular repair loop, use `select_repair_model` instead of `select_model` when calling the repair agent.

**Step 4: Run tests**

Run: `cargo test --workspace`
Expected: All pass

**Step 5: Commit**

```bash
git add crates/noricum-agents/src/providers.rs crates/noricum-core/src/orchestrator.rs
git commit -m "feat: P16 use faster model for repair calls"
```

---

## Execution Order

Implement in this order (dependency-aware):

1. **Task 1 (P15)** — Adaptive budget *(unblocks all further testing)*
2. **Task 2 (P13)** — Re-translate on high errors *(independent)*
3. **Task 3 (P14)** — API retry *(independent)*
4. **Task 4 (P17)** — Configurable module target *(independent)*
5. **Task 8 (P16)** — Fast repair model *(independent)*
6. **Task 6 (P20)** — Artifact load API *(prerequisite for P19)*
7. **Task 7 (P19)** — Warm-start *(depends on P20)*
8. **Task 5 (P18)** — Parallel waves *(biggest refactor, do last)*

Tasks 1-5 are independent quick wins (~30 min each). Tasks 6-7 build on each other (~1h). Task 8 is the largest refactor (~2h).

## Verification

After all tasks, run:
```bash
cargo test --workspace
cargo clippy --workspace
```

Then retry miniz_zip.c:
```bash
set -a && source .env && set +a
cargo run -p noricum-cli -- migrate tests/fixtures/miniz/miniz_zip.c \
    --provider deepseek --skip-c2rust --diff-test \
    --artifacts-dir .noricum-artifacts \
    --module-target-loc 500

# Second run with warm-start:
cargo run -p noricum-cli -- migrate tests/fixtures/miniz/miniz_zip.c \
    --provider deepseek --skip-c2rust --diff-test \
    --artifacts-dir .noricum-artifacts \
    --module-target-loc 500 \
    --warm-start .noricum-artifacts/miniz_zip-<timestamp>
```
