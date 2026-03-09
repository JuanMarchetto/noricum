# Pipeline Artifact Persistence Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ensure every intermediate product of the migration pipeline is persisted to disk, so no output is ever lost regardless of crashes, errors, or which pipeline path is taken.

**Architecture:** New `ArtifactStore` struct in `noricum-core` that creates a timestamped run directory and exposes typed methods for saving each artifact. The store is created at the start of `migrate_file()` and threaded through the pipeline. Always-on by default with a configurable output directory.

**Tech Stack:** Rust std::fs for I/O, serde_json for structured data, existing project patterns (thiserror, tracing).

---

## What Gets Lost Today (and What This Plan Fixes)

| Artifact | Where It's Lost | Fix |
|----------|----------------|-----|
| `AnalysisResult` | Local variable in `migrate_file()` | Save as JSON after analysis agent |
| Per-chunk Rust outputs | Combined in `translate_chunked()`, chunks dropped | Save each chunk before combination |
| Per-module Rust outputs | Combined in `assemble_module_outputs()`, dropped | Save each module before assembly |
| Agreed signatures (P11) | Local variable in `translate_chunked()` | Save before chunked translation |
| Foundation context (P9) | Local variable in `translate_chunked()` | Save chunk-0 foundation |
| Repair iteration N outputs | Overwritten by iteration N+1 on `unit.rust_output` | Save each iteration's Rust + validation |
| Re-translation outputs | Lost if not better than current | Save with "retranslation" label |
| Best version tracking | Local variables in repair loop | Save best version when updated |
| Idiomatic hints | Generated on-the-fly, never saved | Save as part of repair iteration context |
| Validation results per iteration | Overwritten each time | Save as JSON per iteration |
| Initial validation (pre-repair) | Overwritten by first repair | Save separately |

## Directory Structure

```
.noricum-artifacts/
  {name}-{YYYYMMDD-HHMMSS}/
    manifest.json                      # Run metadata + final summary
    00-c-source.c                      # Original C input
    01-analysis.json                   # AnalysisResult from analysis agent
    02-c2rust.rs                       # C2Rust output (if produced)
    03-translation/
      final.rs                         # Combined translation output
      chunk-00.rs                      # Per-chunk (if chunked translation)
      chunk-01.rs
      ...
      agreed-signatures.rs             # P11 signatures (if applicable)
      foundation.rs                    # P9 foundation context (if applicable)
      module-{name}.rs                 # Per-module (if modular migration)
      ...
      retranslation-stub.rs            # P6 re-translation (if triggered)
      retranslation-unsafe.rs          # Quality gate re-translation (if triggered)
    04-validation-initial.json         # First validation result
    05-repair/
      iter-01.rs                       # Rust code at start of iteration 1
      iter-01-validation.json          # Validation result after repair
      iter-01-rejected.rs              # If P0 quality floor rejected it
      iter-02.rs
      iter-02-validation.json
      ...
      retranslation-stall.rs           # Stall-triggered re-translation (if any)
      best-version.rs                  # Final best-tracked version
    06-final.rs                        # Final output (what ends up in unit.rust_output)
    07-tests.rs                        # Generated tests (if any)
```

---

### Task 1: Define `ArtifactStore` struct and constructor

**Files:**
- Create: `crates/noricum-core/src/artifacts.rs`
- Modify: `crates/noricum-core/src/lib.rs` (add `pub mod artifacts;`)

**Step 1: Write the failing test**

```rust
// In crates/noricum-core/src/artifacts.rs at the bottom
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_artifact_store_creates_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ArtifactStore::new(tmp.path(), "test_func").unwrap();
        assert!(store.run_dir().exists());
        // Directory name should contain the function name
        let dir_name = store.run_dir().file_name().unwrap().to_string_lossy();
        assert!(dir_name.contains("test_func"), "dir name should contain function name: {dir_name}");
    }

    #[test]
    fn test_artifact_store_saves_c_source() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ArtifactStore::new(tmp.path(), "add").unwrap();
        store.save_c_source("int add(int a, int b) { return a + b; }").unwrap();
        let saved = std::fs::read_to_string(store.run_dir().join("00-c-source.c")).unwrap();
        assert_eq!(saved, "int add(int a, int b) { return a + b; }");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p noricum-core artifacts::tests --no-run 2>&1 | head -20`
Expected: FAIL — module `artifacts` doesn't exist yet

**Step 3: Write minimal implementation**

