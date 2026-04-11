//! Cargo crate builder for multi-file Rust output.
//!
//! Generates a proper Cargo crate from modular migration outputs:
//! `Cargo.toml`, `src/lib.rs`, `src/types.rs`, and per-module `src/{name}.rs`.

use std::path::{Path, PathBuf};

use tracing::{debug, info};

use crate::ToolError;

/// Rust reserved keywords that need raw identifier syntax in `pub mod` declarations.
const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "yield",
];

/// Manages the output directory layout for a generated Rust crate.
///
/// ```text
/// {output_dir}/
/// +-- Cargo.toml
/// +-- src/
///     +-- lib.rs          (pub mod declarations)
///     +-- types.rs         (shared type contract)
///     +-- {module_a}.rs
///     +-- {module_b}.rs
///     +-- main.rs          (optional, for diff test)
/// ```
#[derive(Debug)]
pub struct CrateBuilder {
    /// Root directory of the generated crate.
    output_dir: PathBuf,
    /// Crate name (derived from C source filename, sanitized for Cargo).
    crate_name: String,
    /// Module names that have been written (in insertion order for lib.rs).
    modules: Vec<String>,
    /// Whether a types.rs has been written.
    has_types: bool,
}

impl CrateBuilder {
    /// Create a new CrateBuilder, initializing the directory structure.
    ///
    /// Creates `{output_dir}/src/` if it does not exist.
    pub fn new(output_dir: &Path, crate_name: &str) -> Result<Self, ToolError> {
        let sanitized = sanitize_crate_name(crate_name);
        let src_dir = output_dir.join("src");
        std::fs::create_dir_all(&src_dir)?;
        debug!(output_dir = %output_dir.display(), crate_name = %sanitized, "CrateBuilder initialized");
        Ok(Self {
            output_dir: output_dir.to_path_buf(),
            crate_name: sanitized,
            modules: Vec::new(),
            has_types: false,
        })
    }

    /// Return the root path of the output crate.
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    /// Return the crate name.
    pub fn crate_name(&self) -> &str {
        &self.crate_name
    }

    /// Return the list of module names written so far.
    pub fn modules(&self) -> &[String] {
        &self.modules
    }

    /// Write the shared type contract to `src/types.rs`.
    ///
    /// This file contains all shared struct/enum/const definitions that
    /// modules import via `use crate::types::*;`.
    pub fn write_types(&mut self, type_contract: &str) -> Result<(), ToolError> {
        let path = self.output_dir.join("src/types.rs");
        std::fs::write(&path, type_contract)?;
        self.has_types = true;
        info!(path = %path.display(), lines = type_contract.lines().count(), "wrote types.rs");
        Ok(())
    }

    /// Write a module's Rust source to `src/{module_name}.rs`.
    ///
    /// Automatically prepends `use crate::types::*;` if types.rs exists.
    /// The module name is sanitized (lowercase, underscores only).
    pub fn write_module(
        &mut self,
        module_name: &str,
        rust_source: &str,
    ) -> Result<PathBuf, ToolError> {
        let safe_name = sanitize_module_name(module_name);
        let path = self.output_dir.join(format!("src/{safe_name}.rs"));

        let content = if self.has_types {
            format!(
                "#![allow(unused_imports)]\nuse crate::types::*;\n\n{}",
                rust_source
            )
        } else {
            rust_source.to_string()
        };

        std::fs::write(&path, &content)?;
        if !self.modules.contains(&safe_name) {
            self.modules.push(safe_name.clone());
        }
        debug!(module = %safe_name, path = %path.display(), "wrote module");
        Ok(path)
    }

    /// Read a module's current source from `src/{module_name}.rs`.
    pub fn read_module(&self, module_name: &str) -> Result<String, ToolError> {
        let safe_name = sanitize_module_name(module_name);
        let path = self.output_dir.join(format!("src/{safe_name}.rs"));
        Ok(std::fs::read_to_string(&path)?)
    }

