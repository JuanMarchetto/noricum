mod api;
mod commands;
mod crust_bench;
mod report;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand};
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
    Migrate(MigrateOpts),
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
        /// Ollama model name (forces Ollama provider instead of Anthropic)
        #[arg(long)]
        ollama_model: Option<String>,
    },
    /// Compare two baseline JSON files side-by-side
    Compare {
        /// Path to baseline A JSON file
        #[arg(long)]
        baseline_a: PathBuf,
        /// Label for baseline A
        #[arg(long, default_value = "A")]
        label_a: String,
        /// Path to baseline B JSON file
        #[arg(long)]
        baseline_b: PathBuf,
        /// Label for baseline B
        #[arg(long, default_value = "B")]
        label_b: String,
    },
    /// Review behavioral equivalence between C source and Rust migration
    Review {
        /// Path to the original C source file
        #[arg(long)]
        c_source: PathBuf,
        /// Path to the migrated Rust source file
        #[arg(long)]
        rust_source: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Also run diff test and include result in review context
        #[arg(long)]
        diff_test: bool,
    },
    /// Run CRUST-Bench evaluation (interface-aware mode)
    CrustBench {
        /// Path to the CRUST-Bench dataset directory (must contain CBench/ and RBench/)
        #[arg(long)]
        dataset: PathBuf,
        /// Filter projects by name substring
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
        /// Ollama model name (forces Ollama provider instead of Anthropic)
        #[arg(long)]
        ollama_model: Option<String>,
    },
}

/// Options for the `migrate` subcommand.
#[derive(Parser)]
struct MigrateOpts {
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
    /// Generate doc comments on migrated Rust functions
    #[arg(long)]
    docs: bool,
    /// Maximum total token budget (input + output) per run; exceeding aborts the migration
    /// (default: 500000, use 0 for unlimited)
    #[arg(long)]
    max_tokens: Option<u64>,
    /// Maximum number of LLM API calls per run (default: 20, use 0 for unlimited)
    #[arg(long)]
    max_llm_calls: Option<u32>,
    /// Ollama model name (default: qwen2.5-coder:32b)
    #[arg(long)]
    ollama_model: Option<String>,
    /// Skip C2Rust transpilation (translate directly from C source)
    #[arg(long)]
    skip_c2rust: bool,
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
        Commands::Migrate(opts) => {
            commands::migrate::cmd_migrate(commands::migrate::MigrateParams {
                path: opts.path,
                no_llm: opts.no_llm,
                output: opts.output,
                diff_test: opts.diff_test,
                json: opts.json,
                report: opts.report,
                fuzz: opts.fuzz,
                fuzz_iterations: opts.fuzz_iterations,
                audit_log: opts.audit_log,
                audit_level: opts.audit_level,
                docs: opts.docs,
                max_tokens: opts.max_tokens,
                max_llm_calls: opts.max_llm_calls,
                ollama_model: opts.ollama_model,
                skip_c2rust: opts.skip_c2rust,
            })
            .await
        }
        Commands::Analyze { path } => commands::analyze::cmd_analyze(&path),
        Commands::Doctor => commands::doctor::cmd_doctor(),
        Commands::Serve { host, port } => cmd_serve(&host, port).await,
        Commands::Bench {
            fixtures,
            json,
            save_baseline,
            compare_baseline,
            ollama_model,
        } => {
            commands::bench::cmd_bench(
                &fixtures,
                json,
                save_baseline.as_deref(),
                compare_baseline.as_deref(),
                ollama_model,
            )
            .await
        }
        Commands::Compare {
            baseline_a,
            baseline_b,
            label_a,
            label_b,
        } => commands::compare::cmd_compare(&baseline_a, &label_a, &baseline_b, &label_b),
        Commands::Review {
            c_source,
            rust_source,
            json,
            diff_test,
        } => {
            commands::review::cmd_review(commands::review::ReviewParams {
                c_path: c_source,
                rust_path: rust_source,
                json,
                diff_test,
            })
            .await
        }
        Commands::CrustBench {
            dataset,
            filter,
            limit,
            json,
            output,
            ollama_model,
        } => {
            cmd_crust_bench(
                &dataset,
                filter.as_deref(),
                limit,
                json,
                output.as_deref(),
                ollama_model,
            )
            .await
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        for cause in e.chain().skip(1) {
            eprintln!("  caused by: {cause}");
        }
        eprintln!();
        eprintln!(
            "hint: try `noricum doctor` to check tool availability, or `--no-llm` to skip LLM agents"
        );
        std::process::exit(1);
    }
}