```rust
// crates/noricum-core/src/artifacts.rs

/// Pipeline artifact persistence.
///
/// Saves every intermediate product of the migration pipeline to disk
/// so no output is ever lost. Creates a timestamped run directory per
/// migration and provides typed methods for each artifact kind.
use std::path::{Path, PathBuf};

use chrono::Utc;
use tracing::debug;

/// Persists all intermediate pipeline artifacts to a run directory.
#[derive(Debug, Clone)]
pub struct ArtifactStore {
    run_dir: PathBuf,
}

impl ArtifactStore {
    /// Create a new artifact store for a migration run.
    ///
    /// Creates `{base_dir}/{name}-{YYYYMMDD-HHMMSS}/` and required subdirectories.
    pub fn new(base_dir: &Path, function_name: &str) -> Result<Self, std::io::Error> {
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let dir_name = format!("{function_name}-{timestamp}");
        let run_dir = base_dir.join(dir_name);
        std::fs::create_dir_all(&run_dir)?;
        std::fs::create_dir_all(run_dir.join("03-translation"))?;
        std::fs::create_dir_all(run_dir.join("05-repair"))?;
        debug!(dir = %run_dir.display(), "artifact store created");
        Ok(Self { run_dir })
    }

    /// Path to this run's artifact directory.
    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    /// Save the original C source.
    pub fn save_c_source(&self, c_source: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join("00-c-source.c");
        std::fs::write(&path, c_source)?;
        debug!(path = %path.display(), "saved C source artifact");
        Ok(())
    }
}
```

Also add `pub mod artifacts;` to `crates/noricum-core/src/lib.rs`.

**Step 4: Run test to verify it passes**

Run: `cargo test -p noricum-core artifacts::tests -v`
Expected: 2 tests PASS

**Step 5: Commit**

```bash
git add crates/noricum-core/src/artifacts.rs crates/noricum-core/src/lib.rs
git commit -m "feat: add ArtifactStore struct with constructor and C source save"
```

---

### Task 2: Add save methods for all artifact types

**Files:**
- Modify: `crates/noricum-core/src/artifacts.rs`

**Step 1: Write failing tests for each save method**

```rust
#[test]
fn test_save_analysis() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_analysis("{\"difficulty\":\"easy\"}").unwrap();
    let saved = std::fs::read_to_string(store.run_dir().join("01-analysis.json")).unwrap();
    assert!(saved.contains("easy"));
}

#[test]
fn test_save_c2rust() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_c2rust("unsafe fn add() {}").unwrap();
    let saved = std::fs::read_to_string(store.run_dir().join("02-c2rust.rs")).unwrap();
    assert!(saved.contains("unsafe"));
}

#[test]
fn test_save_translation_final() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_translation_final("fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();
    let saved = std::fs::read_to_string(store.run_dir().join("03-translation/final.rs")).unwrap();
    assert!(saved.contains("fn add"));
}

#[test]
fn test_save_translation_chunk() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_translation_chunk(0, "// chunk 0").unwrap();
    store.save_translation_chunk(1, "// chunk 1").unwrap();
    assert!(store.run_dir().join("03-translation/chunk-00.rs").exists());
    assert!(store.run_dir().join("03-translation/chunk-01.rs").exists());
}

#[test]
fn test_save_agreed_signatures() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_agreed_signatures("fn foo() -> i32;").unwrap();
    assert!(store.run_dir().join("03-translation/agreed-signatures.rs").exists());
}

#[test]
fn test_save_foundation() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_foundation("struct Foo {}").unwrap();
    assert!(store.run_dir().join("03-translation/foundation.rs").exists());
}

#[test]
fn test_save_translation_module() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_translation_module("parser", "fn parse() {}").unwrap();
    assert!(store.run_dir().join("03-translation/module-parser.rs").exists());
}

#[test]
fn test_save_retranslation() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_retranslation("stub", "fn foo() {}").unwrap();
    assert!(store.run_dir().join("03-translation/retranslation-stub.rs").exists());
}

#[test]
fn test_save_initial_validation() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_initial_validation("{\"compiles\":true}").unwrap();
    assert!(store.run_dir().join("04-validation-initial.json").exists());
}

#[test]
fn test_save_repair_iteration() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_repair_iteration(1, "fn repaired() {}", "{\"compiles\":true}").unwrap();
    assert!(store.run_dir().join("05-repair/iter-01.rs").exists());
    assert!(store.run_dir().join("05-repair/iter-01-validation.json").exists());
}

#[test]
fn test_save_repair_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_repair_rejected(1, "unsafe fn bad() {}").unwrap();
    assert!(store.run_dir().join("05-repair/iter-01-rejected.rs").exists());
}

#[test]
fn test_save_retranslation_stall() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_retranslation_stall("fn fresh() {}").unwrap();
    assert!(store.run_dir().join("05-repair/retranslation-stall.rs").exists());
}

#[test]
fn test_save_best_version() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_best_version("fn best() {}", 85, 0, true).unwrap();
    assert!(store.run_dir().join("05-repair/best-version.rs").exists());
}

#[test]
fn test_save_final_output() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_final_output("fn final_out() {}").unwrap();
    assert!(store.run_dir().join("06-final.rs").exists());
}

#[test]
fn test_save_generated_tests() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_generated_tests("#[test] fn test_it() {}").unwrap();
    assert!(store.run_dir().join("07-tests.rs").exists());
}

#[test]
fn test_save_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ArtifactStore::new(tmp.path(), "f").unwrap();
    store.save_manifest("{\"name\":\"f\",\"state\":\"Validated\"}").unwrap();
    assert!(store.run_dir().join("manifest.json").exists());
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p noricum-core artifacts::tests --no-run 2>&1 | head -20`
Expected: FAIL — methods don't exist yet

