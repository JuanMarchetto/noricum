//! P33: Type contract generation for modular migration.
//!
//! Provides helpers to build LLM prompts for type-contract extraction,
//! strip non-type definitions from LLM output, and abbreviate C source
//! to fit token budgets.

use crate::CoreError;
use noricum_agents::providers::{LlmClient, ProviderConfig, select_model};
use std::path::Path;

/// Maximum retries for type contract compilation.
const MAX_CONTRACT_RETRIES: usize = 3;

/// Maximum lines for abbreviated C source context.
const CONTRACT_C_CONTEXT_MAX_LINES: usize = 2000;

/// Maximum lines to inline from resolved headers.
const MAX_HEADER_LINES: usize = 1500;

/// P33: Type contract prompt preamble.
const TYPE_CONTRACT_PREAMBLE: &str = "You are an expert C-to-Rust type translator. \
You translate C type definitions to idiomatic Rust. You output ONLY valid Rust code.";

/// P33: Generate a validated Rust type contract from C shared context.
///
/// 1. Resolve `#include` headers from the same directory for complete type info
/// 2. Build prompt from shared_context + header types + abbreviated C source
/// 3. Call LLM to translate types
/// 4. Post-process: strip fences, strip impl blocks
/// 5. Validate with `check_rust_compiles()`
/// 6. Try rule engine fixes before LLM retry
/// 7. Retry up to [`MAX_CONTRACT_RETRIES`] times on failure
/// 8. Return `None` if all retries fail (graceful degradation)
pub async fn generate_type_contract(
    client: &LlmClient,
    provider_config: &ProviderConfig,
    shared_context: &str,
    c_source: &str,
    source_path: Option<&Path>,
    artifacts: Option<&crate::artifacts::ArtifactStore>,
) -> Result<Option<String>, CoreError> {
    if shared_context.trim().is_empty() {
        tracing::info!("P33: empty shared_context, skipping type contract");
        return Ok(None);
    }

    // P33b: Resolve headers from #include directives in the same directory
    let header_types = if let Some(path) = source_path {
        let resolved = resolve_header_types(shared_context, path);
        if !resolved.is_empty() {
            tracing::info!(
                "P33: resolved {} lines of header type definitions",
                resolved.lines().count()
            );
        }
        resolved
    } else {
        String::new()
    };

    // Use Easy difficulty — type translation is a focused task
    let model_sel =
        select_model(provider_config, noricum_ir::Difficulty::Easy, "type_contract")
            .map_err(|e| CoreError::Orchestration(format!("P33 model selection: {e}")))?;
    let model = &model_sel.model;

    let c_abbreviated = abbreviate_c_for_context(c_source, CONTRACT_C_CONTEXT_MAX_LINES);
    let user_prompt = build_type_contract_prompt(shared_context, &header_types, &c_abbreviated);
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
            .run_prompt(model, TYPE_CONTRACT_PREAMBLE, 0.2, 16384, &prompt)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("P33: LLM call failed attempt {}: {}", attempt + 1, e);
                continue;
            }
        };

        // Post-process: strip fences, impls, and fix common LLM errors
        let cleaned = strip_markdown_fences(&raw);
        let no_impls = strip_impl_blocks(&cleaned);
        let contract = fix_contract_compilation_issues(&no_impls);

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
fn build_type_contract_prompt(
    shared_context: &str,
    header_types: &str,
    c_source_abbreviated: &str,
) -> String {
    let header_section = if header_types.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nHeader type definitions (from #included files — these contain the REAL struct/enum definitions):\n\
             ```c\n{header_types}\n```\n"
        )
    };

    format!(
        "Translate the C type definitions below to Rust. Output ONLY valid, compilable Rust code.\n\n\
         RULES (follow exactly to ensure compilation):\n\
         1. Every struct must have ALL fields from the C definition. No empty structs.\n\
         2. Field types — use these EXACT mappings:\n\
            - void* → usize (opaque handle, cast later)\n\
            - char* → String\n\
            - T* with length → Vec<T>\n\
            - Nullable T* → Option<Box<T>>\n\
            - FILE* → usize (opaque file handle)\n\
            - Function pointers → Option<usize> (opaque callback, cast later)\n\
            - Integer types → use u8/u16/u32/u64/usize/i32/i64 directly\n\
            - bool → bool\n\
         3. Do NOT use: trait objects (dyn), raw pointers (*mut/*const), type aliases, Box<dyn Any>.\n\
         4. Do NOT add #[derive(Clone)] on structs that contain usize handles. Use #[derive(Debug)] only.\n\
         5. Struct names: CamelCase (e.g., MzZipArchive). Field names: snake_case.\n\
         6. Enum variants: CamelCase. Use #[repr(i32)] if C enum has explicit values.\n\
         7. Size/offset constants: pub const NAME: usize = value;\n\
         8. NO impl blocks, NO fn definitions, NO type aliases.\n\
         9. No markdown fences. No explanations. ONLY Rust code.\n\n\
         C source definitions:\n```c\n{shared_context}\n```\n\
         {header_section}\n\
         C source (for field usage context):\n```c\n{c_source_abbreviated}\n```",
        shared_context = shared_context,
        header_section = header_section,
        c_source_abbreviated = c_source_abbreviated,
    )
}