/// Wait for a shutdown signal (Ctrl+C or SIGTERM).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl+c");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    info!("shutdown signal received, draining connections...");
}

async fn cmd_serve(host: &str, port: u16) -> Result<()> {
    let mut config = noricum_core::MigrationConfig::default();
    // Enable audit logging by default for the REST API server
    if config.audit_log.is_none() {
        config.audit_log = Some("noricum-audit.jsonl".into());
        config.audit_level = noricum_core::audit::AuditLevel::Summary;
        info!("audit logging enabled by default (noricum-audit.jsonl)");
    }
    let api_key = std::env::var("NORICUM_API_KEY").ok();
    let is_public = host != "127.0.0.1" && host != "localhost" && host != "::1";
    if is_public && api_key.is_none() {
        eprintln!("WARNING: serving on non-localhost ({host}) without NORICUM_API_KEY.");
        eprintln!("  All mutating endpoints will require authentication.");
        eprintln!("  Set NORICUM_API_KEY env var to enable access.");
    }
    let rate_limit_rpm: usize = std::env::var("NORICUM_RATE_LIMIT_RPM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(api::DEFAULT_RATE_LIMIT_RPM);
    let state = Arc::new(api::AppState {
        config,
        api_key,
        is_public,
        rate_limiter: api::IpRateLimiter::new(rate_limit_rpm),
    });
    let app = api::build_router(state);
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Noricum API listening on http://{addr}");
    println!("Noricum API listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn cmd_crust_bench(
    dataset: &std::path::Path,
    filter: Option<&str>,
    limit: Option<usize>,
    json: bool,
    output: Option<&std::path::Path>,
    ollama_model: Option<String>,
) -> Result<()> {
    let mut migration_config = noricum_core::MigrationConfig::default();
    if ollama_model.is_some() {
        migration_config.anthropic_api_key = None;
        migration_config.ollama_model = ollama_model;
    }

    let config = crust_bench::CrustBenchConfig {
        dataset_path: dataset.to_path_buf(),
        filter: filter.map(|s| s.to_string()),
        limit,
        migration_config,
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
        println!("CRUST-Bench Report (interface-aware)");
        println!("====================================");
        println!("  Total projects:     {}", report.total_projects);
        println!(
            "  Compilation rate:   {:.1}%",
            report.compilation_rate * 100.0
        );
        println!(
            "  Test pass rate:     {:.1}%",
            report.test_pass_rate * 100.0
        );
        println!("  Avg idiomatic:      {:.1}", report.avg_idiomatic_score);
        println!("  Total LLM calls:    {}", report.total_llm_calls);
        println!("  Total repairs:      {}", report.total_repair_iterations);
        println!();
        for p in &report.projects {
            let status = if p.tests_passed {
                "PASS    "
            } else if p.compilation_success {
                "BUILD_OK"
            } else {
                "FAIL    "
            };
            println!(
                "  {:<30} {} score={:>3.0} unsafe={} repairs={} calls={} {}ms",
                p.name,
                status,
                p.idiomatic_score_avg,
                p.unsafe_count,
                p.repair_iterations,
                p.llm_calls,
                p.total_ms,
            );
        }
    }

    Ok(())
}
