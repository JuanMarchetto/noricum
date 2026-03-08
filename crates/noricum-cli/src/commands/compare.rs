use std::path::Path;

use anyhow::{Context, Result};

/// Compare two baseline JSON files side-by-side.
pub fn cmd_compare(
    baseline_a: &Path,
    label_a: &str,
    baseline_b: &Path,
    label_b: &str,
) -> Result<()> {
    let a_str = std::fs::read_to_string(baseline_a)
        .with_context(|| format!("cannot read baseline A: {}", baseline_a.display()))?;
    let b_str = std::fs::read_to_string(baseline_b)
        .with_context(|| format!("cannot read baseline B: {}", baseline_b.display()))?;

    let a: serde_json::Value = serde_json::from_str(&a_str)?;
    let b: serde_json::Value = serde_json::from_str(&b_str)?;

    let a_results = a["results"]
        .as_array()
        .context("baseline A has no 'results' array")?;
    let b_results = b["results"]
        .as_array()
        .context("baseline B has no 'results' array")?;

    // Header
    let col_w = 22;
    println!("{:<20} | {:<col_w$} | {:<col_w$}", "File", label_a, label_b);
    println!("{:-<20}-+-{:-<col_w$}-+-{:-<col_w$}", "", "", "");

    // Collect all file names from both baselines
    let mut all_names: Vec<String> = Vec::new();
    for r in a_results {
        if let Some(name) = r["name"].as_str() {
            all_names.push(name.to_string());
        }
    }
    for r in b_results {
        if let Some(name) = r["name"].as_str()
            && !all_names.iter().any(|n| n == name)
        {
            all_names.push(name.to_string());
        }
    }
    all_names.sort();

    let mut a_pass = 0u32;
    let mut b_pass = 0u32;
    let mut a_total_score = 0u64;
    let mut b_total_score = 0u64;
    let mut a_total_ms = 0u64;
    let mut b_total_ms = 0u64;
    let mut a_count = 0u32;
    let mut b_count = 0u32;
    let mut a_cost = 0.0f64;
    let mut b_cost = 0.0f64;

    for name in &all_names {
        let a_entry = a_results.iter().find(|r| r["name"].as_str() == Some(name));
        let b_entry = b_results.iter().find(|r| r["name"].as_str() == Some(name));

        let a_col = format_entry(a_entry);
        let b_col = format_entry(b_entry);

        if let Some(e) = a_entry {
            a_count += 1;
            if e["state"].as_str() == Some("Validated") {
                a_pass += 1;
            }
            a_total_score += e["idiomatic_score"].as_u64().unwrap_or(0);
            a_total_ms += e["total_ms"].as_u64().unwrap_or(0);
            a_cost += e["estimated_cost_usd"].as_f64().unwrap_or(0.0);
        }
        if let Some(e) = b_entry {
            b_count += 1;
            if e["state"].as_str() == Some("Validated") {
                b_pass += 1;
            }
            b_total_score += e["idiomatic_score"].as_u64().unwrap_or(0);
            b_total_ms += e["total_ms"].as_u64().unwrap_or(0);
            b_cost += e["estimated_cost_usd"].as_f64().unwrap_or(0.0);
        }

        println!("{:<20} | {:<col_w$} | {:<col_w$}", name, a_col, b_col);
    }

    // Summary
    println!();
    println!("Summary");
    println!(
        "{:<20} | {:<col_w$} | {:<col_w$}",
        "Metric", label_a, label_b
    );
    println!("{:-<20}-+-{:-<col_w$}-+-{:-<col_w$}", "", "", "");

    let a_rate = if a_count > 0 {
        a_pass as f64 / a_count as f64 * 100.0
    } else {
        0.0
    };
    let b_rate = if b_count > 0 {
        b_pass as f64 / b_count as f64 * 100.0
    } else {
        0.0
    };
    let a_avg_score = if a_count > 0 {
        a_total_score as f64 / a_count as f64
    } else {
        0.0
    };
    let b_avg_score = if b_count > 0 {
        b_total_score as f64 / b_count as f64
    } else {
        0.0
    };
    let a_avg_ms = if a_count > 0 {
        a_total_ms as f64 / a_count as f64 / 1000.0
    } else {
        0.0
    };
    let b_avg_ms = if b_count > 0 {
        b_total_ms as f64 / b_count as f64 / 1000.0
    } else {
        0.0
    };

    println!(
        "{:<20} | {:<col_w$} | {:<col_w$}",
        "Pass Rate",
        format!("{:.1}%", a_rate),
        format!("{:.1}%", b_rate),
    );
    println!(
        "{:<20} | {:<col_w$} | {:<col_w$}",
        "Avg Score",
        format!("{:.1}", a_avg_score),
        format!("{:.1}", b_avg_score),
    );
    println!(
        "{:<20} | {:<col_w$} | {:<col_w$}",
        "Avg Time",
        format!("{:.1}s", a_avg_ms),
        format!("{:.1}s", b_avg_ms),
    );
    println!(
        "{:<20} | {:<col_w$} | {:<col_w$}",
        "Total Cost",
        format!("${:.4}", a_cost),
        format!("${:.4}", b_cost),
    );

    // Provider info from first result
    let a_provider = a_results
        .first()
        .and_then(|r| r["provider"].as_str())
        .unwrap_or("unknown");
    let b_provider = b_results
        .first()
        .and_then(|r| r["provider"].as_str())
        .unwrap_or("unknown");
    println!(
        "{:<20} | {:<col_w$} | {:<col_w$}",
        "Provider", a_provider, b_provider,
    );

    Ok(())
}

fn format_entry(entry: Option<&serde_json::Value>) -> String {
    let Some(e) = entry else {
        return "---".to_string();
    };

    let state = e["state"].as_str().unwrap_or("?");
    let status = if state == "Validated" { "PASS" } else { "FAIL" };
    let score = e["idiomatic_score"].as_u64().unwrap_or(0);
    let ms = e["total_ms"].as_u64().unwrap_or(0);
    let secs = ms as f64 / 1000.0;

    format!("{status} score={score:>3} {secs:>5.1}s")
}
