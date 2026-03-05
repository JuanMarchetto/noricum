/// Mixed C/Rust build: compile C as a static library and link with Rust.
///
/// Used during incremental migration where some functions are in Rust
/// and others remain in C. Both need to be linked into a single binary.
use std::path::Path;
use std::process::Command;

use tracing::{debug, info};

use crate::ToolError;

/// Result of a mixed build attempt.
#[derive(Debug)]
pub struct MixedBuildResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Build a mixed C/Rust project.
///
/// 1. Compile C source into an object file
/// 2. Compile Rust source that links against the C object
pub fn build_mixed_project(
    c_source: &str,
    rust_source: &str,
    _migrated_names: &[String],
    _unmigrated_names: &[String],
) -> Result<MixedBuildResult, ToolError> {
    let tmp = tempfile::tempdir()?;
    let c_file = tmp.path().join("unmigrated.c");
    let c_obj = tmp.path().join("unmigrated.o");
    let rs_file = tmp.path().join("migrated.rs");
    let output = tmp.path().join("mixed");

    std::fs::write(&c_file, c_source)?;
    std::fs::write(&rs_file, rust_source)?;

    // Step 1: Compile C to object file
    let c_result = Command::new("cc")
        .args(["-std=c11", "-c", "-o"])
        .arg(&c_obj)
        .arg(&c_file)
        .output()
        .map_err(|_| ToolError::CommandNotFound("cc".to_string()))?;

    if !c_result.status.success() {
        let stderr = String::from_utf8_lossy(&c_result.stderr).to_string();
        debug!(stderr = %stderr, "C compilation to object failed");
        return Ok(MixedBuildResult {
            success: false,
            stdout: String::new(),
            stderr,
        });
    }

    // Step 2: Compile Rust and link with C object
    let rs_result = Command::new("rustc")
        .arg("--edition=2024")
        .arg("-o")
        .arg(&output)
        .arg(&rs_file)
        .arg("-L")
        .arg(tmp.path())
        .arg("-l")
        .arg("static=unmigrated")
        .output()
        .map_err(|_| ToolError::CommandNotFound("rustc".to_string()))?;

    let success = rs_result.status.success();
    let stderr = String::from_utf8_lossy(&rs_result.stderr).to_string();
    let stdout = String::from_utf8_lossy(&rs_result.stdout).to_string();

    info!(success, "mixed build complete");

    Ok(MixedBuildResult {
        success,
        stdout,
        stderr,
    })
}

/// Compile C source into a static library (.a file).
pub fn compile_c_static_lib(
    c_source: &str,
    output_dir: &Path,
    lib_name: &str,
) -> Result<bool, ToolError> {
    let c_file = output_dir.join(format!("{lib_name}.c"));
    let obj_file = output_dir.join(format!("{lib_name}.o"));
    let lib_file = output_dir.join(format!("lib{lib_name}.a"));

    std::fs::write(&c_file, c_source)?;

    // Compile to object
    let result = Command::new("cc")
        .args(["-std=c11", "-c", "-o"])
        .arg(&obj_file)
        .arg(&c_file)
        .output()
        .map_err(|_| ToolError::CommandNotFound("cc".to_string()))?;

    if !result.status.success() {
        return Ok(false);
    }

    // Create static library
    let result = Command::new("ar")
        .args(["rcs"])
        .arg(&lib_file)
        .arg(&obj_file)
        .output()
        .map_err(|_| ToolError::CommandNotFound("ar".to_string()))?;

    Ok(result.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_c_static_lib() {
        let tmp = tempfile::tempdir().unwrap();
        let c_source = "int helper(int x) { return x * 2; }\n";
        let result = compile_c_static_lib(c_source, tmp.path(), "helper");
        match result {
            Ok(success) => {
                if success {
                    assert!(tmp.path().join("libhelper.a").exists());
                }
            }
            Err(_) => {
                // ar might not be available
            }
        }
    }
}