**Step 3: Implement all save methods**

Add these methods to `impl ArtifactStore`:

```rust
    /// Save analysis agent result as JSON.
    pub fn save_analysis(&self, analysis_json: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join("01-analysis.json");
        std::fs::write(&path, analysis_json)?;
        debug!(path = %path.display(), "saved analysis artifact");
        Ok(())
    }

    /// Save C2Rust transpilation output.
    pub fn save_c2rust(&self, rust_source: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join("02-c2rust.rs");
        std::fs::write(&path, rust_source)?;
        debug!(path = %path.display(), "saved c2rust artifact");
        Ok(())
    }

    /// Save the final combined translation output.
    pub fn save_translation_final(&self, rust_source: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join("03-translation/final.rs");
        std::fs::write(&path, rust_source)?;
        debug!(path = %path.display(), "saved translation final artifact");
        Ok(())
    }

    /// Save a single chunk's translation output.
    pub fn save_translation_chunk(&self, chunk_index: usize, rust_source: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join(format!("03-translation/chunk-{chunk_index:02}.rs"));
        std::fs::write(&path, rust_source)?;
        debug!(path = %path.display(), chunk = chunk_index, "saved chunk artifact");
        Ok(())
    }

    /// Save P11 agreed signatures.
    pub fn save_agreed_signatures(&self, signatures: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join("03-translation/agreed-signatures.rs");
        std::fs::write(&path, signatures)?;
        debug!(path = %path.display(), "saved agreed signatures artifact");
        Ok(())
    }

    /// Save P9 foundation context (chunk 0 types/structs).
    pub fn save_foundation(&self, foundation: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join("03-translation/foundation.rs");
        std::fs::write(&path, foundation)?;
        debug!(path = %path.display(), "saved foundation artifact");
        Ok(())
    }

    /// Save a single module's translation output (P3 modular migration).
    pub fn save_translation_module(&self, module_name: &str, rust_source: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join(format!("03-translation/module-{module_name}.rs"));
        std::fs::write(&path, rust_source)?;
        debug!(path = %path.display(), module = module_name, "saved module artifact");
        Ok(())
    }

    /// Save a re-translation output (P6 stub gate or quality gate).
    pub fn save_retranslation(&self, reason: &str, rust_source: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join(format!("03-translation/retranslation-{reason}.rs"));
        std::fs::write(&path, rust_source)?;
        debug!(path = %path.display(), reason, "saved retranslation artifact");
        Ok(())
    }

    /// Save the initial validation result (before repair loop).
    pub fn save_initial_validation(&self, validation_json: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join("04-validation-initial.json");
        std::fs::write(&path, validation_json)?;
        debug!(path = %path.display(), "saved initial validation artifact");
        Ok(())
    }

    /// Save a repair iteration's Rust output and its validation result.
    pub fn save_repair_iteration(
        &self,
        iteration: u32,
        rust_source: &str,
        validation_json: &str,
    ) -> Result<(), std::io::Error> {
        let rs_path = self.run_dir.join(format!("05-repair/iter-{iteration:02}.rs"));
        let val_path = self.run_dir.join(format!("05-repair/iter-{iteration:02}-validation.json"));
        std::fs::write(&rs_path, rust_source)?;
        std::fs::write(&val_path, validation_json)?;
        debug!(path = %rs_path.display(), iteration, "saved repair iteration artifact");
        Ok(())
    }

    /// Save a repair iteration that was rejected by P0 quality floor.
    pub fn save_repair_rejected(&self, iteration: u32, rust_source: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join(format!("05-repair/iter-{iteration:02}-rejected.rs"));
        std::fs::write(&path, rust_source)?;
        debug!(path = %path.display(), iteration, "saved rejected repair artifact");
        Ok(())
    }

    /// Save stall-triggered re-translation output.
    pub fn save_retranslation_stall(&self, rust_source: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join("05-repair/retranslation-stall.rs");
        std::fs::write(&path, rust_source)?;
        debug!(path = %path.display(), "saved stall retranslation artifact");
        Ok(())
    }

    /// Save the current best-tracked version.
    pub fn save_best_version(
        &self,
        rust_source: &str,
        score: u32,
        unsafe_count: u32,
        compiles: bool,
    ) -> Result<(), std::io::Error> {
        let path = self.run_dir.join("05-repair/best-version.rs");
        std::fs::write(&path, rust_source)?;
        let meta_path = self.run_dir.join("05-repair/best-version-meta.json");
        let meta = format!(
            r#"{{"score":{score},"unsafe_count":{unsafe_count},"compiles":{compiles}}}"#
        );
        std::fs::write(&meta_path, meta)?;
        debug!(path = %path.display(), score, unsafe_count, compiles, "saved best version artifact");
        Ok(())
    }

    /// Save the final pipeline output.
    pub fn save_final_output(&self, rust_source: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join("06-final.rs");
        std::fs::write(&path, rust_source)?;
        debug!(path = %path.display(), "saved final output artifact");
        Ok(())
    }

    /// Save generated test code.
    pub fn save_generated_tests(&self, tests: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join("07-tests.rs");
        std::fs::write(&path, tests)?;
        debug!(path = %path.display(), "saved generated tests artifact");
        Ok(())
    }

    /// Save run manifest (metadata + final summary).
    pub fn save_manifest(&self, manifest_json: &str) -> Result<(), std::io::Error> {
        let path = self.run_dir.join("manifest.json");
        std::fs::write(&path, manifest_json)?;
        debug!(path = %path.display(), "saved manifest");
        Ok(())
    }
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p noricum-core artifacts::tests -v`
Expected: All 16 tests PASS

