use std::path::Path;

use anyhow::{Context, Result};
use noricum_core::MigrationConfig;
use noricum_core::audit::AuditLevel;
use noricum_ir::FunctionUnit;
use tracing::info;

use crate::report;

pub struct MigrateParams {
    pub path: std::path::PathBuf,
    pub no_llm: bool,
    pub output: std::path::PathBuf,
    pub diff_test: bool,
    pub json: bool,
    pub report: Option<std::path::PathBuf>,
    pub fuzz: bool,
    pub fuzz_iterations: u32,
    pub audit_log: Option<std::path::PathBuf>,
    pub audit_level: String,
    pub docs: bool,
    pub max_tokens: Option<u64>,
    pub max_llm_calls: Option<u32>,
    pub ollama_model: Option<String>,
    pub skip_c2rust: bool,
    pub provider: Option<String>,
    pub artifacts_dir: std::path::PathBuf,
    pub module_target_loc: Option<usize>,
}

pub async fn cmd_migrate(opts: MigrateParams) -> Result<()> {
    let path = opts
        .path
        .canonicalize()
        .with_context(|| format!("path not found: {}", opts.path.display()))?;

    let level: AuditLevel = opts.audit_level.parse().unwrap_or(AuditLevel::Summary);

    if opts.no_llm {
        return cmd_migrate_sync(
            &path,
            &opts.output,
            opts.diff_test,
            opts.json,
            opts.report.as_deref(),
            opts.fuzz,
            opts.fuzz_iterations,
        );
    }

    let config = MigrationConfig {
        // Force Ollama when --ollama-model is explicitly provided
        anthropic_api_key: if opts.ollama_model.is_some() {
            None
        } else {
            MigrationConfig::default().anthropic_api_key
        },
        primary_provider: opts.provider,
        fuzz_test: opts.fuzz,
        fuzz_iterations: opts.fuzz_iterations,
        audit_log: opts.audit_log.map(|p| p.to_path_buf()),
        audit_level: level,
        generate_docs: opts.docs,
        ollama_model: opts.ollama_model,
        skip_c2rust: opts.skip_c2rust,
        artifacts_dir: opts.artifacts_dir,
        module_target_loc: opts.module_target_loc,
        max_tokens_budget: match opts.max_tokens {
            Some(0) => None,
            Some(n) => Some(n),
            None => MigrationConfig::default().max_tokens_budget,
        },
        max_llm_calls: match opts.max_llm_calls {
            Some(0) => None,
            Some(n) => Some(n),
            None => MigrationConfig::default().max_llm_calls,
        },
        ..MigrationConfig::default()
    };

    if path.is_file() {
        info!(file = %path.display(), "migrating single file");
        let mut unit = noricum_core::orchestrator::migrate_file(&path, &config)
            .await
            .with_context(|| format!("migration failed for {}", path.display()))?;

        if opts.docs
            && let Some(ref rust) = unit.rust_output
        {
            let documented =
                noricum_tools::doc_gen::add_docs_to_rust(rust, &unit.c_source, &unit.name);
            unit.rust_output = Some(documented);
        }

        print_unit_result(
            &unit,
            &opts.output,
            opts.diff_test,
            opts.json,
            opts.fuzz,
            opts.fuzz_iterations,
        )?;

        if let Some(report) = opts.report.as_deref() {
            write_html_report_single(&unit, report)?;
        }
    } else if path.is_dir() {
        info!(dir = %path.display(), "migrating directory");
        let mut project = noricum_core::orchestrator::migrate_directory(&path, &config)
            .await
            .with_context(|| format!("migration failed for {}", path.display()))?;

        if opts.docs {
            for unit in &mut project.units {
                if let Some(ref rust) = unit.rust_output {
                    let documented =
                        noricum_tools::doc_gen::add_docs_to_rust(rust, &unit.c_source, &unit.name);
                    unit.rust_output = Some(documented);
                }
            }
        }

        if opts.json {
            println!("{}", serde_json::to_string_pretty(&project)?);
        } else {
            let summary = project.progress_summary();
            println!("Migration complete: {}", project.name);
            println!("  Total functions: {}", summary.total);
            println!("  Validated: {}", summary.validated);
            println!("  Failed (fallback unsafe): {}", summary.failed);
            println!("  In progress: {}", summary.in_progress);

            for unit in &project.units {
                println!();
                print_unit_result(
                    unit,
                    &opts.output,
                    opts.diff_test,
                    opts.json,
                    opts.fuzz,
                    opts.fuzz_iterations,
                )?;
            }
        }

        // Write output files regardless of json mode
        for unit in &project.units {
            write_unit_output(unit, &opts.output)?;
        }

        if let Some(report) = opts.report.as_deref() {
            write_html_report_project(&project.units, &project.name, report)?;
        }
    } else {
        anyhow::bail!("path is neither a file nor directory: {}", path.display());
    }

    Ok(())
}

