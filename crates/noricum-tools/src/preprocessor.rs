/// C preprocessor handling: expand macros and resolve #includes.
///
/// Runs `gcc -E` (fallback: `cc -E`, `cpp`) to preprocess C source,
/// then strips line directives and system header content to produce
/// clean expanded source suitable for AST analysis.
use std::path::Path;
use std::process::Command;

use tracing::{debug, info, warn};

use crate::ToolError;

/// Configuration for the C preprocessor.
#[derive(Debug, Clone, Default)]
pub struct PreprocessorConfig {
    /// Additional include paths (-I flags).
    pub include_paths: Vec<String>,
    /// Preprocessor defines (-D flags).
    pub defines: Vec<String>,
    /// Override the preprocessor command (default: auto-detect).
    pub command: Option<String>,
}

/// Result of preprocessing a C source file.
#[derive(Debug, Clone)]
pub struct PreprocessedSource {
    /// The preprocessed (macro-expanded) source code.
    pub source: String,
    /// The original source code before preprocessing.
    pub original: String,
    /// Whether preprocessing was actually performed (false = graceful fallback).
    pub was_preprocessed: bool,
}

/// Preprocess a C file using the system preprocessor.
///
/// Tries `gcc -E`, then `cc -E`, then `cpp`. Falls back to original source
/// if no preprocessor is available.
pub fn preprocess_file(
    c_file: &Path,
    config: &PreprocessorConfig,
) -> Result<PreprocessedSource, ToolError> {
    let original = std::fs::read_to_string(c_file)?;

    let commands = if let Some(ref cmd) = config.command {
        vec![cmd.as_str()]
    } else {
        vec!["gcc", "cc", "cpp"]
    };

    for cmd_name in &commands {
        let mut cmd = Command::new(cmd_name);
        cmd.arg("-E");

        for path in &config.include_paths {
            cmd.arg("-I").arg(path);
        }
        for define in &config.defines {
            cmd.arg(format!("-D{define}"));
        }
        cmd.arg(c_file);

        match cmd.output() {
            Ok(output) if output.status.success() => {
                let raw = String::from_utf8_lossy(&output.stdout).to_string();
                let source = strip_preprocessor_output(&raw, c_file);
                info!(
                    preprocessor = cmd_name,
                    original_lines = original.lines().count(),
                    preprocessed_lines = source.lines().count(),
                    "preprocessing complete"
                );
                return Ok(PreprocessedSource {
                    source,
                    original,
                    was_preprocessed: true,
                });
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                debug!(
                    preprocessor = cmd_name,
                    stderr = %stderr,
                    "preprocessor failed, trying next"
                );
            }
            Err(e) => {
                debug!(
                    preprocessor = cmd_name,
                    error = %e,
                    "preprocessor not found, trying next"
                );
            }
        }
    }

    warn!("no preprocessor available, using original source");
    Ok(PreprocessedSource {
        source: original.clone(),
        original,
        was_preprocessed: false,
    })
}

/// Preprocess a C source string (writes to temp file first).
pub fn preprocess_source(
    c_source: &str,
    config: &PreprocessorConfig,
) -> Result<PreprocessedSource, ToolError> {
    let tmp = tempfile::tempdir()?;
    let c_file = tmp.path().join("input.c");
    std::fs::write(&c_file, c_source)?;
    preprocess_file(&c_file, config)
}

/// Check if any C preprocessor is available.
pub fn preprocessor_available() -> bool {
    for cmd in &["gcc", "cc", "cpp"] {
        if Command::new(cmd)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
        {
            return true;
        }
    }
    false
}

