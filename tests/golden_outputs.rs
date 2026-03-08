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
        .args(["-std=gnu11", "-o"])
        .arg(&exe)
        .arg(&full_path)
        .arg("-lm")
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

/// Compile multiple C source files with an include directory and return stdout.
fn compile_and_run_multi(sources: &[&str], include_dir: &str) -> String {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let include_path = base.join(include_dir);
    let tmp = tempfile::tempdir().unwrap();
    let exe = tmp.path().join("test_exe");

    let mut cmd = Command::new("cc");
    cmd.args(["-std=c11", "-lm", "-I"])
        .arg(&include_path)
        .arg("-o")
        .arg(&exe);
    for source in sources {
        let full_path = base.join(source);
        assert!(
            full_path.exists(),
            "fixture not found: {}",
            full_path.display()
        );
        cmd.arg(&full_path);
    }

    let compile = cmd.output().expect("failed to run cc");
    assert!(
        compile.status.success(),
        "multi-file C compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&exe)
        .output()
        .expect("failed to run compiled executable");
    assert!(
        run.status.success(),
        "executable failed: {}",
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

#[test]
fn golden_cjson_test() {
    let output = compile_and_run_multi(
        &[
            "tests/fixtures/cjson/cjson_test.c",
            "tests/fixtures/cjson/cJSON.c",
        ],
        "tests/fixtures/cjson",
    );
    assert_eq!(
        output,
        "create: {\"name\":\"noricum\",\"version\":1,\"valid\":true}\nkey=value\nnum=42\narray_size=5\nitem_2=3\ndone\n"
    );
}

/// Compile a Rust fixture and return its stdout.
fn compile_and_run_rust(fixture_path: &str) -> String {
    let full_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(fixture_path);
    assert!(
        full_path.exists(),
        "fixture not found: {}",
        full_path.display()
    );

    let tmp = tempfile::tempdir().unwrap();
    let exe = tmp.path().join("test_exe");

    let compile = Command::new("rustc")
        .arg(&full_path)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("failed to run rustc");

    assert!(
        compile.status.success(),
        "Rust compilation failed for {}: {}",
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
fn golden_cjson_combined() {
    // Verify the C combined source produces expected output
    let c_output = compile_and_run("tests/fixtures/cjson/cjson_combined.c");
    assert_eq!(
        c_output,
        "create: {\"name\":\"noricum\",\"version\":1,\"valid\":true}\nkey=value\nnum=42\narray_size=5\nitem_2=3\ndone\n"
    );

    // Verify the migrated Rust output if it exists (generated by `noricum migrate`)
    let rust_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("output/cjson/cjson_combined.rs");
    if rust_path.exists() {
        let rust_output = compile_and_run_rust("output/cjson/cjson_combined.rs");
        assert_eq!(
            c_output, rust_output,
            "Rust migration output must match C output byte-for-byte"
        );
    }
}

#[test]
fn golden_expr_eval_fixture() {
    let output = compile_and_run("tests/fixtures/large/expr_eval.c");
    assert!(
        output.contains("=== Basic Arithmetic ==="),
        "should contain basic arithmetic header"
    );
    assert!(output.contains("2 + 3 = 5"), "should contain 2 + 3 = 5");
    assert!(
        output.contains("All tests completed."),
        "should contain completion message"
    );
}

#[test]
fn golden_picohttpparser() {
    // Compile C fixture with appropriate flags (suppress unused warnings from combined file)
    let full_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/picohttpparser/picohttpparser_combined.c");
    assert!(full_path.exists(), "picohttpparser C fixture not found");

    let tmp = tempfile::tempdir().unwrap();
    let exe = tmp.path().join("test_exe");

    let compile = Command::new("cc")
        .args([
            "-std=gnu11",
            "-Wno-unused-function",
            "-Wno-unused-parameter",
            "-o",
        ])
        .arg(&exe)
        .arg(&full_path)
        .output()
        .expect("failed to run cc");
    assert!(
        compile.status.success(),
        "picohttpparser C compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let c_run = Command::new(&exe)
        .output()
        .expect("failed to run picohttpparser C executable");
    assert!(
        c_run.status.success(),
        "picohttpparser C test failed: {}",
        String::from_utf8_lossy(&c_run.stderr)
    );
    let c_output = String::from_utf8_lossy(&c_run.stdout).to_string();

    // Verify against expected output
    let expected = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/picohttpparser/expected_output.txt"),
    )
    .expect("failed to read expected output");
    assert_eq!(c_output, expected, "C output must match expected_output.txt");

    // Verify the migrated Rust version produces identical output
    let rust_output =
        compile_and_run_rust("tests/fixtures/picohttpparser/picohttpparser_migrated.rs");
    assert_eq!(
        c_output, rust_output,
        "Rust migration output must match C output byte-for-byte"
    );
}

#[test]
fn golden_genann() {
    let c_output = compile_and_run("tests/fixtures/genann/genann_combined.c");
    let expected = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/genann/expected_output.txt"),
    )
    .expect("failed to read expected output");
    assert_eq!(c_output, expected, "genann C output must match expected_output.txt");

    // Verify the migrated Rust version produces identical output
    let rust_output = compile_and_run_rust("tests/fixtures/genann/genann_migrated.rs");
    assert_eq!(
        c_output, rust_output,
        "genann Rust migration must match C output byte-for-byte"
    );
}

#[test]
fn golden_cjson_combined_extended() {
    let c_output = compile_and_run("tests/fixtures/cjson/cjson_combined_extended.c");
    assert_eq!(
        c_output,
        "empty_obj: {}\nnested: {\"a\":{\"b\":1}}\n\
         escaped: {\"quote\":\"say \\\"hello\\\"\",\"backslash\":\"path\\\\to\\\\file\",\"newline\":\"line1\\nline2\",\"tab\":\"col1\\tcol2\"}\n\
         roundtrip: {\"x\":10,\"y\":[1,2,3],\"z\":{\"w\":true}}\n\
         parse_fail: NULL\nparse_fail2: NULL\n\
         arr_size: 5\narr[0]=10\narr[1]=20\narr[2]=30\narr[3]=40\narr[4]=50\n\
         zero: 0\nnegative: -42\nlarge: 1000000\n\
         bools: {\"t\":true,\"f\":false}\n\
         null_val: null\nneg_parse: -99\n\
         whitespace: {\"a\":1,\"b\":2}\n\
         empty_arr: []\nempty_arr_size: 0\n\
         extended_done\n"
    );
}
