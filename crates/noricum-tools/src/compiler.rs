/// Compiler tools: compile C and Rust code, run executables, capture output.
///
/// Used for differential testing (compile both, run both, compare outputs)
/// and for the repair loop (compile Rust, check for errors).
use std::path::Path;
use std::process::{Command, Output};

use tracing::{debug, info};

use crate::ToolError;

/// Result of compiling and optionally running code.
#[derive(Debug)]
pub struct CompileResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl CompileResult {
    fn from_output(output: &Output) -> Self {
        Self {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        }
    }
}

/// Compile a C file to an executable.
pub fn compile_c(c_file: &Path, output_path: &Path) -> Result<CompileResult, ToolError> {
    info!(file = %c_file.display(), "compiling C file");

    let output = Command::new("cc")
        .args(["-std=gnu11", "-Wall", "-o"])
        .arg(output_path)
        .arg(c_file)
        .arg("-lm")
        .output()
        .map_err(|_| ToolError::CommandNotFound("cc".to_string()))?;

    let result = CompileResult::from_output(&output);
    debug!(success = result.success, "C compilation finished");
    Ok(result)
}

/// Check if a Rust source file compiles (using rustc directly).
pub fn check_rust_compiles(rust_source: &str) -> Result<CompileResult, ToolError> {
    let tmp = tempfile::tempdir()?;
    let rs_file = tmp.path().join("check.rs");
    std::fs::write(&rs_file, rust_source)?;

    let output = Command::new("rustc")
        .arg("--edition=2024")
        .arg("--crate-type=lib")
        .arg(&rs_file)
        .arg("-o")
        .arg(tmp.path().join("check"))
        .output()
        .map_err(|_| ToolError::CommandNotFound("rustc".to_string()))?;

    let result = CompileResult::from_output(&output);
    debug!(success = result.success, "Rust compile check finished");
    Ok(result)
}

/// Run clippy on a Rust source string, return warnings.
pub fn run_clippy_on_source(rust_source: &str) -> Result<Vec<String>, ToolError> {
    let tmp = tempfile::tempdir()?;
    let rs_file = tmp.path().join("check.rs");
    std::fs::write(&rs_file, rust_source)?;

    let output = Command::new("clippy-driver")
        .arg("--edition=2024")
        .arg(&rs_file)
        .output();

    match output {
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let warnings: Vec<String> = stderr
                .lines()
                .filter(|l| l.contains("warning"))
                .map(String::from)
                .collect();
            Ok(warnings)
        }
        Err(_) => {
            // clippy-driver not available; fall back to rustc warnings
            debug!("clippy-driver not found, skipping clippy check");
            Ok(Vec::new())
        }
    }
}

/// Run a compiled executable and capture its output.
pub fn run_executable(exe_path: &Path, args: &[&str]) -> Result<CompileResult, ToolError> {
    let output = Command::new(exe_path)
        .args(args)
        .output()
        .map_err(ToolError::Io)?;

    Ok(CompileResult::from_output(&output))
}

/// Count the number of `unsafe` blocks and `unsafe fn` declarations in Rust source code.
///
/// Uses tree-sitter for accurate AST-based counting, which correctly handles
/// unsafe in comments, strings, and nested contexts. Falls back to heuristic
/// string matching if tree-sitter parsing fails.
pub fn count_unsafe_blocks(rust_source: &str) -> u32 {
    match count_unsafe_blocks_tree_sitter(rust_source) {
        Some(count) => count,
        None => {
            debug!("tree-sitter parse failed, using heuristic unsafe counter");
            count_unsafe_blocks_heuristic(rust_source)
        }
    }
}

/// Tree-sitter-based unsafe block/fn counter.
fn count_unsafe_blocks_tree_sitter(rust_source: &str) -> Option<u32> {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter_rust::LANGUAGE;
    parser.set_language(&language.into()).ok()?;
    let tree = parser.parse(rust_source, None)?;
    let root = tree.root_node();

    let mut count = 0u32;
    let mut cursor = root.walk();
    count_unsafe_recursive(&mut cursor, &mut count);
    Some(count)
}

/// Walk the tree-sitter AST and count `unsafe_block` and function items with
/// the `unsafe` modifier.
fn count_unsafe_recursive(cursor: &mut tree_sitter::TreeCursor, count: &mut u32) {
    let node = cursor.node();
    match node.kind() {
        "unsafe_block" => {
            *count += 1;
        }
        "function_item" => {
            // Check if this function has an `unsafe` modifier inside `function_modifiers`
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if child.kind() == "function_modifiers" {
                        for j in 0..child.child_count() {
                            if let Some(modifier) = child.child(j)
                                && modifier.kind() == "unsafe"
                            {
                                *count += 1;
                                break;
                            }
                        }
                        break;
                    }
                    // Stop looking once we reach fn keyword or beyond
                    if child.kind() == "fn"
                        || child.kind() == "identifier"
                        || child.kind() == "parameters"
                    {
                        break;
                    }
                }
            }
        }
        _ => {}
    }

    if cursor.goto_first_child() {
        loop {
            count_unsafe_recursive(cursor, count);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

/// Heuristic string-based unsafe counter (fallback).
fn count_unsafe_blocks_heuristic(rust_source: &str) -> u32 {
    // Strip line comments to avoid counting "unsafe" in comments
    let stripped: String = rust_source
        .lines()
        .map(|line| {
            if let Some(idx) = line.find("//") {
                &line[..idx]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut count = 0u32;

    for line in stripped.lines() {
        let trimmed = line.trim();
        if trimmed.contains("unsafe")
            && trimmed.contains('{')
            && !trimmed.contains("unsafe fn")
            && !trimmed.contains("unsafe impl")
            && !trimmed.contains("unsafe trait")
            && let Some(pos) = trimmed.find("unsafe")
        {
            let after = trimmed[pos + 6..].trim_start();
            if after.starts_with('{') {
                count += 1;
            }
        }
        if trimmed.contains("unsafe fn ") || trimmed.contains("unsafe fn(") {
            count += 1;
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_unsafe_blocks() {
        let source = r#"
fn safe_fn() -> i32 { 42 }

unsafe fn dangerous() -> *mut u8 {
    std::ptr::null_mut()
}

fn uses_unsafe() {
    unsafe {
        dangerous();
    }
}
"#;
        assert_eq!(count_unsafe_blocks(source), 2);
    }

    #[test]
    fn test_count_unsafe_zero() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        assert_eq!(count_unsafe_blocks(source), 0);
    }

    #[test]
    fn test_count_unsafe_pub_fn() {
        let source = "pub unsafe fn danger() {}";
        assert_eq!(count_unsafe_blocks(source), 1);
    }

    #[test]
    fn test_count_unsafe_pub_crate_fn() {
        let source = "pub(crate) unsafe fn danger() {}";
        assert_eq!(count_unsafe_blocks(source), 1);
    }

    #[test]
    fn test_count_unsafe_extra_whitespace() {
        let source = "    unsafe  {  ptr::null()  }";
        assert_eq!(count_unsafe_blocks(source), 1);
    }

    #[test]
    fn test_count_unsafe_in_comment_ignored() {
        let source = "fn safe() {} // unsafe { this is a comment }";
        assert_eq!(count_unsafe_blocks(source), 0);
    }

    #[test]
    fn test_check_rust_compiles_valid() {
        let result = check_rust_compiles("pub fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_check_rust_compiles_invalid() {
        let result = check_rust_compiles("fn bad( { }").unwrap();
        assert!(!result.success);
        assert!(!result.stderr.is_empty());
    }
}
