/// Migration orchestrator: drives the per-function state machine.
///
/// The orchestrator takes a C source file through the full pipeline:
/// Pending -> Extracted -> Characterized -> C2RustDone -> Analyzed -> Refined -> Validated
///
/// For v0, the pipeline is simplified:
/// 1. Extract C source
/// 2. Run C2Rust (mechanical translation)
/// 3. Validate the output
///
/// LLM agents (analysis, refinement, repair) will be wired in Phase 1.
use std::path::Path;

use noricum_ir::{FunctionUnit, MigrationProject, MigrationState};
use tracing::info;

use crate::CoreError;

/// Run the migration pipeline on a single C file.
///
/// For v0, this runs:
/// 1. Read C source
/// 2. Attempt C2Rust transpilation
/// 3. Validate result
///
/// Returns the updated FunctionUnit with migration state.
pub fn migrate_file(c_file: &Path) -> Result<FunctionUnit, CoreError> {
    let source_path = c_file.to_string_lossy().to_string();
    let c_source = std::fs::read_to_string(c_file)?;

    let name = c_file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    info!(function = %name, file = %source_path, "starting migration");

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
            info!(function = %unit.name, error = %e, "c2rust not available, skipping mechanical translation");
            // Without C2Rust, we'll need the LLM to translate from scratch (Phase 1)
            // For now, mark as extracted and move on
            unit.state = MigrationState::Extracted;
            return Ok(unit);
        }
    }

    // Step 2: Validate
    let validation = noricum_validation::validate(&unit)?;
    noricum_validation::apply_validation(&mut unit, &validation);

    info!(
        function = %unit.name,
        state = ?unit.state,
        score = ?unit.idiomatic_score,
        "migration complete"
    );

    Ok(unit)
}

/// Run migration on all C files in a directory.
pub fn migrate_directory(dir: &Path) -> Result<MigrationProject, CoreError> {
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
            let unit = migrate_file(&path)?;
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
        "directory migration complete"
    );

    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrate_file_without_c2rust() {
        // Without c2rust installed, migration should still extract the source
        let tmp = tempfile::tempdir().unwrap();
        let c_file = tmp.path().join("test.c");
        std::fs::write(&c_file, "int add(int a, int b) { return a + b; }").unwrap();

        let unit = migrate_file(&c_file).unwrap();
        assert_eq!(unit.name, "test");
        assert!(!unit.c_source.is_empty());
        // Without c2rust, state should be Extracted
        assert_eq!(unit.state, MigrationState::Extracted);
    }

    #[test]
    fn test_migrate_directory() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.c"), "int a() { return 1; }").unwrap();
        std::fs::write(tmp.path().join("b.c"), "int b() { return 2; }").unwrap();

        let project = migrate_directory(tmp.path()).unwrap();
        assert_eq!(project.units.len(), 2);
    }

    #[test]
    fn test_migrate_directory_no_c_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("readme.txt"), "hello").unwrap();

        let result = migrate_directory(tmp.path());
        assert!(result.is_err());
    }
}