    /// Generate and write `src/lib.rs` with `pub mod` declarations for all modules.
    pub fn write_lib_rs(&self) -> Result<(), ToolError> {
        let path = self.output_dir.join("src/lib.rs");
        let mut content = String::new();

        if self.has_types {
            content.push_str("pub mod types;\n");
        }

        for module in &self.modules {
            // Use raw identifier syntax for Rust keywords
            if RUST_KEYWORDS.contains(&module.as_str()) {
                content.push_str(&format!("pub mod r#{module};\n"));
            } else {
                content.push_str(&format!("pub mod {module};\n"));
            }
        }

        std::fs::write(&path, &content)?;
        info!(path = %path.display(), modules = self.modules.len(), "wrote lib.rs");
        Ok(())
    }

    /// Generate and write `Cargo.toml`.
    pub fn write_cargo_toml(&self) -> Result<(), ToolError> {
        let path = self.output_dir.join("Cargo.toml");
        let content = format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[lib]
name = "{name}"
path = "src/lib.rs"

[[bin]]
name = "{name}"
path = "src/main.rs"
required-features = ["bin"]

[features]
bin = []
"#,
            name = self.crate_name,
        );
        std::fs::write(&path, &content)?;
        info!(path = %path.display(), "wrote Cargo.toml");
        Ok(())
    }

    /// Write `src/main.rs` for diff testing.
    ///
    /// The main function must be provided as a string -- typically extracted
    /// from the C source's `main()` translation.
    pub fn write_main(&self, main_source: &str) -> Result<(), ToolError> {
        let path = self.output_dir.join("src/main.rs");

        let mut content = String::new();
        if self.has_types {
            content.push_str("use crate::types::*;\n");
        }
        for module in &self.modules {
            content.push_str(&format!(
                "use {crate_name}::{module}::*;\n",
                crate_name = self.crate_name
            ));
        }
        content.push('\n');
        content.push_str(main_source);

        std::fs::write(&path, &content)?;
        info!(path = %path.display(), "wrote main.rs");
        Ok(())
    }

    /// Return the path to the Cargo.toml.
    pub fn cargo_toml_path(&self) -> PathBuf {
        self.output_dir.join("Cargo.toml")
    }

    /// Return the path to a module's source file.
    pub fn module_path(&self, module_name: &str) -> PathBuf {
        let safe_name = sanitize_module_name(module_name);
        self.output_dir.join(format!("src/{safe_name}.rs"))
    }

    /// Collect all module sources into a single assembled string (for backward compatibility).
    ///
    /// This enables gradual migration: code that expects a single string can still work.
    pub fn assemble_all(&self) -> Result<String, ToolError> {
        let mut combined = String::new();
        if self.has_types {
            let types = std::fs::read_to_string(self.output_dir.join("src/types.rs"))?;
            combined.push_str(&types);
            combined.push_str("\n\n");
        }
        for module in &self.modules {
            let source = self.read_module(module)?;
            // Strip the `use crate::types::*;` line since we're assembling flat
            let stripped: String = source
                .lines()
                .filter(|l| !l.trim().starts_with("use crate::types::"))
                .filter(|l| l.trim() != "#![allow(unused_imports)]")
                .collect::<Vec<&str>>()
                .join("\n");
            combined.push_str(&format!("// --- Module: {} ---\n", module));
            combined.push_str(&stripped);
            combined.push_str("\n\n");
        }
        Ok(combined)
    }
}

/// Sanitize a string into a valid Cargo crate name.
///
/// Rules: lowercase, replace non-alphanumeric with `_`, must start with letter.
fn sanitize_crate_name(name: &str) -> String {
    let mut result: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    // Cargo crate names cannot start with a digit
    if result.starts_with(|c: char| c.is_ascii_digit()) {
        result = format!("crate_{result}");
    }
    if result.is_empty() {
        result = "output".to_string();
    }
    result
}