**Step 5: Commit**

```bash
git add crates/noricum-core/src/artifacts.rs
git commit -m "feat: add all ArtifactStore save methods with tests"
```

---

### Task 3: Add `artifacts_dir` to `MigrationConfig` and CLI flag

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs` (add field to `MigrationConfig`)
- Modify: `crates/noricum-cli/src/main.rs` (add `--artifacts-dir` CLI flag)
- Modify: `crates/noricum-cli/src/commands/migrate.rs` (pass field through `MigrateParams`)

**Step 1: Write the failing test**

```rust
// In crates/noricum-core/src/orchestrator.rs tests (or a new test)
#[test]
fn test_migration_config_default_has_artifacts_dir() {
    let config = MigrationConfig::default();
    assert_eq!(
        config.artifacts_dir,
        std::path::PathBuf::from(".noricum-artifacts")
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p noricum-core test_migration_config_default_has_artifacts_dir --no-run 2>&1 | head -20`
Expected: FAIL — field doesn't exist

**Step 3: Add `artifacts_dir` field to `MigrationConfig`**

In `crates/noricum-core/src/orchestrator.rs`, add to the `MigrationConfig` struct:

```rust
    /// Directory for pipeline artifact persistence. Every intermediate output
    /// is saved here for debugging and recovery. Defaults to `.noricum-artifacts/`.
    pub artifacts_dir: std::path::PathBuf,
```

In `Default for MigrationConfig`, add:

```rust
    artifacts_dir: std::path::PathBuf::from(".noricum-artifacts"),
```

In `crates/noricum-cli/src/commands/migrate.rs`, add to `MigrateParams`:

```rust
    pub artifacts_dir: std::path::PathBuf,
```

And pass it through in `cmd_migrate` when building `MigrationConfig`:

```rust
    artifacts_dir: opts.artifacts_dir,
```

In `crates/noricum-cli/src/main.rs`, add the clap argument to the migrate subcommand:

```rust
    #[arg(long, default_value = ".noricum-artifacts", help = "Directory for intermediate artifact persistence")]
    artifacts_dir: std::path::PathBuf,
```

And pass it through to `MigrateParams`.

**Step 4: Run tests and verify compilation**

Run: `cargo test -p noricum-core test_migration_config_default_has_artifacts_dir -v && cargo check -p noricum-cli`
Expected: test PASS, CLI compiles

**Step 5: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs crates/noricum-cli/src/main.rs crates/noricum-cli/src/commands/migrate.rs
git commit -m "feat: add artifacts_dir config field and --artifacts-dir CLI flag"
```

---

### Task 4: Integrate ArtifactStore into `migrate_file()` — stages 1-5

This is the core integration. The `ArtifactStore` gets created at the start of `migrate_file()` and called at each stage.

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs`

**Step 1: Write the failing test**

```rust
// In crates/noricum-core/src/orchestrator.rs tests or integration tests
#[tokio::test]
async fn test_migrate_file_creates_artifacts() {
    // This test requires a valid fixture and LLM — so it's an integration test.
    // For unit testing, we verify the ArtifactStore is created and basic artifacts saved.
    let tmp = tempfile::tempdir().unwrap();
    let store = crate::artifacts::ArtifactStore::new(tmp.path(), "test").unwrap();
    store.save_c_source("int x = 1;").unwrap();
    store.save_analysis(r#"{"difficulty":"easy"}"#).unwrap();
    store.save_translation_final("let x: i32 = 1;").unwrap();
    store.save_final_output("let x: i32 = 1;").unwrap();

    // Verify all files exist
    assert!(store.run_dir().join("00-c-source.c").exists());
    assert!(store.run_dir().join("01-analysis.json").exists());
    assert!(store.run_dir().join("03-translation/final.rs").exists());
    assert!(store.run_dir().join("06-final.rs").exists());
}
```

**Step 2: Run test to verify it passes (this is a sanity check test)**

Run: `cargo test -p noricum-core test_migrate_file_creates_artifacts -v`
Expected: PASS (this validates the artifact store works end-to-end before we integrate)

**Step 3: Integrate into `migrate_file()` — early stages**

In `migrate_file()`, right after creating the `FunctionUnit`, create the `ArtifactStore` and save artifacts at each stage. All `save_*` calls should use `if let Err(e) = ...` pattern to log warnings without failing the pipeline:

```rust
    // --- Initialize artifact store ---
    let artifacts = match crate::artifacts::ArtifactStore::new(&config.artifacts_dir, &name) {
        Ok(store) => {
            info!(dir = %store.run_dir().display(), "artifact store initialized");
            Some(store)
        }
        Err(e) => {
            warn!(error = %e, "failed to create artifact store, continuing without persistence");
            None
        }
    };

    // Save C source
    if let Some(ref store) = artifacts {
        if let Err(e) = store.save_c_source(&unit.c_source) {
            warn!(error = %e, "failed to save C source artifact");
        }
    }
```

Then add similar save calls at:

1. **After analysis** (~line 614): `store.save_analysis(&serde_json::to_string_pretty(&analysis)?)`
2. **After C2Rust** (~line 577): `store.save_c2rust(&rust_src)`
3. **After translation** (~line 899): `store.save_translation_final(rust_code)`
4. **After P6 retranslation** (~line 837): `store.save_retranslation("stub", &retranslated)`
5. **After quality gate retranslation** (~line 873): `store.save_retranslation("unsafe", &retranslated)`

Use this helper pattern throughout to avoid boilerplate:

```rust
// Helper macro or closure for fire-and-forget artifact saves
fn save_artifact(artifacts: &Option<crate::artifacts::ArtifactStore>, f: impl FnOnce(&crate::artifacts::ArtifactStore) -> Result<(), std::io::Error>) {
    if let Some(ref store) = artifacts {
        if let Err(e) = f(store) {
            tracing::warn!(error = %e, "failed to save artifact");
        }
    }
}
```

**Step 4: Run full test suite**

Run: `cargo test -p noricum-core -v`
Expected: All existing tests PASS (artifact saves are optional, don't affect pipeline behavior)

**Step 5: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs
git commit -m "feat: integrate ArtifactStore into migrate_file() stages 1-5"
```

---

### Task 5: Integrate ArtifactStore into validation and repair loop (stages 6-8)

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs`

**Step 1: Write a test verifying validation serialization works**

```rust
#[test]
fn test_validation_result_serializable() {
    // ValidationResult must be serializable for artifact persistence
    let vr = noricum_validation::ValidationResult {
        compiles: true,
        compiler_errors: vec![],
        clippy_warnings: vec![],
        unsafe_count: 0,
        idiomatic_score: 85,
        diff_test_passed: Some(true),
        diff_test_feedback: vec![],
        passed: true,
    };
    let json = serde_json::to_string_pretty(&vr).unwrap();
    assert!(json.contains("\"compiles\": true"));
}
```

**Step 2: Run test — check if ValidationResult derives Serialize**

Run: `cargo test -p noricum-core test_validation_result_serializable --no-run 2>&1 | head -20`

If `ValidationResult` doesn't derive `Serialize`, we'll need to add it. Check `crates/noricum-validation/src/lib.rs` for the struct definition and add `#[derive(Serialize)]` if missing.

**Step 3: Add artifact saves in the repair loop**

At each point in the repair loop (`~lines 944-1280`), add saves:

1. **After initial validation** (~line 919):
```rust
if let Some(ref store) = artifacts {
    if let Ok(json) = serde_json::to_string_pretty(&validation) {
        let _ = store.save_initial_validation(&json);
    }
}
```

2. **After each successful repair** (~line 1200, after `unit.rust_output = Some(repaired)`):
```rust
if let Some(ref store) = artifacts {
    if let Ok(val_json) = serde_json::to_string_pretty(&re_validation) {
        let _ = store.save_repair_iteration(
            iteration,
            unit.rust_output.as_deref().unwrap_or(""),
            &val_json,
        );
    }
}
```

3. **After P0 quality floor rejection** (~line 1189):
```rust
if let Some(ref store) = artifacts {
    let _ = store.save_repair_rejected(iteration, &repaired);
}
```

4. **After stall re-translation** (~line 1085):
```rust
if let Some(ref store) = artifacts {
    let _ = store.save_retranslation_stall(&retranslated);
}
```

5. **When best version updated** (~lines 1111 and 1232):
```rust
if let Some(ref store) = artifacts {
    if let Some(ref best) = best_version {
        let _ = store.save_best_version(best, best_score, best_unsafe, best_compiles);
    }
}
```

6. **After repair loop ends** (~line 1269, fallback or validated):
```rust
if let Some(ref store) = artifacts {
    if let Some(ref rust) = unit.rust_output {
        let _ = store.save_final_output(rust);
    }
}
```

**Step 4: Run full test suite**

Run: `cargo test --workspace -v 2>&1 | tail -30`
Expected: All tests PASS. Artifact saves don't affect pipeline behavior.

**Step 5: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs crates/noricum-validation/src/lib.rs
git commit -m "feat: integrate ArtifactStore into validation and repair loop"
```

---

### Task 6: Integrate ArtifactStore into chunked translation (`translate_chunked`)

The per-chunk outputs are lost inside `noricum_agents::translation::translate_chunked()`. We need to either:
- (A) Pass the `ArtifactStore` into `translate_chunked()`, or
- (B) Have `translate_chunked()` return per-chunk outputs along with the combined result.

**Option B is cleaner** — it keeps the agents crate independent of the core crate. We modify `translate_chunked` to return a struct with both the combined output and per-chunk details.

**Files:**
- Modify: `crates/noricum-agents/src/translation.rs` (return chunked details)
- Modify: `crates/noricum-core/src/orchestrator.rs` (save chunk artifacts)

**Step 1: Write failing test**

```rust
// In crates/noricum-agents/src/translation.rs tests
#[test]
fn test_chunked_result_has_chunk_details() {
    let result = ChunkedTranslationResult {
        combined: "fn a() {}\nfn b() {}".to_string(),
        chunks: vec![
            ChunkOutput { index: 0, rust_source: "fn a() {}".to_string() },
            ChunkOutput { index: 1, rust_source: "fn b() {}".to_string() },
        ],
        agreed_signatures: None,
        foundation: None,
    };
    assert_eq!(result.chunks.len(), 2);
    assert_eq!(result.chunks[0].index, 0);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p noricum-agents test_chunked_result --no-run 2>&1 | head -10`
Expected: FAIL — types don't exist

**Step 3: Add ChunkedTranslationResult and ChunkOutput types**

```rust
/// Output from a single chunk's translation.
#[derive(Debug, Clone)]
pub struct ChunkOutput {
    /// Chunk index (0-based).
    pub index: usize,
    /// Translated Rust source for this chunk.
    pub rust_source: String,
}

/// Result of chunked translation including per-chunk details.
#[derive(Debug, Clone)]
pub struct ChunkedTranslationResult {
    /// Combined Rust source from all chunks.
    pub combined: String,
    /// Individual chunk outputs (before combination).
    pub chunks: Vec<ChunkOutput>,
    /// P11 agreed signatures (if generated).
    pub agreed_signatures: Option<String>,
    /// P9 foundation context from chunk 0 (if generated).
    pub foundation: Option<String>,
}
```

Modify `translate_chunked()` to return `Result<ChunkedTranslationResult, ...>` instead of `Result<String, ...>`. Collect per-chunk outputs into `ChunkOutput` structs, capture `agreed_signatures` and `foundation_rust` before they go out of scope.

Then in the orchestrator, update the call site to destructure the result and save each chunk:

```rust
let chunked_result = noricum_agents::translation::translate_chunked(...).await?;
let rust_code = chunked_result.combined;
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
```

**Step 4: Run tests**

Run: `cargo test --workspace -v 2>&1 | tail -30`
Expected: All tests PASS

**Step 5: Commit**

```bash
git add crates/noricum-agents/src/translation.rs crates/noricum-core/src/orchestrator.rs
git commit -m "feat: return per-chunk details from translate_chunked and persist as artifacts"
```

---

### Task 7: Integrate ArtifactStore into modular migration

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs` (the `migrate_file_modular` function)

**Step 1: Read `migrate_file_modular` to understand current flow**

Read the function (likely around lines 1400+) to find where per-module outputs are produced and combined.

**Step 2: Add artifact saves for per-module outputs**

The `migrate_file_modular` function needs the `ArtifactStore` passed in. Add it as a parameter:

```rust
async fn migrate_file_modular(
    c_source: &str,
    name: &str,
    config: &MigrationConfig,
    client: &LlmClient,
    provider_config: &ProviderConfig,
    analysis: &noricum_agents::analysis::AnalysisResult,
    difficulty: noricum_ir::Difficulty,
    artifacts: &Option<crate::artifacts::ArtifactStore>,  // ADD THIS
) -> Result<ModularResult, CoreError>
```

Inside the function, save each module's output:

```rust
// After each module is translated
if let Some(ref store) = artifacts {
    let _ = store.save_translation_module(&module_name, &module_rust);
}
```

Update the call site in `migrate_file()` to pass `&artifacts`.

**Step 3: Run tests**

Run: `cargo test --workspace -v 2>&1 | tail -20`
Expected: All tests PASS

**Step 4: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs
git commit -m "feat: persist per-module translation artifacts in modular migration"
```

---

### Task 8: Save manifest and final output, add test generation artifact save

**Files:**
- Modify: `crates/noricum-core/src/orchestrator.rs`

**Step 1: Add manifest save at end of pipeline**

At the very end of `migrate_file()`, before returning the unit, save the manifest and any generated tests:

```rust
    // --- Save final artifacts ---
    if let Some(ref store) = artifacts {
        // Save final output
        if let Some(ref rust) = unit.rust_output {
            let _ = store.save_final_output(rust);
        }
        // Save generated tests
        if let Some(ref tests) = unit.generated_tests {
            let _ = store.save_generated_tests(tests);
        }
        // Save manifest
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
        info!(dir = %store.run_dir().display(), "all pipeline artifacts saved");
    }
```

**Step 2: Run tests**

Run: `cargo test --workspace -v 2>&1 | tail -20`
Expected: All tests PASS

**Step 3: Commit**

```bash
git add crates/noricum-core/src/orchestrator.rs
git commit -m "feat: save manifest and final artifacts at pipeline completion"
```

---

### Task 9: Add `ValidationResult` Serialize derive (if needed)

**Files:**
- Modify: `crates/noricum-validation/src/lib.rs`

**Step 1: Check if `ValidationResult` already derives Serialize**

Read the struct definition. If it doesn't derive `Serialize`, add it.

**Step 2: Add `#[derive(Serialize)]`**

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ValidationResult {
    // ... existing fields
}
```

Make sure `serde::Serialize` is imported.

**Step 3: Run tests**

Run: `cargo test --workspace -v 2>&1 | tail -20`
Expected: All tests PASS

**Step 4: Commit**

```bash
git add crates/noricum-validation/src/lib.rs
git commit -m "feat: derive Serialize on ValidationResult for artifact persistence"
```

---

### Task 10: Integration test — verify artifacts are created during actual migration

**Files:**
- Create: `crates/noricum-core/tests/artifact_persistence.rs`

**Step 1: Write integration test**

```rust
//! Integration test: verify that artifact persistence works with the sync pipeline.
//! The sync pipeline doesn't use LLM but still produces some artifacts.

use std::path::Path;

#[test]
fn test_artifact_store_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let store = noricum_core::artifacts::ArtifactStore::new(tmp.path(), "lifecycle_test").unwrap();

    // Simulate a full pipeline artifact trail
    store.save_c_source("int add(int a, int b) { return a + b; }").unwrap();
    store.save_analysis(r#"{"difficulty":"easy","patterns":["arithmetic"]}"#).unwrap();
    store.save_translation_final("fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();
    store.save_initial_validation(r#"{"compiles":true,"idiomatic_score":90}"#).unwrap();
    store.save_final_output("fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();
    store.save_manifest(r#"{"name":"add","state":"Validated"}"#).unwrap();

    // Verify directory structure
    let dir = store.run_dir();
    assert!(dir.join("00-c-source.c").exists());
    assert!(dir.join("01-analysis.json").exists());
    assert!(dir.join("03-translation/final.rs").exists());
    assert!(dir.join("04-validation-initial.json").exists());
    assert!(dir.join("06-final.rs").exists());
    assert!(dir.join("manifest.json").exists());

    // Verify content round-trip
    let c_source = std::fs::read_to_string(dir.join("00-c-source.c")).unwrap();
    assert_eq!(c_source, "int add(int a, int b) { return a + b; }");
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
    assert!(manifest.contains("Validated"));
}

#[test]
fn test_artifact_store_repair_loop_simulation() {
    let tmp = tempfile::tempdir().unwrap();
    let store = noricum_core::artifacts::ArtifactStore::new(tmp.path(), "repair_test").unwrap();

    store.save_c_source("int complex() { /* ... */ }").unwrap();
    store.save_translation_final("fn complex() { todo!() }").unwrap();
    store.save_initial_validation(r#"{"compiles":false}"#).unwrap();

    // Simulate 3 repair iterations
    store.save_repair_iteration(1, "fn complex() { /* attempt 1 */ }", r#"{"compiles":false}"#).unwrap();
    store.save_repair_rejected(2, "unsafe fn complex() {}").unwrap();
    store.save_repair_iteration(3, "fn complex() -> i32 { 42 }", r#"{"compiles":true,"idiomatic_score":75}"#).unwrap();
    store.save_best_version("fn complex() -> i32 { 42 }", 75, 0, true).unwrap();
    store.save_final_output("fn complex() -> i32 { 42 }").unwrap();

    // All artifacts should exist
    let dir = store.run_dir();
    assert!(dir.join("05-repair/iter-01.rs").exists());
    assert!(dir.join("05-repair/iter-01-validation.json").exists());
    assert!(dir.join("05-repair/iter-02-rejected.rs").exists());
    assert!(dir.join("05-repair/iter-03.rs").exists());
    assert!(dir.join("05-repair/best-version.rs").exists());
    assert!(dir.join("05-repair/best-version-meta.json").exists());
    assert!(dir.join("06-final.rs").exists());

    // Verify best-version meta
    let meta = std::fs::read_to_string(dir.join("05-repair/best-version-meta.json")).unwrap();
    assert!(meta.contains("\"score\":75"));
}
```

**Step 2: Run integration test**

Run: `cargo test -p noricum-core --test artifact_persistence -v`
Expected: 2 tests PASS

**Step 3: Commit**

```bash
git add crates/noricum-core/tests/artifact_persistence.rs
git commit -m "test: add integration tests for artifact persistence lifecycle"
```

---

### Task 11: Verify workspace compiles and all tests pass

**Step 1: Full workspace check**

Run: `cargo clippy --workspace -- -D warnings 2>&1 | tail -20`
Expected: 0 warnings

**Step 2: Full test suite**

Run: `cargo test --workspace 2>&1 | tail -20`
Expected: All tests PASS (existing 293+ tests + new artifact tests)

**Step 3: Commit (if any fixes needed)**

```bash
git add -A
git commit -m "fix: address clippy warnings from artifact persistence integration"
```

---

## Summary of Changes

| File | Change |
|------|--------|
| `crates/noricum-core/src/artifacts.rs` | **NEW** — `ArtifactStore` struct with 16 save methods |
| `crates/noricum-core/src/lib.rs` | Add `pub mod artifacts;` |
| `crates/noricum-core/src/orchestrator.rs` | Create `ArtifactStore`, save artifacts at every stage, add `artifacts_dir` to config |
| `crates/noricum-agents/src/translation.rs` | Return `ChunkedTranslationResult` with per-chunk details |
| `crates/noricum-validation/src/lib.rs` | Add `Serialize` derive to `ValidationResult` |
| `crates/noricum-cli/src/main.rs` | Add `--artifacts-dir` CLI flag |
| `crates/noricum-cli/src/commands/migrate.rs` | Pass `artifacts_dir` through config |
| `crates/noricum-core/tests/artifact_persistence.rs` | **NEW** — integration tests |

**Total estimated new code:** ~300 lines (`artifacts.rs`) + ~50 lines (orchestrator saves) + ~30 lines (translation return type) + ~80 lines (tests) = ~460 lines

**Key design decisions:**
1. **Always-on** — artifacts are saved by default, no opt-in needed
2. **Fire-and-forget** — artifact save failures are warned but never fail the pipeline
3. **Agents stay independent** — `noricum-agents` doesn't depend on `noricum-core`; chunked details are returned as data, not saved directly
4. **Numbered prefixes** — directory structure uses `00-`, `01-`, etc. for natural sort order matching pipeline stages