/// Resolve `#include "file.h"` directives from shared_context, reading header files
/// from the same directory as the source file. Extracts type definitions (structs,
/// enums, typedefs, function pointer typedefs) from headers.
fn resolve_header_types(shared_context: &str, source_path: &Path) -> String {
    let source_dir = match source_path.parent() {
        Some(dir) => dir,
        None => return String::new(),
    };

    let mut header_content = String::new();
    let mut total_lines = 0;

    // Find #include "local.h" directives (not <system.h>)
    for line in shared_context.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("#include") {
            let rest = rest.trim();
            if let Some(filename) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                let header_path = source_dir.join(filename);
                if let Ok(content) = std::fs::read_to_string(&header_path) {
                    let type_lines = extract_type_lines_from_header(&content);
                    let line_count = type_lines.lines().count();
                    if total_lines + line_count <= MAX_HEADER_LINES {
                        if !header_content.is_empty() {
                            header_content.push_str("\n\n");
                        }
                        header_content
                            .push_str(&format!("// === From {filename} ===\n{type_lines}"));
                        total_lines += line_count;
                        tracing::debug!(
                            "P33: resolved header {} ({} type lines)",
                            filename,
                            line_count
                        );
                    }
                }
            }
        }
    }

    header_content
}

/// Extract type-relevant lines from a C header: typedefs, structs, enums,
/// #define constants, and function pointer typedefs.
fn extract_type_lines_from_header(header: &str) -> String {
    let lines: Vec<&str> = header.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;
    let mut in_block = false;
    let mut brace_depth = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Skip include guards, pragmas, #include directives, comments-only lines
        if trimmed.starts_with("#pragma")
            || trimmed.starts_with("#ifndef")
            || trimmed.starts_with("#define _")
            || trimmed.starts_with("#endif")
            || trimmed.starts_with("#include")
            || trimmed.starts_with("#ifdef")
            || trimmed.starts_with("#else")
            || trimmed.starts_with("#if ")
        {
            i += 1;
            continue;
        }

        // Track braces for multi-line type definitions
        if in_block {
            result.push(lines[i]);
            for ch in trimmed.chars() {
                if ch == '{' {
                    brace_depth += 1;
                }
                if ch == '}' {
                    brace_depth -= 1;
                }
            }
            if brace_depth <= 0 {
                in_block = false;
            }
            i += 1;
            continue;
        }

        // Detect type definition starts
        let is_type_line = trimmed.starts_with("typedef ")
            || trimmed.starts_with("struct ")
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("#define ")
            || trimmed.starts_with("extern ")
            || (trimmed.contains("typedef") && trimmed.contains("(*)"));

        if is_type_line {
            result.push(lines[i]);
            // Check if this opens a multi-line block
            for ch in trimmed.chars() {
                if ch == '{' {
                    brace_depth += 1;
                }
                if ch == '}' {
                    brace_depth -= 1;
                }
            }
            if brace_depth > 0 {
                in_block = true;
            }
        }

        i += 1;
    }

    result.join("\n")
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

