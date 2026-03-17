//! P33: Type contract generation for modular migration.
//!
//! Provides helpers to build LLM prompts for type-contract extraction,
//! strip non-type definitions from LLM output, and abbreviate C source
//! to fit token budgets.

use crate::CoreError;
use noricum_agents::providers::{LlmClient, ProviderConfig, select_model};

/// Maximum retries for type contract compilation.
const MAX_CONTRACT_RETRIES: usize = 3;

/// Maximum lines for abbreviated C source context.
const CONTRACT_C_CONTEXT_MAX_LINES: usize = 2000;

/// P33: Type contract prompt preamble.
const TYPE_CONTRACT_PREAMBLE: &str = "You are an expert C-to-Rust type translator. \
You translate C type definitions to idiomatic Rust. You output ONLY valid Rust code.";

/// P33: Generate a validated Rust type contract from C shared context.
///
/// 1. Build prompt from shared_context + abbreviated C source
/// 2. Call LLM to translate types
/// 3. Post-process: strip fences, strip impl blocks
/// 4. Validate with `check_rust_compiles()`
/// 5. Try rule engine fixes before LLM retry
/// 6. Retry up to [`MAX_CONTRACT_RETRIES`] times on failure
/// 7. Return `None` if all retries fail (graceful degradation)
pub async fn generate_type_contract(
    client: &LlmClient,
    provider_config: &ProviderConfig,
    shared_context: &str,
    c_source: &str,
    artifacts: Option<&crate::artifacts::ArtifactStore>,
) -> Result<Option<String>, CoreError> {
    if shared_context.trim().is_empty() {
        tracing::info!("P33: empty shared_context, skipping type contract");
        return Ok(None);
    }

    // Use Easy difficulty — type translation is a focused task
    let model_sel =
        select_model(provider_config, noricum_ir::Difficulty::Easy, "type_contract")
            .map_err(|e| CoreError::Orchestration(format!("P33 model selection: {e}")))?;
    let model = &model_sel.model;

    let c_abbreviated = abbreviate_c_for_context(c_source, CONTRACT_C_CONTEXT_MAX_LINES);
    let user_prompt = build_type_contract_prompt(shared_context, &c_abbreviated);
    let mut last_errors = String::new();

    for attempt in 0..MAX_CONTRACT_RETRIES {
        let prompt = if attempt == 0 {
            user_prompt.clone()
        } else {
            format!(
                "{}\n\nPREVIOUS ATTEMPT FAILED TO COMPILE. Fix these errors:\n{}\n\nOutput ONLY corrected Rust code.",
                user_prompt, last_errors
            )
        };

        let raw = match client
            .run_prompt(model, TYPE_CONTRACT_PREAMBLE, 0.2, 8192, &prompt)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("P33: LLM call failed attempt {}: {}", attempt + 1, e);
                continue;
            }
        };

        // Post-process
        let cleaned = strip_markdown_fences(&raw);
        let contract = strip_impl_blocks(&cleaned);

        if contract.trim().is_empty() {
            tracing::warn!(
                "P33: empty contract after post-processing, attempt {}",
                attempt + 1
            );
            continue;
        }

        // Validate compilation
        match noricum_tools::compiler::check_rust_compiles(&contract) {
            Ok(result) if result.success => {
                tracing::info!(
                    "P33: type contract compiled (attempt {}, {} lines)",
                    attempt + 1,
                    contract.lines().count()
                );
                if let Some(store) = artifacts {
                    let _ = store.save_type_contract(&contract);
                }
                return Ok(Some(contract));
            }
            Ok(result) => {
                tracing::warn!(
                    "P33: contract failed to compile (attempt {}): {}",
                    attempt + 1,
                    &result.stderr[..result.stderr.len().min(500)]
                );
                // Try mechanical fixes via rule engine before LLM retry
                let parsed_errors =
                    noricum_tools::repair_rules::parse_rustc_errors(&result.stderr);
                let fixed =
                    noricum_tools::repair_rules::apply_all_rules(&contract, &parsed_errors);
                if fixed != contract {
                    match noricum_tools::compiler::check_rust_compiles(&fixed) {
                        Ok(r2) if r2.success => {
                            tracing::info!(
                                "P33: rule engine fixed contract (attempt {})",
                                attempt + 1
                            );
                            if let Some(store) = artifacts {
                                let _ = store.save_type_contract(&fixed);
                            }
                            return Ok(Some(fixed));
                        }
                        _ => {}
                    }
                }
                last_errors = result.stderr;
            }
            Err(e) => {
                tracing::warn!("P33: compiler error attempt {}: {}", attempt + 1, e);
                continue;
            }
        }
    }

    tracing::warn!(
        "P33: type contract failed after {} retries, proceeding without contract",
        MAX_CONTRACT_RETRIES
    );
    Ok(None)
}

