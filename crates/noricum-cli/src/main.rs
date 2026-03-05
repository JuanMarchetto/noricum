use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
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
    Migrate {
        /// Path to a C file or directory containing C files
        path: PathBuf,
    },
    /// Analyze a C file and report difficulty classification
    Analyze {
        /// Path to a C file
        path: PathBuf,
    },
    /// Check if required tools are available
    Doctor,
}

fn setup_tracing(verbosity: u8) {
    let filter = match verbosity {
        0 => "noricum=info",
        1 => "noricum=debug",
        _ => "noricum=trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()),
        )
        .init();
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    setup_tracing(cli.verbose);

    match cli.command {
        Commands::Migrate { path } => cmd_migrate(&path),
        Commands::Analyze { path } => cmd_analyze(&path),
        Commands::Doctor => cmd_doctor(),
    }
}

fn cmd_migrate(path: &Path) -> Result<()> {
    let path = path
        .canonicalize()
        .with_context(|| format!("path not found: {}", path.display()))?;

    if path.is_file() {
        info!(file = %path.display(), "migrating single file");
        let unit = noricum_core::orchestrator::migrate_file(&path)
            .with_context(|| format!("migration failed for {}", path.display()))?;

        println!("Migration result for: {}", unit.name);
        println!("  State: {:?}", unit.state);
        if let Some(score) = unit.idiomatic_score {
            println!("  Idiomatic score: {score}/100");
        }
        if let Some(unsafe_count) = unit.unsafe_count {
            println!("  Unsafe blocks: {unsafe_count}");
        }
        if let Some(ref rust_output) = unit.rust_output {
            println!("\n--- Generated Rust ---");
            println!("{rust_output}");
        } else {
            println!("\n  (no Rust output generated - c2rust may not be installed)");
            println!("  Run `noricum doctor` to check tool availability.");
        }
    } else if path.is_dir() {
        info!(dir = %path.display(), "migrating directory");
        let project = noricum_core::orchestrator::migrate_directory(&path)
            .with_context(|| format!("migration failed for {}", path.display()))?;

        let summary = project.progress_summary();
        println!("Migration complete: {}", project.name);
        println!("  Total functions: {}", summary.total);
        println!("  Validated: {}", summary.validated);
        println!("  Failed (fallback unsafe): {}", summary.failed);
        println!("  In progress: {}", summary.in_progress);
    } else {
        anyhow::bail!("path is neither a file nor directory: {}", path.display());
    }

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