/// Fix common LLM compilation errors in type contracts:
/// - Replace `dyn TraitA + TraitB` with `usize` (E0225)
/// - Replace bare `dyn Trait` without `Box` (E0782)
/// - Remove `Clone` from derive when struct might not be Clone-safe
/// - Replace `*mut T` / `*const T` with `usize`
/// - Remove `type Alias = ...` lines
fn fix_contract_compilation_issues(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let re_multi_trait =
        regex::Regex::new(r"(?:Box<|Option<Box<)?dyn\s+[\w:]+(?:\s*\+\s*[\w:]+)+>?>?").ok();
    let re_bare_dyn = regex::Regex::new(r"dyn\s+[\w:]+").ok();
    let re_raw_ptr = regex::Regex::new(r"\*(?:mut|const)\s+[\w:]+").ok();
    for line in source.lines() {
        let trimmed = line.trim();

        // Strip type aliases (e.g., `type MzBool = i32;`)
        if (trimmed.starts_with("pub type ") || trimmed.starts_with("type "))
            && trimmed.contains('=')
            && trimmed.ends_with(';')
        {
            result.push_str(&format!("// P33: stripped alias: {trimmed}\n"));
            continue;
        }

        let mut fixed = line.to_string();

        // Replace Box<dyn TraitA + TraitB> → usize (E0225)
        if fixed.contains("dyn ") && fixed.contains(" + ") {
            // Multi-trait objects are invalid — replace entire type with usize
            if let Some(ref re) = re_multi_trait {
                fixed = re.replace_all(&fixed, "usize").to_string();
            }
        }

        // Replace bare `dyn Trait` (without Box) → usize (E0782)
        if fixed.contains(": dyn ") || fixed.contains("(dyn ") {
            // Use a simple regex and then check the char before the match to avoid
            // replacing `dyn` inside `Box<dyn ...>` or `(dyn ...` contexts.
            if let Some(ref re) = re_bare_dyn {
                let mut result = String::new();
                let mut last_end = 0;
                for m in re.find_iter(&fixed) {
                    let start = m.start();
                    // Only replace if not preceded by < or (
                    let preceded_by_angle_or_paren = start > 0
                        && matches!(fixed.as_bytes().get(start - 1), Some(b'<') | Some(b'('));
                    result.push_str(&fixed[last_end..start]);
                    if preceded_by_angle_or_paren {
                        result.push_str(m.as_str());
                    } else {
                        result.push_str("usize");
                    }
                    last_end = m.end();
                }
                result.push_str(&fixed[last_end..]);
                fixed = result;
            }
        }

        // Replace raw pointers → usize
        if fixed.contains("*mut ") || fixed.contains("*const ") {
            fixed = fixed.replace("*mut std::ffi::c_void", "usize");
            fixed = fixed.replace("*const std::ffi::c_void", "usize");
            fixed = fixed.replace("*mut c_void", "usize");
            fixed = fixed.replace("*const c_void", "usize");
            // Generic pointer patterns
            if let Some(ref re) = re_raw_ptr {
                fixed = re.replace_all(&fixed, "usize").to_string();
            }
        }

        // Replace derive(Debug, Clone) → derive(Debug) to avoid E0277 on non-Clone fields
        if trimmed.starts_with("#[derive(") && fixed.contains("Clone") {
            fixed = fixed.replace(", Clone", "").replace("Clone, ", "").replace("Clone", "Debug");
            // Deduplicate Debug
            fixed = fixed.replace("Debug, Debug", "Debug").replace("(Debug, )", "(Debug)");
        }

        result.push_str(&fixed);
        result.push('\n');
    }
    result
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
            build_type_contract_prompt("typedef struct { int x; } Foo;", "", "void bar() {}");
        assert!(prompt.contains("typedef struct { int x; } Foo;"));
        assert!(prompt.contains("void bar() {}"));
        assert!(prompt.contains("NO impl blocks"));
        assert!(prompt.contains("EXACT mappings"));
        assert!(prompt.contains("Do NOT use: trait objects"));
    }

    #[test]
    fn test_build_type_contract_prompt_includes_header_types() {
        let prompt = build_type_contract_prompt(
            "#include \"miniz.h\"",
            "// === From miniz.h ===\ntypedef unsigned int mz_uint;",
            "void foo() {}",
        );
        assert!(prompt.contains("Header type definitions"));
        assert!(prompt.contains("From miniz.h"));
        assert!(prompt.contains("mz_uint"));
    }

    #[test]
    fn test_resolve_header_types_finds_local_headers() {
        // Create a temp directory with a .c and .h file
        let dir = std::env::temp_dir().join("noricum_test_headers");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join("test.h"),
            "typedef struct { int x; int y; } Point;\nenum Color { RED, GREEN };\n",
        )
        .unwrap();
        let source_path = dir.join("test.c");
        std::fs::write(&source_path, "").unwrap();

        let shared = "#include \"test.h\"\n#define MAX 100\n";
        let result = resolve_header_types(shared, &source_path);
        assert!(result.contains("Point"));
        assert!(result.contains("Color"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_extract_type_lines_from_header() {
        let header = r#"
#ifndef MY_HEADER_H
#define MY_HEADER_H

#include <stdio.h>

typedef unsigned int mz_uint;

typedef struct {
    int x;
    int y;
} Point;

enum Color { RED, GREEN, BLUE };

void some_function(int arg);

#define MAX_SIZE 100

typedef int (*callback_fn)(void*);

#endif
"#;
        let result = extract_type_lines_from_header(header);
        assert!(result.contains("typedef unsigned int mz_uint"));
        assert!(result.contains("typedef struct"));
        assert!(result.contains("Point"));
        assert!(result.contains("enum Color"));
        assert!(result.contains("#define MAX_SIZE"));
        assert!(result.contains("callback_fn"));
        // Should NOT contain function declarations or include guards
        assert!(!result.contains("some_function"));
        assert!(!result.contains("#ifndef"));
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

    #[test]
    fn test_fix_contract_compilation_issues() {
        let input = r#"
#[derive(Debug, Clone)]
pub struct MzZipArchive {
    pub m_p_file: Option<Box<dyn std::io::Read + std::io::Write>>,
    pub m_p_state: *mut MzZipInternalState,
    pub m_archive_size: u64,
}

pub type MzBool = i32;
pub type MzUint = u32;
"#;
        let result = fix_contract_compilation_issues(input);

        // Clone should be stripped (struct has usize handles)
        assert!(!result.contains("Clone"), "Clone should be stripped");
        assert!(result.contains("Debug"), "Debug should remain");

        // dyn multi-trait → usize
        assert!(!result.contains("dyn std::io::Read"), "trait objects should be replaced");
        assert!(result.contains("usize"), "should have usize replacements");

        // raw pointers → usize
        assert!(!result.contains("*mut"), "raw pointers should be replaced");

        // type aliases commented out (not active code)
        assert!(result.contains("// P33: stripped alias"), "aliases should be commented out");
        assert!(!result.contains("\npub type MzBool"), "aliases should not be active");

        // Should compile
        let compile = noricum_tools::compiler::check_rust_compiles(&result).unwrap();
        assert!(compile.success, "Fixed contract should compile: {}", compile.stderr);
    }
}
