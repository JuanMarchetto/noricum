/// C2Rust integration: invokes c2rust transpile as a subprocess.
///
/// C2Rust is "step zero" — it handles mechanical translation from C to unsafe Rust.
/// Noricum's LLM agents then refine the output to safe, idiomatic Rust.
use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::{debug, info, warn};

use crate::ToolError;

/// Result of a C2Rust transpilation.
#[derive(Debug)]
pub struct C2RustOutput {
    /// Path to the generated Rust file
    pub rust_file: PathBuf,
    /// The generated Rust source code
    pub rust_source: String,
    /// Any warnings from C2Rust
    pub warnings: Vec<String>,
}

/// Check if c2rust is available on the system.
pub fn check_c2rust_available() -> Result<String, ToolError> {
    let output = Command::new("c2rust")
        .arg("--version")
        .output()
        .map_err(|_| ToolError::CommandNotFound("c2rust".to_string()))?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        info!(version = %version, "c2rust found");
        Ok(version)
    } else {
        Err(ToolError::CommandNotFound(
            "c2rust found but returned error".to_string(),
        ))
    }
}

/// Generate a compile_commands.json for a single C file.
///
/// C2Rust requires a compilation database. For single files, we generate a minimal one.
pub fn generate_compile_commands(c_file: &Path, output_dir: &Path) -> Result<PathBuf, ToolError> {
    let c_file = c_file.canonicalize().map_err(ToolError::Io)?;
    let compile_commands_path = output_dir.join("compile_commands.json");

    let compile_commands = serde_json::json!([{
        "directory": output_dir.to_string_lossy(),
        "command": format!("cc -std=gnu11 -c {}", c_file.display()),
        "file": c_file.to_string_lossy()
    }]);

    std::fs::write(
        &compile_commands_path,
        serde_json::to_string_pretty(&compile_commands).map_err(|e| {
            ToolError::C2RustFailed(format!("failed to serialize compile_commands.json: {e}"))
        })?,
    )?;

    debug!(path = %compile_commands_path.display(), "generated compile_commands.json");
    Ok(compile_commands_path)
}

/// Transpile a C file to Rust using C2Rust.
///
/// This creates a temporary directory, generates compile_commands.json,
/// runs c2rust transpile, and returns the generated Rust source.
pub fn transpile(c_file: &Path) -> Result<C2RustOutput, ToolError> {
    let c_file = c_file
        .canonicalize()
        .map_err(|e| ToolError::C2RustFailed(format!("source file not found: {e}")))?;

    info!(file = %c_file.display(), "transpiling C file with c2rust");

    let tmp_dir = tempfile::tempdir()?;
    let compile_commands = generate_compile_commands(&c_file, tmp_dir.path())?;

    let output = Command::new("c2rust")
        .arg("transpile")
        .arg(&compile_commands)
        .arg("--output-dir")
        .arg(tmp_dir.path())
        .output()
        .map_err(|_| ToolError::CommandNotFound("c2rust".to_string()))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let warnings: Vec<String> = stderr
        .lines()
        .filter(|l| l.contains("warning"))
        .map(String::from)
        .collect();

    if !warnings.is_empty() {
        warn!(count = warnings.len(), "c2rust produced warnings");
    }

    if !output.status.success() {
        return Err(ToolError::C2RustFailed(format!(
            "c2rust exited with {}: {}",
            output.status, stderr
        )));
    }

    // C2Rust outputs a .rs file with the same stem as the input
    let stem = c_file
        .file_stem()
        .ok_or_else(|| ToolError::C2RustFailed("source file has no stem".to_string()))?
        .to_string_lossy();
    let rust_file = find_rust_output(tmp_dir.path(), &stem)?;
    let rust_source = std::fs::read_to_string(&rust_file)?;

    info!(
        lines = rust_source.lines().count(),
        "c2rust transpilation complete"
    );

    Ok(C2RustOutput {
        rust_file,
        rust_source,
        warnings,
    })
}

/// Find the generated .rs file in the output directory.
fn find_rust_output(dir: &Path, stem: &str) -> Result<PathBuf, ToolError> {
    // C2Rust may place the output at various locations; search recursively
    for entry in walkdir(dir)? {
        if let Some(name) = entry.file_name()
            && name == format!("{stem}.rs").as_str()
        {
            return Ok(entry);
        }
    }
    Err(ToolError::C2RustFailed(format!(
        "c2rust did not produce {stem}.rs in output directory"
    )))
}

/// Simple recursive directory walk (avoids adding walkdir dependency for now).
fn walkdir(dir: &Path) -> Result<Vec<PathBuf>, ToolError> {
    let mut results = Vec::new();
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                results.extend(walkdir(&path)?);
            } else {
                results.push(path);
            }
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_compile_commands() {
        let tmp = tempfile::tempdir().unwrap();
        let c_file = tmp.path().join("test.c");
        std::fs::write(&c_file, "int main() { return 0; }").unwrap();

        let result = generate_compile_commands(&c_file, tmp.path());
        assert!(result.is_ok());

        let cc_path = result.unwrap();
        assert!(cc_path.exists());

        let content = std::fs::read_to_string(cc_path).unwrap();
        assert!(content.contains("test.c"));
        assert!(content.contains("cc -std=gnu11"));
    }

    #[test]
    fn test_check_c2rust_not_installed() {
        // This test documents behavior when c2rust is not installed
        // It may pass or fail depending on the environment
        let result = check_c2rust_available();
        if let Err(err) = result {
            match err {
                ToolError::CommandNotFound(_) => {} // expected
                other => panic!("unexpected error: {other}"),
            }
        }
    }
}
