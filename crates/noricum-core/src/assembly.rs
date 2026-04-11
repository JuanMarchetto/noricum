//! Assembly and deduplication of modular migration outputs.
//!
//! After each module is independently translated and repaired, this module
//! combines them into a single Rust source, deduplicating `use` statements,
//! type definitions, and stripping duplicate struct/enum/const from later modules.

use std::collections::{BTreeMap, BTreeSet, HashSet};

/// P25: Build accumulated Rust source from completed module outputs for incremental validation.
/// This allows validating module N against the combined output of modules 0..N-1.
/// Only includes modules that compiled successfully to avoid error propagation (P26).
pub(crate) fn build_assembly_context(module_outputs: &[(String, String, bool)]) -> String {
    let compilable: Vec<&str> = module_outputs
        .iter()
        .filter(|(_, _, compiles)| *compiles)
        .map(|(_, code, _)| code.as_str())
        .collect();
    if compilable.is_empty() {
        return String::new();
    }
    compilable.join("\n\n")
}

/// P25: Validate a module in the context of prior module outputs.
/// Compiles `prior_outputs + current_module` together to resolve cross-module deps.
pub(crate) fn validate_module_with_assembly(
    mod_unit: &noricum_ir::FunctionUnit,
    module_name: &str,
    module_outputs: &[(String, String, bool)],
    threshold: u32,
) -> Result<noricum_validation::ValidationResult, crate::CoreError> {
    let assembly_context = build_assembly_context(module_outputs);
    if assembly_context.is_empty() {
        return Ok(noricum_validation::validate_with_threshold(
            mod_unit, threshold,
        )?);
    }
    // Combine prior outputs with current module for compilation check
    let combined = format!(
        "{}\n\n// --- Module: {} ---\n{}",
        assembly_context,
        module_name,
        mod_unit.rust_output.as_deref().unwrap_or("")
    );
    let mut temp_unit = mod_unit.clone();
    temp_unit.rust_output = Some(combined);
    Ok(noricum_validation::validate_with_threshold(
        &temp_unit, threshold,
    )?)
}

/// Assemble the final Rust output from individually migrated module outputs.
///
/// P27: Deduplicates `use` statements and type/struct/enum/const definitions
/// across modules. The first module to define a name wins; subsequent modules
/// have their duplicate definitions stripped. This prevents compilation errors
/// from modules that independently translate the same C types.
pub(crate) fn assemble_module_outputs(
    modules: &[(String, String, bool)],
    type_contract: Option<&str>,
) -> String {
    let mut all_uses: Vec<String> = Vec::new();
    let mut code_parts: Vec<String> = Vec::new();
    // P27: Track which type names have already been defined
    let mut defined_types: HashSet<String> = HashSet::new();

    // P33: If type contract provided, seed defined_types so P27 dedup strips module redefinitions
    let contract_block = if let Some(contract) = type_contract {
        for cline in contract.lines() {
            let trimmed = cline.trim();
            if let Some(type_name) = extract_definition_name(trimmed) {
                defined_types.insert(type_name);
            }
        }
        format!(
            "// === P33: Type Contract (shared types) ===\n{}\n// === End Type Contract ===\n\n",
            contract
        )
    } else {
        String::new()
    };

    for (mod_name, rust_code, _compiles) in modules {
        // P31: Strip markdown fences before processing
        let clean_code: String = rust_code
            .lines()
            .filter(|line| !line.trim().starts_with("```"))
            .collect::<Vec<&str>>()
            .join("\n");

        let mod_code_lines =
            dedup_module_definitions(&clean_code, &mut all_uses, &mut defined_types);
        // Remove leading/trailing empty lines
        let trimmed_lines = trim_empty_lines(&mod_code_lines);
        if !trimmed_lines.is_empty() {
            code_parts.push(format!(
                "// --- Module: {} ---\n{}",
                mod_name,
                trimmed_lines.join("\n")
            ));
        }
    }

    let mut output = String::new();
    if !all_uses.is_empty() {
        // P31: Merge use statements that share the same base path
        let merged = merge_use_statements(all_uses);
        output.push_str(&merged.join("\n"));
        output.push_str("\n\n");
    }
    // P33: Prepend type contract before module code
    if !contract_block.is_empty() {
        output.push_str(&contract_block);
    }
    output.push_str(&code_parts.join("\n\n"));
    output.push('\n');
    output
}

