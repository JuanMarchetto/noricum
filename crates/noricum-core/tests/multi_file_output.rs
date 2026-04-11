//! Integration test: verify multi-file crate output with a known fixture.
//!
//! Uses CrateBuilder + check_crate_compiles to verify the
//! multi-file output pipeline produces compilable Rust crates.

use noricum_tools::compiler::check_crate_compiles;
use noricum_tools::crate_builder::CrateBuilder;

/// Test that CrateBuilder produces a compilable crate from pre-translated modules.
#[test]
fn test_crate_builder_compiles_simple_module() {
    let tmp = tempfile::tempdir().unwrap();
    let mut builder = CrateBuilder::new(tmp.path(), "simple_test").unwrap();

    builder
        .write_types(
            "/// A simple key-value pair.\n\
             pub struct Entry {\n\
                 pub key: String,\n\
                 pub value: i32,\n\
             }\n",
        )
        .unwrap();

    builder
        .write_module(
            "ops",
            "pub fn create(key: &str, value: i32) -> Entry {\n\
                 Entry { key: key.to_string(), value }\n\
             }\n\
             pub fn lookup(entries: &[Entry], key: &str) -> Option<i32> {\n\
                 entries.iter().find(|e| e.key == key).map(|e| e.value)\n\
             }\n",
        )
        .unwrap();

    builder.write_lib_rs().unwrap();
    builder.write_cargo_toml().unwrap();

    let result = check_crate_compiles(tmp.path()).unwrap();
    assert!(
        result.success,
        "generated crate should compile: {}",
        result.stderr
    );
}

/// Test multi-module crate with cross-module dependencies.
#[test]
fn test_crate_builder_cross_module_deps() {
    let tmp = tempfile::tempdir().unwrap();
    let mut builder = CrateBuilder::new(tmp.path(), "cross_dep_test").unwrap();

    builder
        .write_types(
            "pub struct Point { pub x: f64, pub y: f64 }\n\
             pub struct Rect { pub origin: Point, pub size: Point }\n",
        )
        .unwrap();

    builder
        .write_module(
            "geometry",
            "pub fn distance(a: &Point, b: &Point) -> f64 {\n\
                 ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()\n\
             }\n",
        )
        .unwrap();

    builder
        .write_module(
            "shapes",
            "use crate::geometry;\n\n\
             pub fn rect_diagonal(r: &Rect) -> f64 {\n\
                 let corner = Point { x: r.origin.x + r.size.x, y: r.origin.y + r.size.y };\n\
                 geometry::distance(&r.origin, &corner)\n\
             }\n",
        )
        .unwrap();

    builder.write_lib_rs().unwrap();
    builder.write_cargo_toml().unwrap();

    let result = check_crate_compiles(tmp.path()).unwrap();
    assert!(
        result.success,
        "cross-module crate should compile: {}",
        result.stderr
    );
}

/// Test that assemble_all() produces backward-compatible single-file output.
#[test]
fn test_crate_builder_backward_compat_assembly() {
    let tmp = tempfile::tempdir().unwrap();
    let mut builder = CrateBuilder::new(tmp.path(), "compat_test").unwrap();

    builder
        .write_types("pub struct Foo { pub x: i32 }")
        .unwrap();
    builder
        .write_module("a", "pub fn a(f: &Foo) -> i32 { f.x }")
        .unwrap();
    builder
        .write_module("b", "pub fn b() -> Foo { Foo { x: 42 } }")
        .unwrap();
    builder.write_lib_rs().unwrap();
    builder.write_cargo_toml().unwrap();

    // The assembled single-file should compile with rustc
    let assembled = builder.assemble_all().unwrap();
    let result = noricum_tools::compiler::check_rust_compiles(&assembled).unwrap();
    assert!(
        result.success,
        "assembled single-file should compile: {}",
        result.stderr
    );
}
