//! Artifact persistence for migration pipeline runs.
//!
//! [`ArtifactStore`] captures every intermediate output produced during a
//! single-function migration run, writing each artifact to a deterministic
//! file-system layout under a timestamped run directory.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Utc;
use tracing::debug;

/// Sanitise a user-provided name so it is safe to use as a path component.
///
/// Replaces any character that is not alphanumeric, `_`, or `-` with `_`.
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Persists all intermediate artifacts produced during a migration run.
///
/// The directory layout is:
/// ```text
/// {base_dir}/{function_name}-{YYYYMMDD-HHMMSS}/
/// ├── 00-c-source.c
/// ├── 01-analysis.json
/// ├── 02-c2rust.rs
/// ├── 03-translation/
/// │   ├── final.rs
/// │   ├── chunk-00.rs …
/// │   ├── agreed-signatures.rs
/// │   ├── foundation.rs
/// │   ├── module-{name}.rs
/// │   └── retranslation-{reason}.rs
/// ├── 04-validation-initial.json
/// ├── 05-repair/
/// │   ├── iter-00.rs
/// │   ├── iter-00-validation.json
/// │   ├── iter-00-rejected.rs
/// │   ├── retranslation-stall.rs
/// │   ├── best-version.rs
/// │   └── best-version-meta.json
/// ├── 06-final.rs
/// ├── 07-tests.rs
/// └── manifest.json
/// ```
#[derive(Debug, Clone)]
pub struct ArtifactStore {
    run_dir: PathBuf,
}

impl ArtifactStore {
    /// Create a new artifact store, initialising the run directory and
    /// subdirectories on disk.
    ///
    /// The run directory is placed at
    /// `{base_dir}/{function_name}-{YYYYMMDD-HHMMSS}/` and the
    /// `03-translation/` and `05-repair/` subdirectories are created eagerly.
    pub fn new(base_dir: &Path, function_name: &str) -> io::Result<Self> {
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let safe_name = sanitize_name(function_name);
        let run_dir = base_dir.join(format!("{safe_name}-{timestamp}"));

        fs::create_dir_all(run_dir.join("03-translation"))?;
        fs::create_dir_all(run_dir.join("05-repair"))?;

        debug!(run_dir = %run_dir.display(), "created artifact store");

        Ok(Self { run_dir })
    }

    /// Return the root path of this run's artifact directory.
    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    // ------------------------------------------------------------------
    // Save helpers
    // ------------------------------------------------------------------