/// Build the user prompt for type contract generation.
fn build_type_contract_prompt(shared_context: &str, c_source_abbreviated: &str) -> String {
    format!(
        "Translate ALL type definitions from this C code to idiomatic Rust.\n\n\
         Rules:\n\
         - Convert typedef struct → pub struct with pub fields\n\
         - Convert enum → pub enum (use #[repr(i32)] if values have explicit integer assignments)\n\
         - Convert #define constants → pub const\n\
         - Convert typedef aliases → pub type\n\
         - Use idiomatic Rust types:\n\
           - void* → Box<dyn std::any::Any> (or concrete type if determinable)\n\
           - char* → String (owned) or &str (borrowed)\n\
           - T* + size_t len → Vec<T>\n\
           - Nullable pointers → Option<T>\n\
           - Function pointers → fn(...) -> ... or Option<fn(...) -> ...> for nullable\n\
         - Add #[derive(Debug, Clone)] where appropriate\n\
         - NO function bodies — only struct/enum/const/type definitions\n\
         - NO impl blocks\n\
         - NO standalone fn definitions\n\
         - Include brief doc comments for each type explaining its purpose\n\n\
         Output ONLY valid Rust code. No markdown fences. No explanations.\n\n\
         C type definitions:\n```c\n{shared_context}\n```\n\n\
         Full C source (for context on how types are used):\n```c\n{c_source_abbreviated}\n```",
        shared_context = shared_context,
        c_source_abbreviated = c_source_abbreviated,
    )
}

/// Build abbreviated C source: first N lines that fit in budget.
fn abbreviate_c_for_context(c_source: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = c_source.lines().collect();
    if lines.len() <= max_lines {
        return c_source.to_string();
    }
    lines[..max_lines].join("\n")
}

/// Strip impl blocks and standalone fn definitions from LLM output.
/// Keeps only struct/enum/const/type definitions.
fn strip_impl_blocks(rust_source: &str) -> String {
    let lines: Vec<&str> = rust_source.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("impl ") {
            i = skip_braced_block_lines(&lines, i);
            continue;
        }
        if (trimmed.starts_with("pub fn ") || trimmed.starts_with("fn "))
            && trimmed.contains('(')
        {
            if trimmed.contains('{') {
                i = skip_braced_block_lines(&lines, i);
            } else {
                i += 1;
            }
            continue;
        }
        result.push(lines[i]);
        i += 1;
    }
    result.join("\n")
}

/// Skip a braced block starting at `start`, returns index after closing brace.
fn skip_braced_block_lines(lines: &[&str], start: usize) -> usize {
    let mut depth = 0;
    let mut i = start;
    while i < lines.len() {
        for ch in lines[i].chars() {
            if ch == '{' {
                depth += 1;
            }
            if ch == '}' {
                depth -= 1;
            }
        }
        i += 1;
        if depth <= 0 && i > start + 1 {
            break;
        }
    }
    i
}