fn cmd_migrate_sync(
    path: &Path,
    output_dir: &Path,
    run_diff: bool,
    json: bool,
    report_path: Option<&Path>,
    fuzz: bool,
    fuzz_iterations: u32,
) -> Result<()> {
    if path.is_file() {
        info!(file = %path.display(), "migrating single file (sync, no LLM)");
        let unit = noricum_core::orchestrator::migrate_file_sync(path)
            .with_context(|| format!("migration failed for {}", path.display()))?;

        print_unit_result(&unit, output_dir, run_diff, json, fuzz, fuzz_iterations)?;

        if let Some(report) = report_path {
            write_html_report_single(&unit, report)?;
        }
    } else if path.is_dir() {
        info!(dir = %path.display(), "migrating directory (sync, no LLM)");
        let project = noricum_core::orchestrator::migrate_directory_sync(path)
            .with_context(|| format!("migration failed for {}", path.display()))?;

        if json {
            println!("{}", serde_json::to_string_pretty(&project)?);
        } else {
            let summary = project.progress_summary();
            println!("Migration complete: {}", project.name);
            println!("  Total functions: {}", summary.total);
            println!("  Validated: {}", summary.validated);
            println!("  Failed (fallback unsafe): {}", summary.failed);
            println!("  In progress: {}", summary.in_progress);

            for unit in &project.units {
                println!();
                print_unit_result(unit, output_dir, run_diff, json, fuzz, fuzz_iterations)?;
            }
        }

        for unit in &project.units {
            write_unit_output(unit, output_dir)?;
        }

        if let Some(report) = report_path {
            write_html_report_project(&project.units, &project.name, report)?;
        }
    } else {
        anyhow::bail!("path is neither a file nor directory: {}", path.display());
    }

    Ok(())
}

pub fn write_unit_output(unit: &FunctionUnit, output_dir: &Path) -> Result<()> {
    if let Some(ref rust_output) = unit.rust_output {
        std::fs::create_dir_all(output_dir)?;
        let output_file = output_dir.join(format!("{}.rs", unit.name));
        std::fs::write(&output_file, rust_output)?;
    }
    Ok(())
}

fn print_unit_result(
    unit: &FunctionUnit,
    output_dir: &Path,
    run_diff: bool,
    json: bool,
    fuzz: bool,
    fuzz_iterations: u32,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(unit)?);
        write_unit_output(unit, output_dir)?;
        return Ok(());
    }

    println!("Migration result for: {}", unit.name);
    println!("  State: {:?}", unit.state);
    if let Some(difficulty) = unit.difficulty {
        println!("  Difficulty: {difficulty:?}");
    }
    if let Some(score) = unit.idiomatic_score {
        println!("  Idiomatic score: {score}/100");
    }
    if let Some(unsafe_count) = unit.unsafe_count {
        println!("  Unsafe blocks: {unsafe_count}");
    }

    write_unit_output(unit, output_dir)?;

    if let Some(ref rust_output) = unit.rust_output {
        let output_file = output_dir.join(format!("{}.rs", unit.name));
        println!("  Output: {}", output_file.display());

        println!("\n--- Generated Rust ---");
        println!("{rust_output}");

        if run_diff {
            println!("\n--- Differential Test ---");
            match noricum_tools::diff_test::run_diff_test(&unit.c_source, rust_output) {
                Ok(result) => {
                    if result.passed {
                        println!("  PASSED: C and Rust outputs match");
                    } else {
                        println!("  FAILED:");
                        if !result.c_compiled {
                            println!("    C compilation failed");
                        }
                        if !result.rust_compiled {
                            println!("    Rust compilation failed");
                        }
                        if !result.c_output.is_empty() {
                            println!("    C output:    {:?}", result.c_output);
                        }
                        if !result.rust_output.is_empty() {
                            println!("    Rust output: {:?}", result.rust_output);
                        }
                    }
                }
                Err(e) => println!("  Error running diff test: {e}"),
            }
        }

        if fuzz && fuzz_iterations > 0 {
            println!("\n--- Fuzz Test ---");
            let fuzz_config = noricum_tools::fuzz_test::FuzzConfig {
                iterations: fuzz_iterations,
                seed: Some(42),
                ..Default::default()
            };
            match noricum_tools::fuzz_test::run_fuzz_test(&unit.c_source, rust_output, &fuzz_config)
            {
                Ok(result) => {
                    if result.all_passed {
                        println!(
                            "  PASSED: {}/{} iterations match",
                            result.iterations_run, fuzz_iterations
                        );
                    } else {
                        println!(
                            "  FAILED: {} divergences in {} iterations",
                            result.failures, result.iterations_run
                        );
                        if let Some(ref div) = result.first_divergence {
                            println!("  First divergence:");
                            println!("    Input: {:?}", div.input.label);
                            println!("    C output:    {:?}", div.c_output);
                            println!("    Rust output: {:?}", div.rust_output);
                        }
                    }
                }
                Err(e) => println!("  Error running fuzz test: {e}"),
            }
        }
    } else {
        println!("\n  (no Rust output generated)");
        println!("  Run `noricum doctor` to check tool availability.");
    }

    if let Some(ref tests) = unit.generated_tests {
        println!("\n--- Generated Tests ---");
        println!("{tests}");
    }

    if unit.rust_output.is_some() {
        println!();
        println!("Note: This code was generated by an LLM and verified by differential testing");
        println!("against specific inputs. Review before deploying to production.");
    }

    Ok(())
}

fn write_html_report_single(unit: &FunctionUnit, report_path: &Path) -> Result<()> {
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let html = report::generate_html_report(unit);
    std::fs::write(report_path, &html)?;
    println!("  HTML report: {}", report_path.display());
    Ok(())
}

fn write_html_report_project(
    units: &[FunctionUnit],
    project_name: &str,
    report_path: &Path,
) -> Result<()> {
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let html = report::generate_project_html_report(units, project_name);
    std::fs::write(report_path, &html)?;
    println!("  HTML report: {}", report_path.display());
    Ok(())
}
