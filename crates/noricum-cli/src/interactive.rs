/// Interactive human-in-the-loop review mode.
///
/// Provides terminal-based side-by-side comparison of C and Rust code,
/// allowing the user to approve, retranslate, edit, skip, or abort migrations.
///
use std::io::Write;

/// Action the user can take during interactive review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewAction {
    /// Accept the current translation.
    Approve,
    /// Request a new translation from the LLM.
    Retranslate,
    /// Open the translation in $EDITOR for manual edits.
    Edit,
    /// Skip this function (keep current state).
    Skip,
    /// Abort the entire migration.
    Abort,
}

/// Point in the pipeline where review is requested.
#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum ReviewPoint {
    AfterTranslation {
        function_name: String,
        c_source: String,
        rust_output: String,
        idiomatic_score: u32,
        unsafe_count: u32,
    },
    AfterRepair {
        function_name: String,
        before: String,
        after: String,
        iteration: u32,
        errors: Vec<String>,
    },
    AfterValidation {
        function_name: String,
        rust_output: String,
        passed: bool,
        score: u32,
    },
}

/// Display a review prompt for a translation and get user action.
pub fn review_translation(
    name: &str,
    c_source: &str,
    rust_output: &str,
    score: u32,
    unsafe_count: u32,
) -> ReviewAction {
    println!("\n{}", "=".repeat(60));
    println!("Review: {name}  |  Score: {score}/100  |  Unsafe: {unsafe_count}");
    println!("{}", "=".repeat(60));

    // Show C source
    println!("\n--- C Source ---");
    for (i, line) in c_source.lines().enumerate() {
        println!("{:4} | {}", i + 1, line);
    }

    // Show Rust output
    println!("\n--- Rust Translation ---");
    for (i, line) in rust_output.lines().enumerate() {
        println!("{:4} | {}", i + 1, line);
    }

    // Show diff summary
    let c_lines = c_source.lines().count();
    let r_lines = rust_output.lines().count();
    println!(
        "\n  C: {} lines -> Rust: {} lines ({:+} lines)",
        c_lines,
        r_lines,
        r_lines as i64 - c_lines as i64
    );

    prompt_action()
}

/// Display a review prompt for a repair iteration.
pub fn review_repair(
    name: &str,
    before: &str,
    after: &str,
    iteration: u32,
    errors: &[String],
) -> ReviewAction {
    println!("\n{}", "=".repeat(60));
    println!("Repair Review: {name}  |  Iteration: {iteration}");
    println!("{}", "=".repeat(60));

    if !errors.is_empty() {
        println!("\n--- Errors ---");
        for e in errors {
            println!("  {e}");
        }
    }

    // Show diff between before and after
    println!("\n--- Changes ---");
    let diff = similar::TextDiff::from_lines(before, after);
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            similar::ChangeTag::Delete => "-",
            similar::ChangeTag::Insert => "+",
            similar::ChangeTag::Equal => " ",
        };
        print!("{sign}{change}");
    }

    prompt_action()
}

/// Display a review prompt after validation.
pub fn review_validation(name: &str, rust_output: &str, passed: bool, score: u32) -> ReviewAction {
    let status = if passed { "PASSED" } else { "FAILED" };
    println!("\n{}", "=".repeat(60));
    println!("Validation: {name}  |  {status}  |  Score: {score}/100");
    println!("{}", "=".repeat(60));

    println!("\n--- Final Rust Output ---");
    for (i, line) in rust_output.lines().enumerate() {
        println!("{:4} | {}", i + 1, line);
    }

    prompt_action()
}

fn prompt_action() -> ReviewAction {
    println!();
    println!("  [a]pprove  [r]etranslate  [e]dit  [s]kip  [q]uit");
    print!("  > ");
    let _ = std::io::stdout().flush();

    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return ReviewAction::Skip;
    }

    match input.trim().to_lowercase().as_str() {
        "a" | "approve" | "y" | "yes" => ReviewAction::Approve,
        "r" | "retranslate" => ReviewAction::Retranslate,
        "e" | "edit" => ReviewAction::Edit,
        "s" | "skip" | "n" | "no" => ReviewAction::Skip,
        "q" | "quit" | "abort" => ReviewAction::Abort,
        _ => {
            println!("  Unknown action, skipping.");
            ReviewAction::Skip
        }
    }
}

/// Generate a markdown review report.
pub fn generate_review_report(units: &[noricum_ir::FunctionUnit], project_name: &str) -> String {
    let mut md = String::new();
    md.push_str(&format!("# Migration Review Report: {project_name}\n\n"));
    md.push_str(&format!(
        "Generated: {}\n\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    ));

    let total = units.len();
    let validated = units
        .iter()
        .filter(|u| u.state == noricum_ir::MigrationState::Validated)
        .count();
    md.push_str("## Summary\n\n");
    md.push_str("| Metric | Value |\n|--------|-------|\n");
    md.push_str(&format!("| Total functions | {total} |\n"));
    md.push_str(&format!("| Validated | {validated} |\n"));
    md.push_str(&format!(
        "| Success rate | {:.1}% |\n\n",
        validated as f64 / total.max(1) as f64 * 100.0
    ));

    md.push_str("## Functions\n\n");
    for unit in units {
        md.push_str(&format!("### {}\n\n", unit.name));
        md.push_str(&format!("- **State:** {:?}\n", unit.state));
        if let Some(score) = unit.idiomatic_score {
            md.push_str(&format!("- **Score:** {score}/100\n"));
        }
        if let Some(unsafe_n) = unit.unsafe_count {
            md.push_str(&format!("- **Unsafe blocks:** {unsafe_n}\n"));
        }
        if let Some(ref rust) = unit.rust_output {
            md.push_str(&format!("\n```rust\n{rust}\n```\n\n"));
        }
    }

    md
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_action_variants() {
        assert_ne!(ReviewAction::Approve, ReviewAction::Skip);
        assert_eq!(ReviewAction::Abort, ReviewAction::Abort);
    }

    #[test]
    fn test_generate_review_report() {
        let units = vec![noricum_ir::FunctionUnit::new(
            "add".into(),
            "add.c".into(),
            "int add(int a, int b) { return a + b; }".into(),
        )];
        let report = generate_review_report(&units, "test_project");
        assert!(report.contains("# Migration Review Report"));
        assert!(report.contains("test_project"));
        assert!(report.contains("add"));
    }
}