/// Strip markdown fences from LLM output.
fn strip_markdown_fences(source: &str) -> String {
    source
        .lines()
        .filter(|line| {
            let t = line.trim();
            !t.starts_with("```")
        })
        .collect::<Vec<&str>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_type_contract_prompt_contains_shared_context() {
        let prompt =
            build_type_contract_prompt("typedef struct { int x; } Foo;", "void bar() {}");
        assert!(prompt.contains("typedef struct { int x; } Foo;"));
        assert!(prompt.contains("void bar() {}"));
        assert!(prompt.contains("NO impl blocks"));
    }

    #[test]
    fn test_strip_impl_blocks() {
        let input = "pub struct Foo {\n    pub x: i32,\n}\n\nimpl Foo {\n    pub fn new() -> Self {\n        Foo { x: 0 }\n    }\n}\n\npub enum Bar {\n    A,\n    B,\n}\n\nimpl Default for Bar {\n    fn default() -> Self {\n        Bar::A\n    }\n}\n\npub const MAX: i32 = 100;\n";
        let result = strip_impl_blocks(input);
        assert!(result.contains("pub struct Foo"));
        assert!(result.contains("pub enum Bar"));
        assert!(result.contains("pub const MAX"));
        assert!(!result.contains("impl Foo"));
        assert!(!result.contains("impl Default"));
        assert!(!result.contains("fn new"));
    }

    #[test]
    fn test_strip_impl_blocks_preserves_empty() {
        assert_eq!(strip_impl_blocks(""), "");
    }

    #[test]
    fn test_strip_standalone_fn() {
        let input = "pub struct X { pub a: i32 }\npub fn helper() -> i32 {\n    42\n}\n";
        let result = strip_impl_blocks(input);
        assert!(result.contains("pub struct X"));
        assert!(!result.contains("pub fn helper"));
    }

    #[test]
    fn test_abbreviate_under_budget() {
        let source = "line1\nline2\nline3";
        assert_eq!(abbreviate_c_for_context(source, 100), source);
    }

    #[test]
    fn test_abbreviate_over_budget() {
        let source = "line1\nline2\nline3\nline4\nline5";
        let result = abbreviate_c_for_context(source, 3);
        assert_eq!(result, "line1\nline2\nline3");
    }

    #[test]
    fn test_strip_markdown_fences() {
        let input = "```rust\npub struct Foo {}\n```\n";
        let result = strip_markdown_fences(input);
        assert!(!result.contains("```"));
        assert!(result.contains("pub struct Foo"));
    }

    #[test]
    fn test_validate_type_contract_compiles() {
        let contract =
            "pub struct Foo { pub x: i32 }\npub enum Bar { A, B }\npub const MAX: i32 = 100;\n";
        let result = noricum_tools::compiler::check_rust_compiles(contract).unwrap();
        assert!(
            result.success,
            "Valid type contract should compile: {}",
            result.stderr
        );
    }

    #[test]
    fn test_validate_type_contract_rejects_invalid() {
        let contract = "pub struct Foo { pub x: UnknownType }\n";
        let result = noricum_tools::compiler::check_rust_compiles(contract).unwrap();
        assert!(!result.success);
    }

    #[test]
    fn test_strip_impl_blocks_preserves_complex_types() {
        let input = r#"
/// Zip internal state (opaque).
pub struct ZipInternalState;

/// Zip archive state.
pub struct ZipArchive {
    pub archive_size: u64,
    pub total_files: u32,
    pub zip_mode: ZipMode,
    pub state: Option<Box<ZipInternalState>>,
}

/// Zip operation mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZipMode {
    Reading,
    Writing,
    WritingHasBeenFinalized,
}

/// Zip error codes.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(i32)]
pub enum ZipError {
    NoError = 0,
    NotAnArchive = 1,
    FailedFindingCentralDir = 2,
    CrcCheckFailed = 3,
}

impl ZipArchive {
    pub fn new() -> Self {
        ZipArchive {
            archive_size: 0,
            total_files: 0,
            zip_mode: ZipMode::Reading,
            state: None,
        }
    }
}

pub const MZ_ZIP_MAX_IO_BUF_SIZE: usize = 64 * 1024;
pub type MzUint = u32;
"#;
        let result = strip_impl_blocks(input);
        assert!(result.contains("pub struct ZipArchive"));
        assert!(result.contains("pub enum ZipMode"));
        assert!(result.contains("pub enum ZipError"));
        assert!(result.contains("pub const MZ_ZIP_MAX_IO_BUF_SIZE"));
        assert!(result.contains("pub type MzUint"));
        assert!(!result.contains("impl ZipArchive"));
        assert!(!result.contains("fn new"));

        // Verify it compiles
        let compile_result = noricum_tools::compiler::check_rust_compiles(&result).unwrap();
        assert!(
            compile_result.success,
            "Stripped contract should compile: {}",
            compile_result.stderr
        );
    }
}
