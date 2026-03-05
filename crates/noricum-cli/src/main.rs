mod api;
mod crust_bench;
mod interactive;
mod report;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use noricum_core::MigrationConfig;
use noricum_core::audit::AuditLevel;
use noricum_ir::FunctionUnit;
use tracing::info;

#[derive(Parser)]
#[command(name = "noricum")]
#[command(about = "Autonomous C/C++ to Rust migration agent")]
#[command(version)]
struct Cli {
    /// Increase logging verbosity
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Migrate a C file or directory to Rust
    Migrate {
        /// Path to a C file or directory containing C files
        path: PathBuf,
        /// Force synchronous mode (no LLM agents)
        #[arg(long)]
        no_llm: bool,
        /// Output directory for generated Rust files (default: ./output)
        #[arg(short, long, default_value = "output")]
        output: PathBuf,
        /// Run differential tests after migration
        #[arg(long)]
        diff_test: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Generate HTML report at this path
        #[arg(long)]
        report: Option<PathBuf>,
        /// Run fuzz testing for behavioral comparison
        #[arg(long)]
        fuzz: bool,
        /// Number of fuzz test iterations (default: 100)
        #[arg(long, default_value_t = 100)]
        fuzz_iterations: u32,
        /// Write audit trail to this file (JSON-lines format)
        #[arg(long)]
        audit_log: Option<PathBuf>,
        /// Audit detail level: summary, detailed, or full
        #[arg(long, default_value = "summary")]
        audit_level: String,
        /// Incremental migration mode (track per-function state)
        #[arg(long)]
        incremental: bool,
        /// Migrate only these functions (comma-separated names)
        #[arg(long, value_delimiter = ',')]
        functions: Option<Vec<String>>,
        /// Directory for incremental state files
        #[arg(long, default_value = ".noricum-state")]
        state_dir: PathBuf,
        /// Interactive review mode
        #[arg(long)]
        interactive: bool,
    },
    /// Analyze a C file and report difficulty classification
    Analyze {
        /// Path to a C file
        path: PathBuf,
    },
    /// Check if required tools are available
    Doctor,
    /// Start REST API server
    Serve {
        /// Host address to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port to listen on
        #[arg(short, long, default_value_t = 3000)]
        port: u16,
    },
    /// Run benchmark on all fixtures and produce a report
    Bench {
        /// Fixtures directory (default: tests/fixtures/simple)
        #[arg(short, long, default_value = "tests/fixtures/simple")]
        fixtures: PathBuf,
        /// Output report as JSON
        #[arg(long)]
        json: bool,
        /// Save results as baseline
        #[arg(long)]
        save_baseline: Option<PathBuf>,
        /// Compare against a baseline and report regressions
        #[arg(long)]
        compare_baseline: Option<PathBuf>,
    },
    /// Run CRUST-Bench evaluation
    CrustBench {
        /// Path to the CRUST-Bench dataset directory
        #[arg(long)]
        dataset: PathBuf,
        /// Filter projects by name prefix
        #[arg(long)]
        filter: Option<String>,
        /// Limit number of projects to evaluate
        #[arg(long)]
        limit: Option<usize>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Write report to this path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn setup_tracing(verbosity: u8) {
    let filter = match verbosity {
        0 => "noricum=info",
        1 => "noricum=debug",
        _ => "noricum=trace",
    };

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()),
        )
        .init();
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    setup_tracing(cli.verbose);

    let result = match cli.command {
        Commands::Migrate {
            path,
            no_llm,
            output,
            diff_test,
            json,
            report,
            fuzz,
            fuzz_iterations,
            audit_log,
            audit_level,
            incremental: _,
            functions: _,
            state_dir: _,
            interactive: _,
        } => {
            let level: AuditLevel = audit_level.parse().unwrap_or(AuditLevel::Summary);
            cmd_migrate(
                &path,
                no_llm,
                &output,
                diff_test,
                json,
                report.as_deref(),
                fuzz,
                fuzz_iterations,
                audit_log.as_deref(),
                level,
            )
            .await
        }
        Commands::Analyze { path } => cmd_analyze(&path),
        Commands::Doctor => cmd_doctor(),
        Commands::Serve { host, port } => cmd_serve(&host, port).await,
        Commands::Bench {
            fixtures,
            json,
            save_baseline,
            compare_baseline,
        } => {
            cmd_bench(
                &fixtures,
                json,
                save_baseline.as_deref(),
                compare_baseline.as_deref(),
            )
            .await
        }
        Commands::CrustBench {
            dataset,
            filter,
            limit,
            json,
            output,
        } => cmd_crust_bench(&dataset, filter.as_deref(), limit, json, output.as_deref()).await,
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        for cause in e.chain().skip(1) {
            eprintln!("  caused by: {cause}");
        }
        std::process::exit(1);
    }
}

