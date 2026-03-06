/// CRUST-Bench integration: evaluate Noricum against the CRUST-Bench dataset.
///
/// Discovers C projects in the CRUST-Bench directory structure, migrates each,
/// and produces an aggregate report for benchmarking against DARPA TRACTOR teams.
use std::path::{Path, PathBuf};

use anyhow::Result;
use noricum_core::MigrationConfig;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Configuration for CRUST-Bench evaluation.
pub struct CrustBenchConfig {
    pub dataset_path: PathBuf,
    pub filter: Option<String>,
    pub limit: Option<usize>,
    pub migration_config: MigrationConfig,
}

/// Result for a single project in CRUST-Bench.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectResult {
    pub name: String,
    pub c_loc: u32,
    pub rust_loc: u32,
    pub compilation_success: bool,
    pub tests_total: u32,
    pub tests_passed: u32,
    pub idiomatic_score_avg: f64,
    pub unsafe_count: u32,
    pub repair_iterations: u32,
    pub llm_calls: u32,
    pub total_ms: u64,
    pub error: Option<String>,
}

/// Aggregate report for all projects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrustBenchReport {
    pub total_projects: usize,
    pub compilation_rate: f64,
    pub test_pass_rate: f64,
    pub avg_idiomatic_score: f64,
    pub projects: Vec<ProjectResult>,
}

/// Discover project directories in the CRUST-Bench dataset.
pub fn discover_projects(dataset_path: &Path) -> Result<Vec<PathBuf>> {
    let mut projects = Vec::new();

    if !dataset_path.is_dir() {
        anyhow::bail!("CRUST-Bench dataset not found: {}", dataset_path.display());
    }

    for entry in std::fs::read_dir(dataset_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            // Check if directory contains C files
            let has_c = std::fs::read_dir(&path)?.filter_map(|e| e.ok()).any(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext == "c" || ext == "h")
            });
            if has_c {
                projects.push(path);
            }
        }
    }

    projects.sort();
    Ok(projects)
}

/// Migrate a single CRUST-Bench project.
async fn run_project(project_dir: &Path, config: &MigrationConfig) -> ProjectResult {
    let name = project_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    info!(project = %name, "evaluating CRUST-Bench project");

    let start = std::time::Instant::now();

    match noricum_core::orchestrator::migrate_directory(project_dir, config).await {
        Ok(project) => {
            let units = &project.units;
            let c_loc: u32 = units.iter().map(|u| u.metrics.c_lines).sum();
            let rust_loc: u32 = units.iter().map(|u| u.metrics.rust_lines).sum();
            let compilation_success = units
                .iter()
                .all(|u| u.state == noricum_ir::MigrationState::Validated);
            let tests_total = units.len() as u32;
            let tests_passed = units
                .iter()
                .filter(|u| u.state == noricum_ir::MigrationState::Validated)
                .count() as u32;
            let scores: Vec<f64> = units
                .iter()
                .filter_map(|u| u.idiomatic_score.map(|s| s as f64))
                .collect();
            let avg_score = if scores.is_empty() {
                0.0
            } else {
                scores.iter().sum::<f64>() / scores.len() as f64
            };
            let unsafe_count: u32 = units.iter().filter_map(|u| u.unsafe_count).sum();
            let repair_iterations: u32 = units.iter().map(|u| u.metrics.repair_iterations).sum();
            let llm_calls: u32 = units.iter().map(|u| u.metrics.llm_calls).sum();

            ProjectResult {
                name,
                c_loc,
                rust_loc,
                compilation_success,
                tests_total,
                tests_passed,
                idiomatic_score_avg: avg_score,
                unsafe_count,
                repair_iterations,
                llm_calls,
                total_ms: start.elapsed().as_millis() as u64,
                error: None,
            }
        }
        Err(e) => {
            warn!(project = %name, error = %e, "CRUST-Bench project failed");
            ProjectResult {
                name,
                c_loc: 0,
                rust_loc: 0,
                compilation_success: false,
                tests_total: 0,
                tests_passed: 0,
                idiomatic_score_avg: 0.0,
                unsafe_count: 0,
                repair_iterations: 0,
                llm_calls: 0,
                total_ms: start.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
            }
        }
    }
}

/// Run CRUST-Bench evaluation across all discovered projects.
pub async fn run_crust_bench(config: &CrustBenchConfig) -> Result<CrustBenchReport> {
    let mut projects = discover_projects(&config.dataset_path)?;

    // Apply filter
    if let Some(ref filter) = config.filter {
        projects.retain(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().contains(filter.as_str()))
                .unwrap_or(false)
        });
    }

    // Apply limit
    if let Some(limit) = config.limit {
        projects.truncate(limit);
    }

    info!(
        total = projects.len(),
        filter = ?config.filter,
        limit = ?config.limit,
        "starting CRUST-Bench evaluation"
    );

    let mut results = Vec::new();
    for project_dir in &projects {
        let result = run_project(project_dir, &config.migration_config).await;
        results.push(result);
    }

    let total = results.len();
    let compiled = results.iter().filter(|r| r.compilation_success).count();
    let passed = results.iter().filter(|r| r.tests_passed > 0).count();
    let scores: Vec<f64> = results.iter().map(|r| r.idiomatic_score_avg).collect();
    let avg_score = if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    };

    Ok(CrustBenchReport {
        total_projects: total,
        compilation_rate: if total > 0 {
            compiled as f64 / total as f64
        } else {
            0.0
        },
        test_pass_rate: if total > 0 {
            passed as f64 / total as f64
        } else {
            0.0
        },
        avg_idiomatic_score: avg_score,
        projects: results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_projects_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = discover_projects(tmp.path()).unwrap();
        assert!(projects.is_empty());
    }

    #[test]
    fn test_discover_projects_with_c_files() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("myproject");
        std::fs::create_dir(&proj).unwrap();
        std::fs::write(proj.join("main.c"), "int main() { return 0; }").unwrap();

        let projects = discover_projects(tmp.path()).unwrap();
        assert_eq!(projects.len(), 1);
    }

    #[test]
    fn test_report_serialization() {
        let report = CrustBenchReport {
            total_projects: 1,
            compilation_rate: 1.0,
            test_pass_rate: 1.0,
            avg_idiomatic_score: 90.0,
            projects: vec![ProjectResult {
                name: "test".to_string(),
                c_loc: 100,
                rust_loc: 80,
                compilation_success: true,
                tests_total: 5,
                tests_passed: 5,
                idiomatic_score_avg: 90.0,
                unsafe_count: 0,
                repair_iterations: 0,
                llm_calls: 2,
                total_ms: 1000,
                error: None,
            }],
        };
        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("test"));
        let deser: CrustBenchReport = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.total_projects, 1);
    }
}
