//! Integration tests for the Noricum project.
//!
//! These tests verify end-to-end behavior of the CLI, analysis pipeline,
//! and differential testing.

use std::process::Command;

/// Test that the `noricum` CLI binary can be built and run with `--help`.
#[test]
fn test_cli_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "noricum-cli", "--bin", "noricum", "--", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run noricum --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "noricum --help should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Autonomous C/C++ to Rust migration agent"),
        "help output should contain description, got: {stdout}"
    );
    assert!(
        stdout.contains("migrate"),
        "help output should list migrate subcommand"
    );
    assert!(
        stdout.contains("analyze"),
        "help output should list analyze subcommand"
    );
}

/// Test that `noricum analyze tests/fixtures/simple/add.c` returns "Easy".
#[test]
fn test_analyze_add_c() {
    let output = Command::new("cargo")
        .args([
            "run",
            "-p", "noricum-cli",
            "--bin", "noricum",
            "--",
            "analyze",
            "tests/fixtures/simple/add.c",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run noricum analyze");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "analyze should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Easy"),
        "add.c should be classified as Easy, got: {stdout}"
    );
}

/// Test differential testing with add.c and a known-good Rust translation.
#[test]
fn test_diff_test_add_function() {
    let c_source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/simple/add.c"),
    )
    .expect("failed to read add.c");

    let rust_source = r#"
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {
    println!("{}", add(2, 3));
    println!("{}", add(-1, 1));
    println!("{}", add(0, 0));
}
"#;

    let result = noricum_tools::diff_test::run_diff_test(&c_source, rust_source)
        .expect("diff_test should not error");

    assert!(result.c_compiled, "C source should compile");
    assert!(result.rust_compiled, "Rust source should compile");
    assert!(
        result.passed,
        "outputs should match: C={:?}, Rust={:?}",
        result.c_output,
        result.rust_output
    );
    assert_eq!(result.c_output, "5\n0\n0\n");
}

/// Test differential testing detects a mismatch.
#[test]
fn test_diff_test_mismatch() {
    let c_source = r#"
#include <stdio.h>
int main(void) {
    printf("42\n");
    return 0;
}
"#;
    let rust_source = r#"
fn main() {
    println!("99");
}
"#;

    let result = noricum_tools::diff_test::run_diff_test(c_source, rust_source)
        .expect("diff_test should not error");

    assert!(result.c_compiled);
    assert!(result.rust_compiled);
    assert!(!result.passed, "mismatched outputs should fail");
}