    /// Write `content` to `path` (relative to [`run_dir`](Self::run_dir)),
    /// creating parent directories as needed.
    fn write_artifact(&self, relative: &Path, content: &str) -> io::Result<()> {
        let full = self.run_dir.join(relative);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full, content)?;
        debug!(path = %full.display(), bytes = content.len(), "saved artifact");
        Ok(())
    }

    // ------------------------------------------------------------------
    // Public save methods
    // ------------------------------------------------------------------

    /// Save the original C source (`00-c-source.c`).
    pub fn save_c_source(&self, c_source: &str) -> io::Result<()> {
        self.write_artifact(Path::new("00-c-source.c"), c_source)
    }

    /// Save the analysis JSON (`01-analysis.json`).
    pub fn save_analysis(&self, analysis_json: &str) -> io::Result<()> {
        self.write_artifact(Path::new("01-analysis.json"), analysis_json)
    }

    /// Save the C2Rust mechanical translation (`02-c2rust.rs`).
    pub fn save_c2rust(&self, rust_source: &str) -> io::Result<()> {
        self.write_artifact(Path::new("02-c2rust.rs"), rust_source)
    }

    /// Save the P33 type contract (`02-type-contract.rs`).
    pub fn save_type_contract(&self, rust_source: &str) -> io::Result<()> {
        self.write_artifact(Path::new("02-type-contract.rs"), rust_source)
    }

    /// Save the final merged translation (`03-translation/final.rs`).
    pub fn save_translation_final(&self, rust_source: &str) -> io::Result<()> {
        self.write_artifact(Path::new("03-translation/final.rs"), rust_source)
    }

    /// Save a single translation chunk (`03-translation/chunk-{index:02}.rs`).
    pub fn save_translation_chunk(&self, chunk_index: usize, rust_source: &str) -> io::Result<()> {
        let name = format!("03-translation/chunk-{chunk_index:02}.rs");
        self.write_artifact(Path::new(&name), rust_source)
    }

    /// Save the agreed-upon signatures (`03-translation/agreed-signatures.rs`).
    pub fn save_agreed_signatures(&self, signatures: &str) -> io::Result<()> {
        self.write_artifact(Path::new("03-translation/agreed-signatures.rs"), signatures)
    }

    /// Save the foundation types extracted from chunk 0
    /// (`03-translation/foundation.rs`).
    pub fn save_foundation(&self, foundation: &str) -> io::Result<()> {
        self.write_artifact(Path::new("03-translation/foundation.rs"), foundation)
    }

    /// Save a translated module (`03-translation/module-{name}.rs`).
    pub fn save_translation_module(&self, module_name: &str, rust_source: &str) -> io::Result<()> {
        let safe = sanitize_name(module_name);
        let name = format!("03-translation/module-{safe}.rs");
        self.write_artifact(Path::new(&name), rust_source)
    }

    /// Save a re-translation attempt (`03-translation/retranslation-{reason}.rs`).
    pub fn save_retranslation(&self, reason: &str, rust_source: &str) -> io::Result<()> {
        let safe = sanitize_name(reason);
        let name = format!("03-translation/retranslation-{safe}.rs");
        self.write_artifact(Path::new(&name), rust_source)
    }

    /// Save the initial validation result (`04-validation-initial.json`).
    pub fn save_initial_validation(&self, validation_json: &str) -> io::Result<()> {
        self.write_artifact(Path::new("04-validation-initial.json"), validation_json)
    }

    /// Save a repair iteration's Rust output and its validation result.
    ///
    /// Writes `05-repair/iter-{iter:02}.rs` and
    /// `05-repair/iter-{iter:02}-validation.json`.
    pub fn save_repair_iteration(
        &self,
        iteration: u32,
        rust_source: &str,
        validation_json: &str,
    ) -> io::Result<()> {
        let rs_name = format!("05-repair/iter-{iteration:02}.rs");
        let json_name = format!("05-repair/iter-{iteration:02}-validation.json");
        self.write_artifact(Path::new(&rs_name), rust_source)?;
        self.write_artifact(Path::new(&json_name), validation_json)?;
        Ok(())
    }

    /// Save a rejected repair iteration (`05-repair/iter-{iter:02}-rejected.rs`).
    pub fn save_repair_rejected(&self, iteration: u32, rust_source: &str) -> io::Result<()> {
        let name = format!("05-repair/iter-{iteration:02}-rejected.rs");
        self.write_artifact(Path::new(&name), rust_source)
    }

    /// Save a stall-triggered re-translation (`05-repair/retranslation-stall.rs`).
    pub fn save_retranslation_stall(&self, rust_source: &str) -> io::Result<()> {
        self.write_artifact(Path::new("05-repair/retranslation-stall.rs"), rust_source)
    }

    /// Save the current best version and its metadata.
    ///
    /// Writes `05-repair/best-version.rs` and
    /// `05-repair/best-version-meta.json`.
    pub fn save_best_version(
        &self,
        rust_source: &str,
        score: u32,
        unsafe_count: u32,
        compiles: bool,
    ) -> io::Result<()> {
        self.write_artifact(Path::new("05-repair/best-version.rs"), rust_source)?;
        let meta = format!(
            "{{\"score\":{score},\"unsafe_count\":{unsafe_count},\"compiles\":{compiles}}}"
        );
        self.write_artifact(Path::new("05-repair/best-version-meta.json"), &meta)?;
        Ok(())
    }

    /// Save a module-scoped repair iteration (`05-repair/{module}/iter-{iter:02}.rs`).
    ///
    /// Used by the modular pipeline to avoid collisions between modules.
    pub fn save_module_repair_iteration(
        &self,
        module_name: &str,
        iteration: u32,
        rust_source: &str,
        validation_json: &str,
    ) -> io::Result<()> {
        let safe = sanitize_name(module_name);
        let rs_name = format!("05-repair/{safe}/iter-{iteration:02}.rs");
        let json_name = format!("05-repair/{safe}/iter-{iteration:02}-validation.json");
        self.write_artifact(Path::new(&rs_name), rust_source)?;
        self.write_artifact(Path::new(&json_name), validation_json)?;
        Ok(())
    }

    /// Save a module-scoped rejected repair (`05-repair/{module}/iter-{iter:02}-rejected.rs`).
    pub fn save_module_repair_rejected(
        &self,
        module_name: &str,
        iteration: u32,
        rust_source: &str,
    ) -> io::Result<()> {
        let safe = sanitize_name(module_name);
        let name = format!("05-repair/{safe}/iter-{iteration:02}-rejected.rs");
        self.write_artifact(Path::new(&name), rust_source)
    }

    /// Save the final migration output (`06-final.rs`).
    pub fn save_final_output(&self, rust_source: &str) -> io::Result<()> {
        self.write_artifact(Path::new("06-final.rs"), rust_source)
    }

    /// Save generated test code (`07-tests.rs`).
    pub fn save_generated_tests(&self, tests: &str) -> io::Result<()> {
        self.write_artifact(Path::new("07-tests.rs"), tests)
    }

    /// Save the generated crate directory structure as an artifact.
    ///
    /// Copies the entire crate directory into the artifact store at `08-crate/`,
    /// skipping the `target/` directory (cargo build artifacts).
    pub fn save_crate_output(&self, crate_dir: &Path) -> io::Result<()> {
        let dest = self.run_dir.join("08-crate");
        copy_dir_recursive(crate_dir, &dest)?;
        debug!(
            src = %crate_dir.display(),
            dest = %dest.display(),
            "saved crate output artifact"
        );
        Ok(())
    }

    /// Save the run manifest (`manifest.json`).
    pub fn save_manifest(&self, manifest_json: &str) -> io::Result<()> {
        self.write_artifact(Path::new("manifest.json"), manifest_json)
    }

    // ------------------------------------------------------------------
    // V2 manifest + load methods (P20)
    // ------------------------------------------------------------------

    /// Save a v2 manifest with per-module metadata (`manifest.json`).
    pub fn save_manifest_v2(&self, manifest: &ArtifactManifest) -> io::Result<()> {
        let json = serde_json::to_string_pretty(manifest).map_err(io::Error::other)?;
        self.write_artifact(Path::new("manifest.json"), &json)
    }

    /// Load the v2 manifest from this run's artifact directory.
    pub fn load_manifest(&self) -> io::Result<ArtifactManifest> {
        let path = self.run_dir.join("manifest.json");
        let content = fs::read_to_string(&path)?;
        serde_json::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    /// Load a previously saved module translation (`03-translation/module-{name}.rs`).
    /// Returns `None` if the module file does not exist.
    pub fn load_translation_module(&self, module_name: &str) -> io::Result<Option<String>> {
        let safe = sanitize_name(module_name);
        let path = self
            .run_dir
            .join(format!("03-translation/module-{safe}.rs"));
        if path.exists() {
            Ok(Some(fs::read_to_string(&path)?))
        } else {
            Ok(None)
        }
    }

    /// Open an existing artifact directory (for warm-start).
    ///
    /// Unlike [`new`](Self::new) this does **not** create any directories —
    /// it simply wraps an existing path for reading.
    pub fn from_existing(path: &Path) -> io::Result<Self> {
        if !path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("artifact directory not found: {}", path.display()),
            ));
        }
        Ok(Self {
            run_dir: path.to_path_buf(),
        })
    }
}