/// P27: Extract non-duplicate lines from a module, collecting `use` statements
/// and stripping type/struct/enum/const definitions that were already defined
/// by earlier modules.
fn dedup_module_definitions<'a>(
    rust_code: &'a str,
    all_uses: &mut Vec<String>,
    defined_types: &mut HashSet<String>,
) -> Vec<&'a str> {
    let mut result_lines: Vec<&str> = Vec::new();
    let lines: Vec<&str> = rust_code.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Deduplicate use statements
        if trimmed.starts_with("use ") && trimmed.ends_with(';') {
            if !all_uses.contains(&trimmed.to_string()) {
                all_uses.push(trimmed.to_string());
            }
            i += 1;
            continue;
        }

        // Check for type definitions: pub struct/enum/const/type Name
        if let Some(type_name) = extract_definition_name(trimmed) {
            if defined_types.contains(&type_name) {
                // Skip this entire definition (including its body with braces)
                i = skip_braced_block(&lines, i);
                continue;
            }
            defined_types.insert(type_name);
        }

        // Check for impl blocks: impl TypeName / impl Trait for TypeName
        if let Some(impl_target) = extract_impl_target(trimmed) {
            // If the type isn't defined yet (was stripped), skip the impl too
            // But only skip if we've seen this type before AND it was from another module
            // (i.e., the type was stripped from this module)
            if !defined_types.contains(&impl_target) && trimmed.contains("impl ") {
                // Type not defined anywhere yet — keep the impl, it defines behavior
                result_lines.push(lines[i]);
                i += 1;
                continue;
            }
        }

        result_lines.push(lines[i]);
        i += 1;
    }
    result_lines
}

