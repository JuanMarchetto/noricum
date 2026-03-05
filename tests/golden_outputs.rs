//! Golden output regression tests.
//!
//! These tests compile each C fixture and verify the output matches
//! known-good expected values. This catches fixture file corruption,
//! compiler behavior changes, and ensures our diff test baseline is stable.

use std::path::Path;
use std::process::Command;

/// Compile a C fixture and return its stdout.
fn compile_and_run(fixture_path: &str) -> String {
    let full_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(fixture_path);
    assert!(
        full_path.exists(),
        "fixture not found: {}",
        full_path.display()
    );

    let tmp = tempfile::tempdir().unwrap();
    let exe = tmp.path().join("test_exe");

    let compile = Command::new("cc")
        .args(["-std=c11", "-o"])
        .arg(&exe)
        .arg(&full_path)
        .output()
        .expect("failed to run cc");

    assert!(
        compile.status.success(),
        "C compilation failed for {}: {}",
        fixture_path,
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&exe)
        .output()
        .expect("failed to run compiled executable");

    assert!(
        run.status.success(),
        "executable failed for {}: {}",
        fixture_path,
        String::from_utf8_lossy(&run.stderr)
    );

    String::from_utf8_lossy(&run.stdout).to_string()
}

#[test]
fn golden_add() {
    assert_eq!(compile_and_run("tests/fixtures/simple/add.c"), "5\n0\n0\n");
}

#[test]
fn golden_power() {
    assert_eq!(
        compile_and_run("tests/fixtures/simple/power.c"),
        "1024\n243\n1\n1\n1\n0\n42\n42\n"
    );
}

#[test]
fn golden_gcd() {
    assert_eq!(
        compile_and_run("tests/fixtures/simple/gcd.c"),
        "4\n25\n1\n12\n15\n"
    );
}

#[test]
fn golden_factorial() {
    assert_eq!(
        compile_and_run("tests/fixtures/simple/factorial.c"),
        "1\n1\n120\n3628800\n"
    );
}

#[test]
fn golden_fibonacci() {
    assert_eq!(
        compile_and_run("tests/fixtures/simple/fibonacci.c"),
        "0\n1\n5\n55\n6765\n"
    );
}

#[test]
fn golden_max_min() {
    assert_eq!(
        compile_and_run("tests/fixtures/simple/max_min.c"),
        "7\n-1\n3\n-5\n5\n0\n10\n"
    );
}

#[test]
fn golden_strlen() {
    assert_eq!(
        compile_and_run("tests/fixtures/simple/strlen.c"),
        "5\n0\n7\n"
    );
}

#[test]
fn golden_linked_list() {
    assert_eq!(
        compile_and_run("tests/fixtures/simple/linked_list.c"),
        "60\n"
    );
}

#[test]
fn golden_error_codes() {
    assert_eq!(
        compile_and_run("tests/fixtures/simple/error_codes.c"),
        "10/3 = 3 (err=0)\n10/0 err=-2\nINT_MAX+1 err=-3\n"
    );
}

#[test]
fn golden_buffer() {
    assert_eq!(
        compile_and_run("tests/fixtures/simple/buffer.c"),
        "Hello, Noricum! (len=15)\n"
    );
}

#[test]
fn golden_hash_table() {
    assert_eq!(
        compile_and_run("tests/fixtures/medium/hash_table.c"),
        "size=3\nbeta=2\nbeta_updated=42\nafter_delete=2\nalpha_found=-1\nafter_bulk=22\nkey_0=0\nkey_19=190\ndone\n"
    );
}

#[test]
fn golden_miniz_test() {
    let output = compile_and_run("tests/fixtures/miniz/miniz_test.c");
    assert_eq!(
        output,
        "adler32_hello=530449514\nadler32_empty=1\nadler32_null=1\ncrc32_hello=3964322768\ncrc32_empty=0\ncrc32_null=0\nadler32_incremental=530449514\ncrc32_incremental=3964322768\ndone\n"
    );
}
