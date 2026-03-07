use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use noricum_core::MigrationConfig;

pub async fn cmd_bench(
    fixtures_dir: &Path,
    json: bool,
    save_baseline: Option<&Path>,
    compare_baseline: Option<&Path>,
    ollama_model: Option<String>,
) -> Result<()> {
    let fixtures_dir = fixtures_dir
        .canonicalize()
        .with_context(|| format!("fixtures dir not found: {}", fixtures_dir.display()))?;

    let config = MigrationConfig {
        anthropic_api_key: if ollama_model.is_some() {
            None
        } else {
            MigrationConfig::default().anthropic_api_key
        },
        ollama_model,
        ..MigrationConfig::default()
    };
    let use_llm = config.anthropic_api_key.is_some()
        || config.ollama_model.is_some();

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
    let mut total_cost_usd = 0.0f64;

    for c_file in &c_files {
        let name = c_file
            .file_stem()
            .unwrap_or_else(|| std::ffi::OsStr::new("unknown"))
            .to_string_lossy()
            .to_string();

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
        total_cost_usd += unit.metrics.estimated_cost_usd;

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
            "input_tokens": unit.metrics.input_tokens,
            "output_tokens": unit.metrics.output_tokens,
            "estimated_cost_usd": unit.metrics.estimated_cost_usd,
            "provider": unit.metrics.provider,
            "model": unit.metrics.model,
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
                "estimated_cost_usd": total_cost_usd,
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
        println!("  Est. cost:    ${total_cost_usd:.4}");
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
            "estimated_cost_usd": total_cost_usd,
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
                if base_state == "Validated"
                    && let Some(current) = results.iter().find(|r| r["name"].as_str() == Some(name))
                {
                    let cur_state = current["state"].as_str().unwrap_or("");
                    if cur_state != "Validated" {
                        regressions.push(format!("{name}: {base_state} -> {cur_state}"));
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