/// Extract the name from a type definition line (struct, enum, const, type).
/// Returns None if the line is not a definition.
fn extract_definition_name(line: &str) -> Option<String> {
    // Match patterns like: pub struct Foo { / pub enum Bar { / const X: ...
    let prefixes = [
        "pub struct ",
        "struct ",
        "pub enum ",
        "enum ",
        "pub const ",
        "const ",
        "pub type ",
        "type ",
    ];
    for prefix in &prefixes {
        if let Some(rest) = line.strip_prefix(prefix) {
            // Extract the name (up to first non-alphanumeric/underscore)
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// Extract the target type from an `impl` line.
/// `impl Foo {` -> Some("Foo"), `impl Display for Foo {` -> Some("Foo")
fn extract_impl_target(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("impl ") {
        return None;
    }
    let rest = &trimmed[5..]; // after "impl "
    // Check for "Trait for Type" pattern
    if let Some(for_pos) = rest.find(" for ") {
        let after_for = &rest[for_pos + 5..];
        let name: String = after_for
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    // Direct impl: "impl Type {"
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if !name.is_empty() { Some(name) } else { None }
}

/// Skip a braced block starting at line `start`. Returns the index after the closing brace.
fn skip_braced_block(lines: &[&str], start: usize) -> usize {
    let mut depth = 0i32;
    let mut i = start;
    // Count braces on the first line
    for ch in lines[start].chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    i += 1;
    // If the definition had no opening brace on this line (e.g., `const X: i32 = 5;`)
    // it's a single-line definition — already skipped
    if depth <= 0 {
        return i;
    }
    // Track braces until balanced
    while i < lines.len() && depth > 0 {
        for ch in lines[i].chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    i
}

/// Trim leading and trailing empty lines from a slice.
fn trim_empty_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let mut result: Vec<&str> = lines.to_vec();
    while result.first().is_some_and(|l| l.trim().is_empty()) {
        result.remove(0);
    }
    while result.last().is_some_and(|l| l.trim().is_empty()) {
        result.pop();
    }
    result
}

/// Merge `use` statements that share the same base path.
///
/// Groups `use std::io::{Read, Write};` and `use std::io::{self, Seek};`
/// into `use std::io::{self, Read, Seek, Write};`.
/// Simple `use foo::Bar;` are kept as-is (deduplicated by exact match).
pub(crate) fn merge_use_statements(uses: Vec<String>) -> Vec<String> {
    let brace_re = regex::Regex::new(r"^use\s+(?P<path>[^{;]+)::\{(?P<items>[^}]+)\};$")
        .expect("static regex");
    let simple_re = regex::Regex::new(r"^use\s+(?P<full>[^{]+);$").expect("static regex");

    // path -> set of items
    let mut groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut simple_uses: BTreeSet<String> = BTreeSet::new();

    for u in &uses {
        let trimmed = u.trim();
        if let Some(caps) = brace_re.captures(trimmed) {
            let path = caps["path"].trim().to_string();
            let items: Vec<String> = caps["items"]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let entry = groups.entry(path).or_default();
            for item in items {
                entry.insert(item);
            }
        } else if simple_re.is_match(trimmed) {
            simple_uses.insert(trimmed.to_string());
        }
    }

    let mut result: Vec<String> = Vec::new();

    // Emit merged brace imports
    for (path, items) in &groups {
        let sorted: Vec<&String> = {
            let mut v: Vec<&String> = items.iter().collect();
            // Put `self` first if present
            v.sort_by(|a, b| {
                if a.as_str() == "self" {
                    std::cmp::Ordering::Less
                } else if b.as_str() == "self" {
                    std::cmp::Ordering::Greater
                } else {
                    a.cmp(b)
                }
            });
            v
        };
        let items_str = sorted
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<&str>>()
            .join(", ");
        result.push(format!("use {path}::{{{items_str}}};"));
    }

    // Emit simple imports (but skip if already covered by a brace import)
    for s in &simple_uses {
        let covered = groups.iter().any(|(path, items)| {
            if let Some(rest) = s.strip_prefix(&format!("use {path}::")) {
                let name = rest.trim_end_matches(';').trim();
                items.contains(name)
            } else {
                false
            }
        });
        if !covered {
            result.push(s.clone());
        }
    }

    result.sort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assemble_module_outputs_dedup_uses() {
        let modules = vec![
            (
                "utils".to_string(),
                "use std::collections::HashMap;\n\nfn util_a() -> i32 { 1 }\n".to_string(),
                true,
            ),
            (
                "core".to_string(),
                "use std::collections::HashMap;\nuse std::io;\n\nfn core_b() -> i32 { 2 }\n"
                    .to_string(),
                true,
            ),
        ];
        let result = assemble_module_outputs(&modules, None);
        assert_eq!(
            result.matches("use std::collections::HashMap;").count(),
            1,
            "HashMap use should be deduplicated"
        );
        assert!(result.contains("use std::io;"));
        assert!(result.contains("fn util_a()"));
        assert!(result.contains("fn core_b()"));
        assert!(result.contains("// --- Module: utils ---"));
        assert!(result.contains("// --- Module: core ---"));
    }

    #[test]
    fn test_assemble_module_outputs_empty() {
        let modules: Vec<(String, String, bool)> = vec![];
        let result = assemble_module_outputs(&modules, None);
        assert_eq!(result.trim(), "");
    }

    #[test]
    fn test_assemble_strips_markdown_fences() {
        let modules = vec![
            (
                "mod_a".to_string(),
                "use std::io;\n\nfn foo() -> i32 { 1 }".to_string(),
                true,
            ),
            (
                "mod_b".to_string(),
                "```rust\nfn bar() -> i32 { 2 }\n```".to_string(),
                false,
            ),
        ];
        let assembled = assemble_module_outputs(&modules, None);
        assert!(
            !assembled.contains("```"),
            "fences should be stripped from assembly:\n{assembled}"
        );
        assert!(assembled.contains("fn foo()"));
        assert!(assembled.contains("fn bar()"));
    }

    #[test]
    fn test_assemble_merges_use_imports() {
        let modules = vec![
            (
                "a".to_string(),
                "use std::io::{self, Read};\nfn a() {}".to_string(),
                true,
            ),
            (
                "b".to_string(),
                "use std::io::{self, Read, Seek, SeekFrom};\nfn b() {}".to_string(),
                true,
            ),
            (
                "c".to_string(),
                "use std::io::{self, Write, Seek, SeekFrom};\nfn c() {}".to_string(),
                true,
            ),
        ];
        let assembled = assemble_module_outputs(&modules, None);

        let io_lines: Vec<&str> = assembled
            .lines()
            .filter(|l| l.contains("use std::io"))
            .collect();
        assert_eq!(
            io_lines.len(),
            1,
            "should merge into one use std::io line, got: {io_lines:?}"
        );

        let io_line = io_lines[0];
        for item in &["Read", "Seek", "SeekFrom", "Write", "self"] {
            assert!(
                io_line.contains(item),
                "merged import should contain {item}: {io_line}"
            );
        }
    }

    #[test]
    fn test_assemble_truncated_module_auto_closed() {
        let modules = vec![
            (
                "mod_a".to_string(),
                "use std::io;\n\nfn foo() {\n    1\n}".to_string(),
                true,
            ),
            (
                "mod_b".to_string(),
                "fn bar() {\n    if true {\n        let x = 1;".to_string(),
                false,
            ),
            (
                "mod_c".to_string(),
                "fn baz() {\n    2\n}".to_string(),
                true,
            ),
        ];
        let result = assemble_module_outputs(&modules, None);
        assert!(result.contains("fn foo()"), "mod_a present");
        assert!(result.contains("fn bar()"), "mod_b present");
        assert!(result.contains("fn baz()"), "mod_c present");
    }

    #[test]
    fn test_assemble_module_outputs_preserves_order() {
        let modules = vec![
            ("first".to_string(), "fn a() {}\n".to_string(), true),
            ("second".to_string(), "fn b() {}\n".to_string(), true),
            ("third".to_string(), "fn c() {}\n".to_string(), true),
        ];
        let result = assemble_module_outputs(&modules, None);
        let pos_a = result.find("Module: first").unwrap();
        let pos_b = result.find("Module: second").unwrap();
        let pos_c = result.find("Module: third").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_assemble_dedup_types_across_modules() {
        let mod_a = "pub struct ZipArchive {\n    pub data: Vec<u8>,\n}\n\nfn read(a: &ZipArchive) -> usize { a.data.len() }\n".to_string();
        let mod_b = "pub struct ZipArchive {\n    pub data: Vec<u8>,\n}\n\nfn write(a: &mut ZipArchive) { a.data.push(0); }\n".to_string();
        let modules = vec![
            ("types".to_string(), mod_a, true),
            ("ops".to_string(), mod_b, true),
        ];
        let result = assemble_module_outputs(&modules, None);
        assert_eq!(
            result.matches("pub struct ZipArchive").count(),
            1,
            "ZipArchive should be deduplicated: {result}"
        );
        assert!(result.contains("fn read("), "fn read should survive dedup");
        assert!(
            result.contains("fn write("),
            "fn write should survive dedup"
        );
    }

    #[test]
    fn test_assemble_dedup_enum_and_const() {
        let mod_a = "pub enum ZipError {\n    Io,\n    Parse,\n}\n\npub const HEADER_SIZE: u32 = 30;\n\nfn init() -> i32 { 0 }\n".to_string();
        let mod_b = "pub enum ZipError {\n    Io,\n    Parse,\n}\n\npub const HEADER_SIZE: u32 = 30;\n\nfn process() -> i32 { 1 }\n".to_string();
        let modules = vec![
            ("base".to_string(), mod_a, true),
            ("ext".to_string(), mod_b, true),
        ];
        let result = assemble_module_outputs(&modules, None);
        assert_eq!(
            result.matches("pub enum ZipError").count(),
            1,
            "enum should be deduped"
        );
        assert_eq!(
            result.matches("pub const HEADER_SIZE").count(),
            1,
            "const should be deduped"
        );
        assert!(result.contains("fn init()"));
        assert!(result.contains("fn process()"));
    }

    #[test]
    fn test_assemble_dedup_preserves_first_definition() {
        let mod_a = "pub struct ZipArchive {\n    pub data: Vec<u8>,\n    pub name: String,\n}\n"
            .to_string();
        let mod_b = "pub struct ZipArchive {\n    pub data: Vec<u8>,\n    pub name: String,\n    pub extra: bool,\n}\n\nfn check() -> bool { true }\n".to_string();
        let modules = vec![
            ("first".to_string(), mod_a, true),
            ("second".to_string(), mod_b, true),
        ];
        let result = assemble_module_outputs(&modules, None);
        assert_eq!(result.matches("pub struct ZipArchive").count(), 1);
        assert!(
            result.contains("pub name: String"),
            "first def fields should be present"
        );
        assert!(
            !result.contains("pub extra: bool"),
            "second def fields should be stripped"
        );
        assert!(result.contains("fn check()"), "functions should survive");
    }

    #[test]
    fn test_assemble_with_type_contract() {
        let contract = "pub struct Foo { pub x: i32 }\npub enum Bar { A, B }\n";
        let modules = vec![
            (
                "mod1".to_string(),
                "pub fn create_foo() -> Foo { Foo { x: 1 } }".to_string(),
                true,
            ),
            (
                "mod2".to_string(),
                "pub fn get_bar() -> Bar { Bar::A }".to_string(),
                true,
            ),
        ];
        let result = assemble_module_outputs(&modules, Some(contract));
        let contract_pos = result.find("pub struct Foo").unwrap();
        let fn_pos = result.find("pub fn create_foo").unwrap();
        assert!(
            contract_pos < fn_pos,
            "Type contract should precede module code"
        );
        assert!(result.contains("pub fn get_bar"));
        assert!(
            result.contains("P33: Type Contract"),
            "should contain contract header"
        );
    }

    #[test]
    fn test_assemble_with_type_contract_dedup() {
        let contract = "pub struct Foo { pub x: i32 }\n";
        let modules = vec![(
            "mod1".to_string(),
            "pub struct Foo { pub x: i32 }\npub fn use_foo(f: &Foo) -> i32 { f.x }".to_string(),
            true,
        )];
        let result = assemble_module_outputs(&modules, Some(contract));
        assert_eq!(
            result.matches("pub struct Foo").count(),
            1,
            "contract types should not be duplicated by module redefinitions: {result}"
        );
        assert!(result.contains("pub fn use_foo"));
    }

    #[test]
    fn test_assemble_without_type_contract() {
        let modules = vec![
            (
                "a".to_string(),
                "pub struct X { pub v: i32 }\nfn a() {}".to_string(),
                true,
            ),
            (
                "b".to_string(),
                "pub struct X { pub v: i32 }\nfn b() {}".to_string(),
                true,
            ),
        ];
        let result = assemble_module_outputs(&modules, None);
        assert!(!result.contains("P33: Type Contract"));
        assert_eq!(
            result.matches("pub struct X").count(),
            1,
            "P27 dedup should still work without contract"
        );
    }

    #[test]
    fn test_assembly_validation_catches_cross_module_deps() {
        let module_a = "pub struct ZipArchive { pub data: Vec<u8> }\n";
        let module_b = "fn read_archive(a: &ZipArchive) -> usize { a.data.len() }\n";

        let result_alone = noricum_tools::compiler::check_rust_compiles(module_b).unwrap();
        assert!(!result_alone.success, "module B alone should not compile");

        let combined = format!("{module_a}\n{module_b}");
        let result_with_context = noricum_tools::compiler::check_rust_compiles(&combined).unwrap();
        assert!(
            result_with_context.success,
            "module B with A should compile"
        );

        let outputs = vec![("mod_a".to_string(), module_a.to_string(), true)];
        let ctx = build_assembly_context(&outputs);
        assert!(ctx.contains("ZipArchive"));

        let outputs_with_broken = vec![
            ("mod_a".to_string(), module_a.to_string(), true),
            ("mod_broken".to_string(), "fn broken( {".to_string(), false),
        ];
        let ctx = build_assembly_context(&outputs_with_broken);
        assert!(
            ctx.contains("ZipArchive"),
            "compiling module should be included"
        );
        assert!(
            !ctx.contains("broken"),
            "non-compiling module should be filtered"
        );
    }

    #[test]
    fn test_skip_braced_block_basic() {
        let lines = vec!["pub struct Foo {", "    x: i32,", "}", "fn bar() {}"];
        let end = skip_braced_block(&lines, 0);
        assert_eq!(end, 3, "should skip past closing brace");
    }

    #[test]
    fn test_extract_definition_name() {
        assert_eq!(
            extract_definition_name("pub struct ZipArchive {"),
            Some("ZipArchive".to_string())
        );
        assert_eq!(
            extract_definition_name("pub enum ZipError {"),
            Some("ZipError".to_string())
        );
        assert_eq!(
            extract_definition_name("pub const HEADER: u32 = 30;"),
            Some("HEADER".to_string())
        );
        assert_eq!(
            extract_definition_name("pub type Result = std::result::Result;"),
            Some("Result".to_string())
        );
        assert_eq!(extract_definition_name("fn foo() {}"), None);
        assert_eq!(extract_definition_name("let x = 5;"), None);
    }
}