/// Copy a directory recursively, skipping `target/` subdirectories.
fn copy_dir_recursive(src: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            // Skip target/ directory (cargo build artifacts)
            if entry.file_name() == "target" {
                continue;
            }
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

/// V2 artifact manifest with per-module metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArtifactManifest {
    /// Manifest format version.
    pub version: u32,
    /// Name of the migrated function/file.
    pub function_name: String,
    /// ISO-ish timestamp when this run completed.
    pub timestamp: String,
    /// Per-module results.
    pub modules: Vec<ModuleArtifact>,
}

/// Per-module metadata saved in the v2 manifest.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModuleArtifact {
    /// Module name (e.g. "if", "mz_p1").
    pub name: String,
    /// Final state: "Validated", "FallbackUnsafe", "Skipped".
    pub state: String,
    /// Idiomatic score (0-100).
    pub score: f64,
    /// Whether the final output compiles.
    pub compiles: bool,
    /// Number of unsafe blocks in the final output.
    pub unsafe_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create an `ArtifactStore` in a temporary directory.
    fn make_store() -> (ArtifactStore, TempDir) {
        let tmp = TempDir::new().expect("tempdir");
        let store = ArtifactStore::new(tmp.path(), "my_func").expect("new store");
        (store, tmp)
    }

    /// The constructor must create the run directory and its subdirectories.
    #[test]
    fn new_creates_run_dir_and_subdirs() {
        let (store, _tmp) = make_store();
        assert!(store.run_dir().exists());
        assert!(store.run_dir().join("03-translation").is_dir());
        assert!(store.run_dir().join("05-repair").is_dir());
    }

    /// The run directory name must start with the function name.
    #[test]
    fn run_dir_contains_function_name() {
        let (store, _tmp) = make_store();
        let dir_name = store
            .run_dir()
            .file_name()
            .expect("file_name")
            .to_str()
            .expect("utf8");
        assert!(
            dir_name.starts_with("my_func-"),
            "expected dir to start with 'my_func-', got: {dir_name}"
        );
    }

    #[test]
    fn save_c_source() {
        let (store, _tmp) = make_store();
        store.save_c_source("int main() {}").unwrap();
        let content = fs::read_to_string(store.run_dir().join("00-c-source.c")).unwrap();
        assert_eq!(content, "int main() {}");
    }

    #[test]
    fn save_analysis() {
        let (store, _tmp) = make_store();
        store.save_analysis("{\"difficulty\":\"easy\"}").unwrap();
        let content = fs::read_to_string(store.run_dir().join("01-analysis.json")).unwrap();
        assert_eq!(content, "{\"difficulty\":\"easy\"}");
    }

    #[test]
    fn save_c2rust() {
        let (store, _tmp) = make_store();
        store.save_c2rust("fn main() {}").unwrap();
        let content = fs::read_to_string(store.run_dir().join("02-c2rust.rs")).unwrap();
        assert_eq!(content, "fn main() {}");
    }

    #[test]
    fn save_translation_final() {
        let (store, _tmp) = make_store();
        store.save_translation_final("fn main() {}").unwrap();
        let content = fs::read_to_string(store.run_dir().join("03-translation/final.rs")).unwrap();
        assert_eq!(content, "fn main() {}");
    }

    #[test]
    fn save_translation_chunk() {
        let (store, _tmp) = make_store();
        store.save_translation_chunk(0, "// chunk 0").unwrap();
        store.save_translation_chunk(3, "// chunk 3").unwrap();
        let c0 = fs::read_to_string(store.run_dir().join("03-translation/chunk-00.rs")).unwrap();
        let c3 = fs::read_to_string(store.run_dir().join("03-translation/chunk-03.rs")).unwrap();
        assert_eq!(c0, "// chunk 0");
        assert_eq!(c3, "// chunk 3");
    }

    #[test]
    fn save_agreed_signatures() {
        let (store, _tmp) = make_store();
        store.save_agreed_signatures("fn foo();").unwrap();
        let content =
            fs::read_to_string(store.run_dir().join("03-translation/agreed-signatures.rs"))
                .unwrap();
        assert_eq!(content, "fn foo();");
    }

    #[test]
    fn save_foundation() {
        let (store, _tmp) = make_store();
        store.save_foundation("struct Ctx;").unwrap();
        let content =
            fs::read_to_string(store.run_dir().join("03-translation/foundation.rs")).unwrap();
        assert_eq!(content, "struct Ctx;");
    }

    #[test]
    fn save_translation_module() {
        let (store, _tmp) = make_store();
        store
            .save_translation_module("parser", "mod parser;")
            .unwrap();
        let content =
            fs::read_to_string(store.run_dir().join("03-translation/module-parser.rs")).unwrap();
        assert_eq!(content, "mod parser;");
    }

    #[test]
    fn save_retranslation() {
        let (store, _tmp) = make_store();
        store
            .save_retranslation("too_many_unsafe", "fn safe() {}")
            .unwrap();
        let content = fs::read_to_string(
            store
                .run_dir()
                .join("03-translation/retranslation-too_many_unsafe.rs"),
        )
        .unwrap();
        assert_eq!(content, "fn safe() {}");
    }

    #[test]
    fn save_initial_validation() {
        let (store, _tmp) = make_store();
        store
            .save_initial_validation("{\"compiles\":true}")
            .unwrap();
        let content =
            fs::read_to_string(store.run_dir().join("04-validation-initial.json")).unwrap();
        assert_eq!(content, "{\"compiles\":true}");
    }

    #[test]
    fn save_repair_iteration() {
        let (store, _tmp) = make_store();
        store
            .save_repair_iteration(1, "fn repaired() {}", "{\"ok\":true}")
            .unwrap();
        let rs = fs::read_to_string(store.run_dir().join("05-repair/iter-01.rs")).unwrap();
        let json =
            fs::read_to_string(store.run_dir().join("05-repair/iter-01-validation.json")).unwrap();
        assert_eq!(rs, "fn repaired() {}");
        assert_eq!(json, "{\"ok\":true}");
    }

    #[test]
    fn save_repair_rejected() {
        let (store, _tmp) = make_store();
        store
            .save_repair_rejected(2, "fn bad() { unsafe {} }")
            .unwrap();
        let content =
            fs::read_to_string(store.run_dir().join("05-repair/iter-02-rejected.rs")).unwrap();
        assert_eq!(content, "fn bad() { unsafe {} }");
    }

    #[test]
    fn save_retranslation_stall() {
        let (store, _tmp) = make_store();
        store.save_retranslation_stall("fn fresh() {}").unwrap();
        let content =
            fs::read_to_string(store.run_dir().join("05-repair/retranslation-stall.rs")).unwrap();
        assert_eq!(content, "fn fresh() {}");
    }

    #[test]
    fn save_best_version() {
        let (store, _tmp) = make_store();
        store
            .save_best_version("fn best() {}", 85, 0, true)
            .unwrap();
        let rs = fs::read_to_string(store.run_dir().join("05-repair/best-version.rs")).unwrap();
        let meta =
            fs::read_to_string(store.run_dir().join("05-repair/best-version-meta.json")).unwrap();
        assert_eq!(rs, "fn best() {}");
        assert_eq!(meta, "{\"score\":85,\"unsafe_count\":0,\"compiles\":true}");
    }

    #[test]
    fn save_final_output() {
        let (store, _tmp) = make_store();
        store.save_final_output("fn main() {}").unwrap();
        let content = fs::read_to_string(store.run_dir().join("06-final.rs")).unwrap();
        assert_eq!(content, "fn main() {}");
    }

    #[test]
    fn save_generated_tests() {
        let (store, _tmp) = make_store();
        store.save_generated_tests("#[test] fn t() {}").unwrap();
        let content = fs::read_to_string(store.run_dir().join("07-tests.rs")).unwrap();
        assert_eq!(content, "#[test] fn t() {}");
    }

    #[test]
    fn save_manifest() {
        let (store, _tmp) = make_store();
        store.save_manifest("{\"version\":1}").unwrap();
        let content = fs::read_to_string(store.run_dir().join("manifest.json")).unwrap();
        assert_eq!(content, "{\"version\":1}");
    }

    #[test]
    fn test_artifact_manifest_roundtrip() {
        let (store, _tmp) = make_store();

        let manifest = ArtifactManifest {
            version: 2,
            function_name: "test_func".to_string(),
            timestamp: "20260310-120000".to_string(),
            modules: vec![ModuleArtifact {
                name: "if".to_string(),
                state: "Validated".to_string(),
                score: 100.0,
                compiles: true,
                unsafe_count: 0,
            }],
        };
        store.save_manifest_v2(&manifest).unwrap();
        let loaded = store.load_manifest().unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.modules.len(), 1);
        assert_eq!(loaded.modules[0].name, "if");
        assert_eq!(loaded.modules[0].score, 100.0);
    }

    #[test]
    fn test_load_module_translation() {
        let (store, _tmp) = make_store();
        store
            .save_translation_module("mz_p1", "fn hello() {}")
            .unwrap();
        let code = store.load_translation_module("mz_p1").unwrap();
        assert_eq!(code, Some("fn hello() {}".to_string()));
        let missing = store.load_translation_module("mz_p99").unwrap();
        assert_eq!(missing, None);
    }

    #[test]
    fn test_from_existing() {
        let (store, _tmp) = make_store();
        let run_dir = store.run_dir().to_path_buf();
        let loaded = ArtifactStore::from_existing(&run_dir).unwrap();
        assert_eq!(loaded.run_dir(), run_dir);
    }

    #[test]
    fn test_from_existing_missing() {
        let result = ArtifactStore::from_existing(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_save_crate_output() {
        let (store, _tmp) = make_store();

        // Create a fake crate directory
        let crate_dir = TempDir::new().expect("tempdir");
        let src_dir = crate_dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(crate_dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(src_dir.join("lib.rs"), "pub mod utils;").unwrap();
        fs::write(src_dir.join("utils.rs"), "pub fn add() {}").unwrap();
        // Create a target/ dir that should be skipped
        fs::create_dir_all(crate_dir.path().join("target/debug")).unwrap();
        fs::write(crate_dir.path().join("target/debug/binary"), "big file").unwrap();

        store.save_crate_output(crate_dir.path()).unwrap();

        let dest = store.run_dir().join("08-crate");
        assert!(dest.exists(), "08-crate dir should exist");
        assert!(dest.join("Cargo.toml").exists(), "Cargo.toml should be copied");
        assert!(dest.join("src/lib.rs").exists(), "src/lib.rs should be copied");
        assert!(dest.join("src/utils.rs").exists(), "src/utils.rs should be copied");
        assert!(!dest.join("target").exists(), "target/ should be skipped");
    }
}