/// Strip line directives and system header content from preprocessor output.
///
/// Keeps only content from the original file (tracked via `# N "filename"` directives).
fn strip_preprocessor_output(raw: &str, original_file: &Path) -> String {
    let file_str = original_file.to_string_lossy();
    // Also match just the filename without path
    let file_name = original_file
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut result = String::new();
    let mut in_original = false;

    for line in raw.lines() {
        if line.starts_with('#') && line.contains('"') {
            // Line directive: # linenum "filename" [flags]
            let is_original = line.contains(&*file_str)
                || line.contains(&file_name)
                || line.contains("\"<stdin>\"");
            in_original = is_original;
            continue;
        }

        if in_original && !line.trim().is_empty() {
            result.push_str(line);
            result.push('\n');
        }
    }

    if result.is_empty() {
        // If filtering removed everything, return the raw output minus directives
        strip_line_directives(raw)
    } else {
        result
    }
}

/// Remove `# N "file"` line directives from preprocessor output.
fn strip_line_directives(raw: &str) -> String {
    raw.lines()
        .filter(|line| !line.starts_with('#') || !line.contains('"'))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocessor_available() {
        // On most systems, at least cc should be available
        let available = preprocessor_available();
        // Just test that it doesn't panic; result depends on environment
        let _ = available;
    }

    #[test]
    fn test_preprocess_source_simple() {
        let source = r#"
#define MAX 100
int get_max(void) { return MAX; }
"#;
        let config = PreprocessorConfig::default();
        let result = preprocess_source(source, &config);
        match result {
            Ok(pp) => {
                if pp.was_preprocessed {
                    assert!(
                        pp.source.contains("100") || pp.source.contains("get_max"),
                        "preprocessed source should contain expanded macro or function"
                    );
                }
            }
            Err(_) => {
                // No preprocessor available, that's ok
            }
        }
    }

    #[test]
    fn test_preprocess_source_ifdef() {
        let source = r#"
#ifdef FEATURE_X
int feature_x(void) { return 1; }
#else
int feature_x(void) { return 0; }
#endif
"#;
        let config = PreprocessorConfig::default();
        let result = preprocess_source(source, &config);
        match result {
            Ok(pp) => {
                if pp.was_preprocessed {
                    assert!(pp.source.contains("return 0"));
                    assert!(
                        !pp.source.contains("return 1"),
                        "FEATURE_X not defined, so return 1 should be stripped"
                    );
                }
            }
            Err(_) => {}
        }
    }

    #[test]
    fn test_preprocess_with_define() {
        let source = r#"
#ifdef FEATURE_X
int feature_x(void) { return 1; }
#else
int feature_x(void) { return 0; }
#endif
"#;
        let config = PreprocessorConfig {
            defines: vec!["FEATURE_X".to_string()],
            ..Default::default()
        };
        let result = preprocess_source(source, &config);
        match result {
            Ok(pp) => {
                if pp.was_preprocessed {
                    assert!(
                        pp.source.contains("return 1"),
                        "FEATURE_X defined, should get return 1"
                    );
                }
            }
            Err(_) => {}
        }
    }

    #[test]
    fn test_strip_line_directives() {
        let raw = r#"# 1 "test.c"
# 1 "<built-in>"
# 1 "<command-line>"
# 1 "test.c"
int add(int a, int b) { return a + b; }
"#;
        let stripped = strip_line_directives(raw);
        assert!(!stripped.contains("# 1"));
        assert!(stripped.contains("int add"));
    }

    #[test]
    fn test_strip_system_headers() {
        let raw = r#"# 1 "test.c"
# 1 "/usr/include/stdio.h"
extern int printf(const char *, ...);
# 2 "test.c"
int add(int a, int b) { return a + b; }
"#;
        let path = Path::new("test.c");
        let result = strip_preprocessor_output(raw, path);
        assert!(result.contains("int add"));
        // System header content should be filtered out
        assert!(
            !result.contains("extern int printf"),
            "system headers should be stripped"
        );
    }

    #[test]
    fn test_fallback_to_original() {
        let config = PreprocessorConfig {
            command: Some("nonexistent_preprocessor_12345".to_string()),
            ..Default::default()
        };
        let source = "int f(void) { return 42; }";
        let result = preprocess_source(source, &config).unwrap();
        assert!(!result.was_preprocessed);
        assert_eq!(result.source, result.original);
    }
}
