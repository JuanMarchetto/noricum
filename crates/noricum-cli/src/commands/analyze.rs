use std::path::Path;

use anyhow::{Context, Result};

pub fn cmd_analyze(path: &Path) -> Result<()> {
    let source =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;

    let difficulty = noricum_core::router::classify_difficulty(&source);
    let lines = source.lines().count();

    println!("Analysis of: {}", path.display());
    println!("  Lines: {lines}");
    println!("  Difficulty: {difficulty:?}");

    Ok(())
}