/// Sanitize a module name for use as a Rust module identifier.
///
/// Rules: lowercase, replace non-alphanumeric with `_`, must start with letter/underscore.
fn sanitize_module_name(name: &str) -> String {
    let mut result: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if result.starts_with(|c: char| c.is_ascii_digit()) {
        result = format!("mod_{result}");
    }
    if result.is_empty() {
        result = "unnamed".to_string();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_crate_name() {
        assert_eq!(sanitize_crate_name("hash_table"), "hash_table");
        assert_eq!(sanitize_crate_name("miniz_zip.c"), "miniz_zip_c");
        assert_eq!(sanitize_crate_name("CamelCase"), "camelcase");
        assert_eq!(
            sanitize_crate_name("123starts_with_digit"),
            "crate_123starts_with_digit"
        );
        assert_eq!(sanitize_crate_name(""), "output");
    }

    #[test]
    fn test_sanitize_module_name() {
        assert_eq!(sanitize_module_name("mz_p1"), "mz_p1");
        assert_eq!(sanitize_module_name("if"), "if"); // reserved word, but valid file name
        assert_eq!(sanitize_module_name("CamelCase"), "camelcase");
    }

    #[test]
    fn test_crate_builder_basic() {
        let tmp = tempfile::tempdir().unwrap();
        let mut builder = CrateBuilder::new(tmp.path(), "test_crate").unwrap();

        // Write types
        builder
            .write_types("pub struct Foo { pub x: i32 }")
            .unwrap();
        assert!(builder.has_types);

        // Write module
        builder
            .write_module("utils", "pub fn add(a: i32, b: i32) -> i32 { a + b }")
            .unwrap();
        assert_eq!(builder.modules(), &["utils"]);

        // Read module back
        let source = builder.read_module("utils").unwrap();
        assert!(source.contains("use crate::types::*;"));
        assert!(source.contains("pub fn add"));

        // Write lib.rs
        builder.write_lib_rs().unwrap();
        let lib_rs = std::fs::read_to_string(tmp.path().join("src/lib.rs")).unwrap();
        assert!(lib_rs.contains("pub mod types;"));
        assert!(lib_rs.contains("pub mod utils;"));

        // Write Cargo.toml
        builder.write_cargo_toml().unwrap();
        let cargo_toml = std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
        assert!(cargo_toml.contains("name = \"test_crate\""));
        assert!(cargo_toml.contains("edition = \"2024\""));
    }

    #[test]
    fn test_crate_builder_no_types() {
        let tmp = tempfile::tempdir().unwrap();
        let mut builder = CrateBuilder::new(tmp.path(), "no_types").unwrap();

        builder
            .write_module("core", "pub fn run() -> i32 { 42 }")
            .unwrap();
        let source = builder.read_module("core").unwrap();
        // Without types.rs, no types import should be added
        assert!(!source.contains("use crate::types::*;"));
    }

    #[test]
    fn test_crate_builder_assemble_all() {
        let tmp = tempfile::tempdir().unwrap();
        let mut builder = CrateBuilder::new(tmp.path(), "asm_test").unwrap();

        builder.write_types("pub struct S { pub v: i32 }").unwrap();
        builder
            .write_module("a", "pub fn a() -> i32 { 1 }")
            .unwrap();
        builder
            .write_module("b", "pub fn b() -> i32 { 2 }")
            .unwrap();

        let assembled = builder.assemble_all().unwrap();
        assert!(assembled.contains("pub struct S"));
        assert!(assembled.contains("pub fn a()"));
        assert!(assembled.contains("pub fn b()"));
        // Should NOT contain crate::types imports in assembled output
        assert!(!assembled.contains("use crate::types"));
    }

    #[test]
    fn test_crate_builder_module_dedup() {
        let tmp = tempfile::tempdir().unwrap();
        let mut builder = CrateBuilder::new(tmp.path(), "dedup_test").unwrap();

        builder.write_module("a", "fn x() {}").unwrap();
        builder.write_module("a", "fn y() {}").unwrap(); // overwrite, not duplicate

        assert_eq!(builder.modules().len(), 1);
        let source = builder.read_module("a").unwrap();
        assert!(source.contains("fn y()"));
    }

    #[test]
    fn test_crate_builder_keyword_module() {
        let tmp = tempfile::tempdir().unwrap();
        let mut builder = CrateBuilder::new(tmp.path(), "keyword_test").unwrap();

        builder
            .write_module("if", "pub fn check() -> bool { true }")
            .unwrap();
        builder.write_lib_rs().unwrap();

        let lib_rs = std::fs::read_to_string(tmp.path().join("src/lib.rs")).unwrap();
        assert!(
            lib_rs.contains("pub mod r#if;"),
            "keyword module should use raw identifier: {lib_rs}"
        );
    }
}
