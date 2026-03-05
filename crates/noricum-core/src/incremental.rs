/// Incremental migration: migrate individual functions while keeping others in C.
///
/// Tracks per-function migration state in a `.noricum-state.json` file,
/// allowing partial migration and re-migration of specific functions.
use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::CoreError;

/// Per-function migration state for incremental mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMigrationState {
    /// Whether this function has been migrated to Rust.
    pub migrated: bool,
    /// Final migration state (e.g., "Validated", "FallbackUnsafe").
    pub state: String,
    /// Idiomatic score achieved.
    pub idiomatic_score: Option<u32>,
    /// Number of unsafe blocks.
    pub unsafe_count: Option<u32>,
    /// Rust source if migrated.
    pub rust_source: Option<String>,
}

/// Persistent state for incremental migration of a C source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalState {
    /// Source file path.
    pub source_file: String,
    /// Per-function state.
    pub functions: HashMap<String, FunctionMigrationState>,
    /// Timestamp of last run.
    pub last_run: String,
}

impl IncrementalState {
    /// Create a new empty incremental state.
    pub fn new(source_file: &str) -> Self {
        Self {
            source_file: source_file.to_string(),
            functions: HashMap::new(),
            last_run: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Load state from a directory, or create a new one.
    pub fn load_or_create(state_dir: &Path, source_file: &str) -> Result<Self, CoreError> {
        let state_path = state_file_path(state_dir, source_file);
        if state_path.exists() {
            let content = std::fs::read_to_string(&state_path)?;
            let state: IncrementalState = serde_json::from_str(&content).map_err(|e| {
                CoreError::Orchestration(format!("failed to parse incremental state: {e}"))
            })?;
            info!(
                source = source_file,
                functions = state.functions.len(),
                "loaded incremental state"
            );
            Ok(state)
        } else {
            Ok(Self::new(source_file))
        }
    }

    /// Save state to a directory.
    pub fn save(&mut self, state_dir: &Path) -> Result<(), CoreError> {
        std::fs::create_dir_all(state_dir)?;
        self.last_run = chrono::Utc::now().to_rfc3339();
        let state_path = state_file_path(state_dir, &self.source_file);
        let content = serde_json::to_string_pretty(self).map_err(|e| {
            CoreError::Orchestration(format!("failed to serialize incremental state: {e}"))
        })?;
        std::fs::write(&state_path, content)?;
        info!(path = %state_path.display(), "saved incremental state");
        Ok(())
    }

    /// Get functions that still need migration.
    pub fn unmigrated_functions(&self, all_functions: &[String]) -> Vec<String> {
        all_functions
            .iter()
            .filter(|name| {
                self.functions
                    .get(*name)
                    .map(|s| !s.migrated)
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    /// Mark a function as migrated.
    pub fn mark_migrated(
        &mut self,
        name: &str,
        state: &str,
        score: Option<u32>,
        unsafe_count: Option<u32>,
        rust_source: Option<String>,
    ) {
        self.functions.insert(
            name.to_string(),
            FunctionMigrationState {
                migrated: true,
                state: state.to_string(),
                idiomatic_score: score,
                unsafe_count,
                rust_source,
            },
        );
    }
}

/// Filter for selecting which functions to migrate.
#[derive(Debug, Clone)]
pub enum FunctionFilter {
    /// Migrate specific functions by name.
    Names(Vec<String>),
    /// Migrate functions matching a regex pattern.
    Pattern(String),
    /// Migrate all functions.
    All,
}

impl FunctionFilter {
    /// Check if a function name matches this filter.
    pub fn matches(&self, name: &str) -> bool {
        match self {
            FunctionFilter::Names(names) => names.iter().any(|n| n == name),
            FunctionFilter::Pattern(pattern) => regex::Regex::new(pattern)
                .map(|re| re.is_match(name))
                .unwrap_or(false),
            FunctionFilter::All => true,
        }
    }
}

fn state_file_path(state_dir: &Path, source_file: &str) -> std::path::PathBuf {
    let name = Path::new(source_file)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    state_dir.join(format!("{name}.noricum-state.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incremental_state_new() {
        let state = IncrementalState::new("test.c");
        assert_eq!(state.source_file, "test.c");
        assert!(state.functions.is_empty());
    }

    #[test]
    fn test_incremental_state_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut state = IncrementalState::new("test.c");
        state.mark_migrated(
            "add",
            "Validated",
            Some(95),
            Some(0),
            Some("fn add() {}".to_string()),
        );

        state.save(tmp.path()).unwrap();
        let loaded = IncrementalState::load_or_create(tmp.path(), "test.c").unwrap();

        assert_eq!(loaded.functions.len(), 1);
        assert!(loaded.functions["add"].migrated);
        assert_eq!(loaded.functions["add"].state, "Validated");
        assert_eq!(loaded.functions["add"].idiomatic_score, Some(95));
    }

    #[test]
    fn test_unmigrated_functions() {
        let mut state = IncrementalState::new("test.c");
        state.mark_migrated("add", "Validated", Some(90), Some(0), None);

        let all = vec!["add".to_string(), "sub".to_string(), "mul".to_string()];
        let unmigrated = state.unmigrated_functions(&all);
        assert_eq!(unmigrated, vec!["sub", "mul"]);
    }

    #[test]
    fn test_function_filter_names() {
        let filter = FunctionFilter::Names(vec!["add".to_string(), "sub".to_string()]);
        assert!(filter.matches("add"));
        assert!(filter.matches("sub"));
        assert!(!filter.matches("mul"));
    }

    #[test]
    fn test_function_filter_pattern() {
        let filter = FunctionFilter::Pattern("^hash_.*".to_string());
        assert!(filter.matches("hash_insert"));
        assert!(filter.matches("hash_lookup"));
        assert!(!filter.matches("create_table"));
    }

    #[test]
    fn test_function_filter_all() {
        let filter = FunctionFilter::All;
        assert!(filter.matches("anything"));
    }

    #[test]
    fn test_state_file_path() {
        let path = state_file_path(Path::new("/tmp/state"), "src/hash_table.c");
        assert_eq!(
            path.to_string_lossy(),
            "/tmp/state/hash_table.noricum-state.json"
        );
    }
}
