//! P33: Type contract generation for modular migration.
//!
//! Provides helpers to build LLM prompts for type-contract extraction,
//! strip non-type definitions from LLM output, and abbreviate C source
//! to fit token budgets.

#[allow(unused_imports)]
use crate::CoreError;
#[allow(unused_imports)]
use noricum_agents::providers::{LlmClient, ProviderConfig, select_model};

/// P33: Type contract prompt preamble.
#[allow(dead_code)]
const TYPE_CONTRACT_PREAMBLE: &str = "You are an expert C-to-Rust type translator. \
You translate C type definitions to idiomatic Rust. You output ONLY valid Rust code.";

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
}