#[allow(clippy::too_many_arguments)]
async fn cmd_migrate(
    path: &Path,
    no_llm: bool,
    output_dir: &Path,
    run_diff: bool,
    json: bool,
    report_path: Option<&Path>,
    fuzz: bool,
    fuzz_iterations: u32,
    audit_log: Option<&Path>,
    audit_level: AuditLevel,
) -> Result<()> {
    let path = path
        .canonicalize()
        .with_context(|| format!("path not found: {}", path.display()))?;

    if no_llm {
        return cmd_migrate_sync(
            &path,
            output_dir,
            run_diff,
            json,
            report_path,
            fuzz,
            fuzz_iterations,
        );
    }

    let mut config = MigrationConfig::default();
    config.fuzz_test = fuzz;
    config.fuzz_iterations = fuzz_iterations;
    config.audit_log = audit_log.map(|p| p.to_path_buf());
    config.audit_level = audit_level;

    if path.is_file() {
        info!(file = %path.display(), "migrating single file");
        let unit = noricum_core::orchestrator::migrate_file(&path, &config)
            .await
            .with_context(|| format!("migration failed for {}", path.display()))?;

        print_unit_result(&unit, output_dir, run_diff, json, fuzz, fuzz_iterations)?;

        if let Some(report) = report_path {
            write_html_report_single(&unit, report)?;
        }
    } else if path.is_dir() {
        info!(dir = %path.display(), "migrating directory");
        let project = noricum_core::orchestrator::migrate_directory(&path, &config)
            .await
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

        // Write output files regardless of json mode
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

fn write_unit_output(unit: &FunctionUnit, output_dir: &Path) -> Result<()> {
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

fn cmd_analyze(path: &Path) -> Result<()> {
    let source =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;

    let difficulty = noricum_core::router::classify_difficulty(&source);
    let lines = source.lines().count();

    println!("Analysis of: {}", path.display());
    println!("  Lines: {lines}");
    println!("  Difficulty: {difficulty:?}");

    Ok(())
}

async fn cmd_bench(
    fixtures_dir: &Path,
    json: bool,
    save_baseline: Option<&Path>,
    compare_baseline: Option<&Path>,
) -> Result<()> {
    let fixtures_dir = fixtures_dir
        .canonicalize()
        .with_context(|| format!("fixtures dir not found: {}", fixtures_dir.display()))?;

    let config = MigrationConfig::default();
    let use_llm = config.anthropic_api_key.is_some();

    let mut c_files: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&fixtures_dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "c") {
            c_files.push(path);
        }
    }
    c_files.sort();

    if c_files.is_empty() {
        anyhow::bail!("no .c files found in {}", fixtures_dir.display());
    }

    if !json {
        println!("Noricum Benchmark Report");
        println!("========================");
        println!("Fixtures: {}", fixtures_dir.display());
        println!(
            "Mode: {}",
            if use_llm {
                "LLM (async)"
            } else {
                "sync (no LLM)"
            }
        );
        println!("Files: {}", c_files.len());
        println!();
    }

    let bench_start = std::time::Instant::now();
    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut total_validated = 0u32;
    let mut total_failed = 0u32;
    let mut total_llm_calls = 0u32;
    let mut total_repair_iters = 0u32;

    for c_file in &c_files {
        let name = c_file.file_stem().unwrap().to_string_lossy().to_string();

        let unit = if use_llm {
            noricum_core::orchestrator::migrate_file(c_file, &config).await?
        } else {
            noricum_core::orchestrator::migrate_file_sync(c_file)?
        };

        let passed = unit.state == noricum_ir::MigrationState::Validated;
        if passed {
            total_validated += 1;
        } else {
            total_failed += 1;
        }
        total_llm_calls += unit.metrics.llm_calls;
        total_repair_iters += unit.metrics.repair_iterations;

        if !json {
            let status = if passed { "PASS" } else { "FAIL" };
            let score = unit.idiomatic_score.unwrap_or(0);
            let unsafe_n = unit.unsafe_count.unwrap_or(0);
            let diff = match unit.metrics.diff_test_passed {
                Some(true) => "PASS",
                Some(false) => "FAIL",
                None => "N/A",
            };
            println!(
                "  {name:<20} {status:<5} score={score:>3} unsafe={unsafe_n} diff={diff:<4} \
                 repairs={:<1} calls={:<1} time={:>5}ms  c={:>3}L rust={:>3}L",
                unit.metrics.repair_iterations,
                unit.metrics.llm_calls,
                unit.metrics.total_ms,
                unit.metrics.c_lines,
                unit.metrics.rust_lines,
            );
        }

        results.push(serde_json::json!({
            "name": unit.name,
            "state": format!("{:?}", unit.state),
            "difficulty": format!("{:?}", unit.difficulty),
            "idiomatic_score": unit.idiomatic_score,
            "unsafe_count": unit.unsafe_count,
            "diff_test_passed": unit.metrics.diff_test_passed,
            "repair_iterations": unit.metrics.repair_iterations,
            "llm_calls": unit.metrics.llm_calls,
            "total_ms": unit.metrics.total_ms,
            "analysis_ms": unit.metrics.analysis_ms,
            "translation_ms": unit.metrics.translation_ms,
            "repair_ms": unit.metrics.repair_ms,
            "test_gen_ms": unit.metrics.test_gen_ms,
            "c_lines": unit.metrics.c_lines,
            "rust_lines": unit.metrics.rust_lines,
        }));
    }

    let total_time = bench_start.elapsed();
    let total_files = c_files.len() as u32;
    let success_rate = if total_files > 0 {
        total_validated as f64 / total_files as f64 * 100.0
    } else {
        0.0
    };

    if json {
        let report = serde_json::json!({
            "summary": {
                "total_files": total_files,
                "validated": total_validated,
                "failed": total_failed,
                "success_rate_pct": success_rate,
                "total_llm_calls": total_llm_calls,
                "total_repair_iterations": total_repair_iters,
                "total_time_ms": total_time.as_millis() as u64,
                "mode": if use_llm { "llm" } else { "sync" },
            },
            "results": results,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!();
        println!("Summary");
        println!("-------");
        println!("  Total:        {total_files}");
        println!("  Validated:    {total_validated}");
        println!("  Failed:       {total_failed}");
        println!("  Success rate: {success_rate:.1}%");
        println!("  LLM calls:    {total_llm_calls}");
        println!("  Repair iters: {total_repair_iters}");
        println!("  Total time:   {:.1}s", total_time.as_secs_f64());
    }

    // Save baseline if requested
    let report_value = serde_json::json!({
        "summary": {
            "total_files": total_files,
            "validated": total_validated,
            "failed": total_failed,
            "success_rate_pct": success_rate,
            "total_llm_calls": total_llm_calls,
            "total_repair_iterations": total_repair_iters,
            "total_time_ms": total_time.as_millis() as u64,
        },
        "results": results,
    });

    if let Some(baseline_path) = save_baseline {
        std::fs::write(baseline_path, serde_json::to_string_pretty(&report_value)?)?;
        println!("Baseline saved to {}", baseline_path.display());
    }

    // Compare against baseline if requested
    if let Some(baseline_path) = compare_baseline {
        let baseline_str = std::fs::read_to_string(baseline_path)
            .with_context(|| format!("cannot read baseline: {}", baseline_path.display()))?;
        let baseline: serde_json::Value = serde_json::from_str(&baseline_str)?;

        let base_rate = baseline["summary"]["success_rate_pct"]
            .as_f64()
            .unwrap_or(0.0);
        let current_rate = success_rate;

        println!("\nBaseline Comparison");
        println!("-------------------");
        println!("  Baseline success rate: {base_rate:.1}%");
        println!("  Current success rate:  {current_rate:.1}%");

        if current_rate < base_rate {
            println!(
                "  REGRESSION: success rate dropped by {:.1}%",
                base_rate - current_rate
            );
        } else if current_rate > base_rate {
            println!(
                "  IMPROVEMENT: success rate increased by {:.1}%",
                current_rate - base_rate
            );
        } else {
            println!("  No change in success rate.");
        }

        // Check individual regressions
        if let Some(base_results) = baseline["results"].as_array() {
            let mut regressions = Vec::new();
            for base_r in base_results {
                let name = base_r["name"].as_str().unwrap_or("");
                let base_state = base_r["state"].as_str().unwrap_or("");
                if base_state == "Validated" {
                    // Check if still validated
                    if let Some(current) = results.iter().find(|r| r["name"].as_str() == Some(name))
                    {
                        let cur_state = current["state"].as_str().unwrap_or("");
                        if cur_state != "Validated" {
                            regressions.push(format!("{name}: {base_state} -> {cur_state}"));
                        }
                    }
                }
            }
            if !regressions.is_empty() {
                println!("  Individual regressions:");
                for r in &regressions {
                    println!("    {r}");
                }
            }
        }
    }

    Ok(())
}

async fn cmd_serve(host: &str, port: u16) -> Result<()> {
    let config = MigrationConfig::default();
    let state = Arc::new(api::AppState { config });
    let app = api::build_router(state);
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Noricum API listening on http://{addr}");
    println!("Noricum API listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn cmd_crust_bench(
    dataset: &Path,
    filter: Option<&str>,
    limit: Option<usize>,
    json: bool,
    output: Option<&Path>,
) -> Result<()> {
    let config = crust_bench::CrustBenchConfig {
        dataset_path: dataset.to_path_buf(),
        filter: filter.map(|s| s.to_string()),
        limit,
        migration_config: MigrationConfig::default(),
    };
    let report = crust_bench::run_crust_bench(&config).await?;

    if json {
        let json_str = serde_json::to_string_pretty(&report)?;
        if let Some(out) = output {
            std::fs::write(out, &json_str)?;
            println!("Report written to {}", out.display());
        } else {
            println!("{json_str}");
        }
    } else {
        println!("CRUST-Bench Report");
        println!("==================");
        println!("  Total projects:    {}", report.total_projects);
        println!(
            "  Compilation rate:  {:.1}%",
            report.compilation_rate * 100.0
        );
        println!("  Test pass rate:    {:.1}%", report.test_pass_rate * 100.0);
        println!("  Avg idiomatic:     {:.1}", report.avg_idiomatic_score);
        println!();
        for p in &report.projects {
            let status = if p.compilation_success { "OK" } else { "FAIL" };
            println!(
                "  {:<30} {} score={:.0} tests={}/{}",
                p.name, status, p.idiomatic_score_avg, p.tests_passed, p.tests_total,
            );
        }
    }

    Ok(())
}

fn cmd_doctor() -> Result<()> {
    println!("Noricum v{}", env!("CARGO_PKG_VERSION"));
    println!();

    // Check C compiler
    print!("  C compiler (cc): ");
    match std::process::Command::new("cc").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let first_line = version.lines().next().unwrap_or("unknown");
            println!("OK ({first_line})");
        }
        _ => println!("NOT FOUND"),
    }

    // Check Rust compiler
    print!("  Rust compiler (rustc): ");
    match std::process::Command::new("rustc")
        .arg("--version")
        .output()
    {
        Ok(output) if output.status.success() => {
            println!("OK ({})", String::from_utf8_lossy(&output.stdout).trim());
        }
        _ => println!("NOT FOUND"),
    }

    // Check c2rust
    print!("  C2Rust (c2rust): ");
    match noricum_tools::c2rust::check_c2rust_available() {
        Ok(version) => println!("OK ({version})"),
        Err(_) => println!("NOT FOUND (install with: cargo install c2rust)"),
    }

    // Check clippy
    print!("  Clippy (clippy-driver): ");
    match std::process::Command::new("clippy-driver")
        .arg("--version")
        .output()
    {
        Ok(output) if output.status.success() => {
            println!("OK ({})", String::from_utf8_lossy(&output.stdout).trim());
        }
        _ => println!("NOT FOUND (install with: rustup component add clippy)"),
    }

    // Check C preprocessor
    print!("  C preprocessor: ");
    if noricum_tools::preprocessor::preprocessor_available() {
        println!("OK");
    } else {
        println!("NOT FOUND");
    }

    println!();
    println!(
        "  ANTHROPIC_API_KEY: {}",
        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            "set"
        } else {
            "not set"
        }
    );

    Ok(())
}
