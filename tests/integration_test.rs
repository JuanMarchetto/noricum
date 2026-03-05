//! Integration tests for the Noricum project.
//!
//! These tests verify end-to-end behavior of the CLI, analysis pipeline,
//! and differential testing.

use std::process::Command;

fn noricum_cmd() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args(["run", "-p", "noricum-cli", "--bin", "noricum", "--"])
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    cmd
}

/// Test that the `noricum` CLI binary can be built and run with `--help`.
#[test]
fn test_cli_help() {
    let output = noricum_cmd()
        .arg("--help")
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
    let output = noricum_cmd()
        .args(["analyze", "tests/fixtures/simple/add.c"])
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

/// Test sync migration (--no-llm) of a simple file.
#[test]
fn test_migrate_sync_simple() {
    let tmp = tempfile::tempdir().unwrap();
    let output = noricum_cmd()
        .args([
            "migrate",
            "tests/fixtures/simple/add.c",
            "--no-llm",
            "--output",
        ])
        .arg(tmp.path())
        .output()
        .expect("failed to run noricum migrate --no-llm");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "migrate --no-llm should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Validated"), "should reach Validated state");
    assert!(
        stdout.contains("fn add"),
        "output should contain translated function"
    );

    // Check that the .rs file was written
    let output_file = tmp.path().join("add.rs");
    assert!(
        output_file.exists(),
        "add.rs should be written to output dir"
    );
    let content = std::fs::read_to_string(&output_file).unwrap();
    assert!(content.contains("fn add"));
}

/// Test sync migration with JSON output.
#[test]
fn test_migrate_sync_json() {
    let tmp = tempfile::tempdir().unwrap();
    let output = noricum_cmd()
        .args([
            "migrate",
            "tests/fixtures/simple/add.c",
            "--no-llm",
            "--json",
            "--output",
        ])
        .arg(tmp.path())
        .output()
        .expect("failed to run noricum migrate --json");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "migrate --json should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // JSON output should be parseable
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    assert_eq!(parsed["name"], "add");
    assert_eq!(parsed["state"], "Validated");
    assert!(parsed["rust_output"].is_string());
}

/// Test sync migration of a directory.
#[test]
fn test_migrate_sync_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let output = noricum_cmd()
        .args(["migrate", "tests/fixtures/simple", "--no-llm", "--output"])
        .arg(tmp.path())
        .output()
        .expect("failed to run noricum migrate directory");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "migrate directory should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Migration complete"),
        "should show migration summary"
    );
    assert!(
        stdout.contains("Total functions"),
        "should show total function count"
    );
}

/// Test sync migration with --diff-test flag.
#[test]
fn test_migrate_sync_with_diff_test() {
    let tmp = tempfile::tempdir().unwrap();
    let output = noricum_cmd()
        .args([
            "migrate",
            "tests/fixtures/simple/add.c",
            "--no-llm",
            "--diff-test",
            "--output",
        ])
        .arg(tmp.path())
        .output()
        .expect("failed to run noricum migrate --diff-test");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "migrate --diff-test should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Differential Test"),
        "should show diff test section"
    );
    assert!(
        stdout.contains("PASSED") || stdout.contains("FAILED"),
        "should show diff test result"
    );
}

/// Test that analyzing a complex file with pointers reports Medium/Hard difficulty.
#[test]
fn test_analyze_complex_difficulty() {
    // Write a temporary complex C file
    let tmp = tempfile::tempdir().unwrap();
    let c_file = tmp.path().join("complex.c");
    std::fs::write(
        &c_file,
        r#"
void* generic_alloc(void *ctx, size_t size) {
    void *ptr = malloc(size);
    if (!ptr) return NULL;
    memset(ptr, 0, size);
    return ptr;
}
"#,
    )
    .unwrap();

    let output = noricum_cmd()
        .args(["analyze"])
        .arg(&c_file)
        .output()
        .expect("failed to run noricum analyze");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.contains("Hard"),
        "void* function should be classified as Hard, got: {stdout}"
    );
}

/// Test doctor command outputs expected checks.
#[test]
fn test_doctor_output() {
    let output = noricum_cmd()
        .arg("doctor")
        .output()
        .expect("failed to run noricum doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("C compiler"));
    assert!(stdout.contains("Rust compiler"));
    assert!(stdout.contains("ANTHROPIC_API_KEY"));
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
        result.c_output, result.rust_output
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

/// Test that --report flag generates an HTML file.
#[test]
fn test_migrate_sync_with_report() {
    let tmp = tempfile::tempdir().unwrap();
    let report_path = tmp.path().join("report.html");
    let output = noricum_cmd()
        .args([
            "migrate",
            "tests/fixtures/simple/add.c",
            "--no-llm",
            "--report",
        ])
        .arg(&report_path)
        .arg("--output")
        .arg(tmp.path())
        .output()
        .expect("failed to run noricum migrate --report");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "migrate --report should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("HTML report"),
        "should mention HTML report in output"
    );
    assert!(report_path.exists(), "HTML report file should exist");
    let html = std::fs::read_to_string(&report_path).unwrap();
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("Noricum Migration Report"));
}

/// Test that hash_table.c is properly classified as complex (needs LLM).
#[test]
fn test_analyze_hash_table() {
    let output = noricum_cmd()
        .args(["analyze", "tests/fixtures/medium/hash_table.c"])
        .output()
        .expect("failed to run noricum analyze");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.contains("Hard"),
        "hash_table.c should be classified as Hard, got: {stdout}"
    );
}

/// Test that miniz_test.c compiles as C (sanity check for diff testing).
#[test]
fn test_miniz_test_c_compiles() {
    let c_source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/miniz/miniz_test.c"),
    )
    .expect("failed to read miniz_test.c");

    // Just verify it has main and compiles
    assert!(c_source.contains("int main("));

    // Compile it
    let tmp = tempfile::tempdir().unwrap();
    let c_file = tmp.path().join("miniz_test.c");
    std::fs::write(&c_file, &c_source).unwrap();
    let output = std::process::Command::new("cc")
        .args(["-std=c11", "-o"])
        .arg(tmp.path().join("miniz_test"))
        .arg(&c_file)
        .output()
        .expect("failed to compile miniz_test.c");
    assert!(
        output.status.success(),
        "miniz_test.c should compile, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Test that migrate --no-llm on multiple fixtures produces consistent results.
#[test]
fn test_migrate_multiple_fixtures() {
    let fixtures = ["add.c", "power.c", "gcd.c", "factorial.c", "fibonacci.c"];

    for fixture in &fixtures {
        let tmp = tempfile::tempdir().unwrap();
        let fixture_path = format!("tests/fixtures/simple/{fixture}");
        let output = noricum_cmd()
            .args(["migrate", &fixture_path, "--no-llm", "--output"])
            .arg(tmp.path())
            .output()
            .unwrap_or_else(|_| panic!("failed to migrate {fixture}"));

        assert!(
            output.status.success(),
            "{fixture} migration should succeed, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Validated") || stdout.contains("Repairing"),
            "{fixture} should produce output, got: {stdout}"
        );
    }
}
