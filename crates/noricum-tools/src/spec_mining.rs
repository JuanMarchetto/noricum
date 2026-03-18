//! Behavioral specification mining: extract function I/O traces from C programs.
//!
//! Instruments C source to log function inputs/outputs, collects traces from
//! execution, and generates Rust property tests from the traces.
//!
//! ## Pipeline
//! 1. [`find_instrumentable_functions`] — identify which C functions can be traced
//! 2. [`generate_instrumented_source`] — rename originals, inject wrapper functions
//! 3. [`collect_traces`] / [`mine_specs`] — compile, run, parse `NORICUM_TRACE|` lines
//! 4. [`generate_spec_tests`] / [`generate_spec_test_module`] — produce Rust `#[test]` fns
//! 5. [`validate_against_specs`] — compile+run spec tests against translated Rust code

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::ast::extract_c_functions;
use crate::diff_test::{compile_c_exe, run_exe};
use crate::ToolError;

// ---------------------------------------------------------------------------
// Core types (Task 1)
// ---------------------------------------------------------------------------

/// A single observed input/output trace for a C function call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionTrace {
    /// Name of the C function that was called.
    pub function_name: String,
    /// Serialized argument values in call order.
    pub inputs: Vec<TraceValue>,
    /// Serialized return value (None for void functions).
    pub output: Option<TraceValue>,
    /// Invocation index (0-based, for ordering multiple calls to the same function).
    pub call_index: u32,
}

/// A traced value with its C type and serialized representation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TraceValue {
    /// C type as string (e.g., "int", "double", "const char*").
    pub c_type: String,
    /// Value serialized as string (e.g., "42", "3.14", "hello").
    pub value: String,
}

/// A C function signature suitable for instrumentation.
#[derive(Debug, Clone)]
pub struct InstrumentableFunction {
    /// Function name.
    pub name: String,
    /// Return type as C string.
    pub return_type: String,
    /// Parameter names and types.
    pub params: Vec<(String, String)>,
    /// Whether this function can be instrumented.
    pub instrumentable: bool,
    /// Reason if not instrumentable.
    pub skip_reason: Option<String>,
}

/// Trace line format written to stderr by instrumented C code.
pub const TRACE_PREFIX: &str = "NORICUM_TRACE|";

// ---------------------------------------------------------------------------
// Function signature extraction (Task 2)
// ---------------------------------------------------------------------------

/// Supported C types for trace instrumentation.
const SCALAR_TYPES: &[&str] = &[
    "int",
    "unsigned int",
    "unsigned",
    "long",
    "unsigned long",
    "short",
    "unsigned short",
    "char",
    "unsigned char",
    "float",
    "double",
    "long double",
    "size_t",
    "ssize_t",
    "int8_t",
    "int16_t",
    "int32_t",
    "int64_t",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "bool",
    "_Bool",
];

/// C types we can instrument for string-like values.
const STRING_TYPES: &[&str] = &["const char*", "const char *", "char*", "char *"];

/// Analyze C source and identify functions suitable for instrumentation.
///
/// Skips: variadics (`...`), void* params, struct params, function pointers,
/// FILE* params, functions returning void (though we can still trace inputs).
pub fn find_instrumentable_functions(c_source: &str) -> Vec<InstrumentableFunction> {
    let functions = extract_c_functions(c_source);
    let mut results = Vec::new();

    for func in &functions {
        let sig_line = c_source[func.start_byte..func.end_byte]
            .lines()
            .next()
            .unwrap_or("");

        let params = parse_params_from_signature(sig_line, &func.name);
        let return_type = func.return_type.trim().to_string();

        let (instrumentable, skip_reason) = check_instrumentable(&return_type, &params, sig_line);

        results.push(InstrumentableFunction {
            name: func.name.clone(),
            return_type,
            params,
            instrumentable,
            skip_reason,
        });
    }

    results
}

/// Parse parameters from a C function signature line.
fn parse_params_from_signature(sig_line: &str, _fn_name: &str) -> Vec<(String, String)> {
    let mut params = Vec::new();

    let open = match sig_line.find('(') {
        Some(i) => i,
        None => return params,
    };
    let close = match sig_line.rfind(')') {
        Some(i) => i,
        None => return params,
    };
    let param_str = &sig_line[open + 1..close];

    if param_str.trim() == "void" || param_str.trim().is_empty() {
        return params;
    }

    for param in param_str.split(',') {
        let param = param.trim();
        if param == "..." {
            params.push(("...".to_string(), "...".to_string()));
            continue;
        }
        let tokens: Vec<&str> = param.split_whitespace().collect();
        if tokens.len() >= 2 {
            let name = tokens.last().unwrap_or(&"").trim_start_matches('*').to_string();
            let type_part = if param.contains('*') {
                let star_count = param.chars().filter(|c| *c == '*').count();
                let base_type: Vec<&str> = tokens[..tokens.len() - 1]
                    .iter()
                    .map(|t| t.trim_end_matches('*'))
                    .filter(|t| !t.is_empty())
                    .collect();
                format!("{}{}", base_type.join(" "), "*".repeat(star_count))
            } else {
                tokens[..tokens.len() - 1].join(" ")
            };
            params.push((name, type_part));
        } else if tokens.len() == 1 {
            params.push((format!("_arg{}", params.len()), tokens[0].to_string()));
        }
    }

    params
}

/// Check if a function can be instrumented.
fn check_instrumentable(
    return_type: &str,
    params: &[(String, String)],
    sig_line: &str,
) -> (bool, Option<String>) {
    if sig_line.contains("...") {
        return (false, Some("variadic function".to_string()));
    }

    for (_name, c_type) in params {
        let t = c_type.trim();
        if t.contains("void*") || t.contains("void *") {
            return (false, Some(format!("void* parameter: {t}")));
        }
        if t.starts_with("struct ") || t.starts_with("union ") {
            return (false, Some(format!("struct/union parameter: {t}")));
        }
        if t.contains("(*") {
            return (false, Some(format!("function pointer parameter: {t}")));
        }
        if t.contains("FILE") {
            return (false, Some(format!("FILE parameter: {t}")));
        }
        let is_scalar = SCALAR_TYPES.contains(&t);
        let is_string = STRING_TYPES.contains(&t);
        let is_pointer_to_scalar = t.ends_with('*') && {
            let base = t.trim_end_matches('*').trim();
            SCALAR_TYPES.iter().any(|st| {
                let const_st = format!("const {st}");
                base == *st || base == const_st
            })
        };
        if !is_scalar && !is_string && !is_pointer_to_scalar {
            return (false, Some(format!("unsupported parameter type: {t}")));
        }
    }

    let rt = return_type.trim();
    if rt != "void" {
        let is_scalar_ret = SCALAR_TYPES.contains(&rt);
        let is_string_ret = STRING_TYPES.contains(&rt);
        let is_ptr_ret = rt.ends_with('*');
        if !is_scalar_ret && !is_string_ret && !is_ptr_ret {
            return (false, Some(format!("unsupported return type: {rt}")));
        }
    }

    (true, None)
}

// ---------------------------------------------------------------------------
// Instrumented source generation (Task 3)
// ---------------------------------------------------------------------------

/// Result of instrumenting a C source file.
#[derive(Debug)]
pub struct InstrumentedSource {
    /// The modified C source with tracing wrappers.
    pub source: String,
    /// Names of functions that were successfully instrumented.
    pub instrumented_functions: Vec<String>,
}

/// Generate instrumented C source with wrapper functions that trace I/O.
///
/// For each instrumentable function `foo(int a, int b) -> int`:
/// 1. Renames original to `__noricum_orig_foo`
/// 2. Creates wrapper `foo` that calls `__noricum_orig_foo` and logs trace to stderr
///
/// Non-instrumentable functions and `main()` are left unchanged.
pub fn generate_instrumented_source(c_source: &str) -> InstrumentedSource {
    let functions = find_instrumentable_functions(c_source);
    let mut instrumented = String::new();

    instrumented.push_str("#include <stdio.h>\n");
    instrumented.push_str("#include <string.h>\n\n");

    let mut wrapped_names: Vec<String> = Vec::new();

    // First pass: rename instrumentable functions
    let mut modified_source = c_source.to_string();
    for func in &functions {
        if !func.instrumentable || func.name == "main" {
            continue;
        }
        let original_pattern = format!("{} {}(", func.return_type, func.name);
        let renamed = format!("{} __noricum_orig_{}(", func.return_type, func.name);
        if let Some(pos) = modified_source.find(&original_pattern) {
            modified_source = format!(
                "{}{}{}",
                &modified_source[..pos],
                renamed,
                &modified_source[pos + original_pattern.len()..]
            );
            wrapped_names.push(func.name.clone());
        }
    }

    // Add forward declarations for wrapper functions so main() can call them
    instrumented.push_str("// === NORICUM FORWARD DECLARATIONS ===\n");
    for func in &functions {
        if !func.instrumentable || func.name == "main" || !wrapped_names.contains(&func.name) {
            continue;
        }
        let param_decls: Vec<String> = func
            .params
            .iter()
            .map(|(name, ty)| format!("{ty} {name}"))
            .collect();
        instrumented.push_str(&format!(
            "{} {}({});\n",
            func.return_type,
            func.name,
            param_decls.join(", ")
        ));
    }
    instrumented.push('\n');

    instrumented.push_str(&modified_source);
    instrumented.push_str("\n\n// === NORICUM INSTRUMENTATION WRAPPERS ===\n\n");

    // Second pass: generate wrapper functions
    for func in &functions {
        if !func.instrumentable || func.name == "main" || !wrapped_names.contains(&func.name) {
            continue;
        }

        let counter_name = format!("__noricum_call_count_{}", func.name);
        instrumented.push_str(&format!("static int {counter_name} = 0;\n"));

        let param_decls: Vec<String> = func
            .params
            .iter()
            .map(|(name, ty)| format!("{ty} {name}"))
            .collect();

        instrumented.push_str(&format!(
            "{} {}({}) {{\n",
            func.return_type,
            func.name,
            param_decls.join(", ")
        ));

        let param_names: Vec<&str> = func.params.iter().map(|(n, _)| n.as_str()).collect();
        if func.return_type.trim() == "void" {
            instrumented.push_str(&format!(
                "    __noricum_orig_{}({});\n",
                func.name,
                param_names.join(", ")
            ));
        } else {
            instrumented.push_str(&format!(
                "    {} __noricum_result = __noricum_orig_{}({});\n",
                func.return_type,
                func.name,
                param_names.join(", ")
            ));
        }

        // Generate fprintf trace call
        instrumented.push_str(&format!(
            "    fprintf(stderr, \"NORICUM_TRACE|{{\\\"function_name\\\":\\\"{}\\\",\\\"inputs\\\":[",
            func.name
        ));

        for (i, (_name, c_type)) in func.params.iter().enumerate() {
            if i > 0 {
                instrumented.push(',');
            }
            let format_spec = trace_format_spec(c_type);
            instrumented.push_str(&format!(
                "{{\\\"c_type\\\":\\\"{}\\\",\\\"value\\\":\\\"{}\\\"}}",
                c_type.replace('\"', "\\\\\\\""),
                format_spec
            ));
        }

        instrumented.push_str("],\\\"output\\\":");

        if func.return_type.trim() == "void" {
            instrumented.push_str("null");
        } else {
            let ret_fmt = trace_format_spec(&func.return_type);
            instrumented.push_str(&format!(
                "{{\\\"c_type\\\":\\\"{}\\\",\\\"value\\\":\\\"{}\\\"}}",
                func.return_type.replace('\"', "\\\\\\\""),
                ret_fmt
            ));
        }

        instrumented.push_str(",\\\"call_index\\\":%d}\\n\", ");

        for (name, c_type) in &func.params {
            instrumented.push_str(&trace_fprintf_arg(name, c_type));
            instrumented.push_str(", ");
        }
        if func.return_type.trim() != "void" {
            instrumented.push_str(&trace_fprintf_arg("__noricum_result", &func.return_type));
            instrumented.push_str(", ");
        }
        instrumented.push_str(&format!("{counter_name}++"));
        instrumented.push_str(");\n");

        if func.return_type.trim() != "void" {
            instrumented.push_str("    return __noricum_result;\n");
        }

        instrumented.push_str("}\n\n");
    }

    InstrumentedSource {
        source: instrumented,
        instrumented_functions: wrapped_names,
    }
}

/// Get the fprintf format specifier for a C type.
fn trace_format_spec(c_type: &str) -> &'static str {
    let t = c_type.trim();
    if t.contains("char*") || t.contains("char *") {
        return "%s";
    }
    if t.contains("double") || t.contains("float") {
        return "%.17g";
    }
    if t.contains("unsigned long") || t == "size_t" {
        return "%lu";
    }
    if t.contains("long") {
        return "%ld";
    }
    "%d"
}

/// Get the fprintf argument expression for a parameter.
fn trace_fprintf_arg(name: &str, c_type: &str) -> String {
    let t = c_type.trim();
    if t.contains("char*") || t.contains("char *") {
        return format!("({name} ? {name} : \"(null)\")");
    }
    name.to_string()
}

// ---------------------------------------------------------------------------
// Trace collection (Task 4)
// ---------------------------------------------------------------------------

/// Compile instrumented C source and collect function traces.
///
/// 1. Write instrumented source to temp file
/// 2. Compile with `cc -std=gnu11 -lm`
/// 3. Run the executable
/// 4. Parse `NORICUM_TRACE|` lines from stderr
pub fn collect_traces(
    instrumented_source: &str,
    args: &[String],
    stdin_input: Option<&str>,
) -> Result<Vec<FunctionTrace>, ToolError> {
    let tmp = tempfile::tempdir()?;
    let c_file = tmp.path().join("instrumented.c");
    let exe_file = tmp.path().join("instrumented_exe");

    std::fs::write(&c_file, instrumented_source)?;

    let compiled = compile_c_exe(&c_file, &exe_file)?;
    if !compiled {
        warn!("instrumented C source failed to compile");
        return Ok(Vec::new());
    }

    let output = run_exe(&exe_file, stdin_input, args)?;
    let traces = parse_traces(&output.stderr);

    info!(
        trace_count = traces.len(),
        exit_code = output.exit_code,
        "collected function traces"
    );

    Ok(traces)
}

/// Parse NORICUM_TRACE lines from stderr output into structured traces.
pub fn parse_traces(stderr: &str) -> Vec<FunctionTrace> {
    let mut traces = Vec::new();

    for line in stderr.lines() {
        let line = line.trim();
        if let Some(json_str) = line.strip_prefix(TRACE_PREFIX) {
            match serde_json::from_str::<FunctionTrace>(json_str) {
                Ok(trace) => traces.push(trace),
                Err(e) => {
                    debug!(line = %line, error = %e, "failed to parse trace line");
                }
            }
        }
    }

    traces
}

/// Mine behavioral specs from a C source file using its existing main() as the driver.
///
/// Main entry point: takes raw C source, instruments it, compiles, runs, and returns traces.
pub fn mine_specs(c_source: &str) -> Result<Vec<FunctionTrace>, ToolError> {
    let instrumented = generate_instrumented_source(c_source);

    if instrumented.instrumented_functions.is_empty() {
        info!("no instrumentable functions found, skipping spec mining");
        return Ok(Vec::new());
    }

    info!(
        functions = instrumented.instrumented_functions.len(),
        names = ?instrumented.instrumented_functions,
        "mining specs from instrumented C source"
    );

    collect_traces(&instrumented.source, &[], None)
}

// ---------------------------------------------------------------------------
// Rust test generation (Task 5)
// ---------------------------------------------------------------------------

/// Generated Rust spec test from behavioral traces.
#[derive(Debug, Clone)]
pub struct SpecTest {
    /// Rust test function source code.
    pub test_source: String,
    /// Name of the C function being tested.
    pub function_name: String,
    /// Number of test cases (trace entries) for this function.
    pub case_count: usize,
}

/// Generate Rust test functions from collected function traces.
///
/// Groups traces by function name and generates one `#[test]` per C function.
pub fn generate_spec_tests(traces: &[FunctionTrace]) -> Vec<SpecTest> {
    let mut by_function: std::collections::BTreeMap<String, Vec<&FunctionTrace>> =
        std::collections::BTreeMap::new();

    for trace in traces {
        by_function
            .entry(trace.function_name.clone())
            .or_default()
            .push(trace);
    }

    let mut tests = Vec::new();

    for (fn_name, fn_traces) in &by_function {
        let mut test_body = String::new();
        test_body.push_str(&format!("#[test]\nfn spec_{fn_name}() {{\n"));

        for trace in fn_traces {
            if let Some(ref output) = trace.output {
                let args: Vec<String> = trace
                    .inputs
                    .iter()
                    .map(rust_literal_for_trace_value)
                    .collect();

                let expected = rust_literal_for_trace_value(output);
                let call = format!("{fn_name}({})", args.join(", "));

                if output.c_type.contains("double") || output.c_type.contains("float") {
                    test_body.push_str(&format!(
                        "    assert!(({call} - {expected}).abs() < 1e-10, \"call_index={}\");\n",
                        trace.call_index
                    ));
                } else {
                    test_body.push_str(&format!(
                        "    assert_eq!({call}, {expected}, \"call_index={}\");\n",
                        trace.call_index
                    ));
                }
            }
        }

        test_body.push_str("}\n");

        tests.push(SpecTest {
            test_source: test_body,
            function_name: fn_name.clone(),
            case_count: fn_traces.len(),
        });
    }

    tests
}

/// Convert a trace value to a Rust literal string.
fn rust_literal_for_trace_value(tv: &TraceValue) -> String {
    let t = tv.c_type.trim();

    if t.contains("char*") || t.contains("char *") {
        return format!(
            "\"{}\"",
            tv.value.replace('\\', "\\\\").replace('\"', "\\\"")
        );
    }
    if t.contains("double") || t.contains("float") {
        let suffix = if t.contains("float") { "f32" } else { "f64" };
        return format!("{}_{suffix}", tv.value);
    }
    if t.contains("unsigned long") || t == "size_t" {
        return format!("{}_u64", tv.value);
    }
    if t.contains("unsigned") {
        return format!("{}_u32", tv.value);
    }
    if t.contains("long") {
        return format!("{}_i64", tv.value);
    }
    format!("{}_i32", tv.value)
}

/// Generate a complete Rust test module from traces, ready to append to translated code.
///
/// Returns a `#[cfg(test)] mod spec_tests { ... }` block with all generated tests.
pub fn generate_spec_test_module(traces: &[FunctionTrace]) -> String {
    let tests = generate_spec_tests(traces);

    if tests.is_empty() {
        return String::new();
    }

    let mut module = String::new();
    module.push_str("\n#[cfg(test)]\nmod spec_tests {\n    use super::*;\n\n");

    for test in &tests {
        for line in test.test_source.lines() {
            module.push_str("    ");
            module.push_str(line);
            module.push('\n');
        }
        module.push('\n');
    }

    module.push_str("}\n");
    module
}

// ---------------------------------------------------------------------------
// Spec validation (Task 6)
// ---------------------------------------------------------------------------

/// Result of validating Rust code against mined behavioral specs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecValidationResult {
    /// Total number of spec test cases.
    pub total_specs: usize,
    /// Number of specs that passed.
    pub passed: usize,
    /// Number of specs that failed.
    pub failed: usize,
    /// Details of failed specs.
    pub failures: Vec<String>,
}

/// Validate a Rust module against mined specs.
///
/// Compiles the Rust source with the spec test module appended,
/// then runs as a test binary. Returns per-function pass/fail.
pub fn validate_against_specs(
    rust_source: &str,
    traces: &[FunctionTrace],
) -> Result<SpecValidationResult, ToolError> {
    let spec_module = generate_spec_test_module(traces);
    if spec_module.is_empty() {
        return Ok(SpecValidationResult {
            total_specs: 0,
            passed: 0,
            failed: 0,
            failures: Vec::new(),
        });
    }

    let combined = format!("{rust_source}\n{spec_module}");

    let tmp = tempfile::tempdir()?;
    let rs_file = tmp.path().join("spec_test.rs");
    let test_exe = tmp.path().join("spec_test_exe");

    std::fs::write(&rs_file, &combined)?;

    let output = std::process::Command::new("rustc")
        .arg("--edition=2024")
        .arg("--test")
        .arg("-o")
        .arg(&test_exe)
        .arg(&rs_file)
        .output()
        .map_err(|_| ToolError::CommandNotFound("rustc".to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!(stderr = %stderr, "spec test compilation failed");
        return Ok(SpecValidationResult {
            total_specs: traces.len(),
            passed: 0,
            failed: traces.len(),
            failures: vec![format!(
                "Spec tests failed to compile: {}",
                stderr.lines().take(5).collect::<Vec<_>>().join("\n")
            )],
        });
    }

    let test_output = run_exe(&test_exe, None, &[])?;

    // Parse test output — check both stdout and stderr (Rust test harness output location varies)
    let all_output = format!("{}\n{}", test_output.stdout, test_output.stderr);
    let passed = all_output
        .lines()
        .filter(|l| l.contains("test ") && l.contains("... ok"))
        .count();
    let failed_lines: Vec<String> = all_output
        .lines()
        .filter(|l| l.contains("test ") && l.contains("... FAILED"))
        .map(String::from)
        .collect();
    let failed = failed_lines.len();

    Ok(SpecValidationResult {
        total_specs: passed + failed,
        passed,
        failed,
        failures: failed_lines,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Task 1: Serialization ---

    #[test]
    fn test_trace_value_serialization() {
        let tv = TraceValue {
            c_type: "int".to_string(),
            value: "42".to_string(),
        };
        let json = serde_json::to_string(&tv).unwrap();
        let parsed: TraceValue = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, tv);
    }

    #[test]
    fn test_function_trace_serialization_roundtrip() {
        let trace = FunctionTrace {
            function_name: "add".to_string(),
            inputs: vec![
                TraceValue {
                    c_type: "int".to_string(),
                    value: "2".to_string(),
                },
                TraceValue {
                    c_type: "int".to_string(),
                    value: "3".to_string(),
                },
            ],
            output: Some(TraceValue {
                c_type: "int".to_string(),
                value: "5".to_string(),
            }),
            call_index: 0,
        };
        let json = serde_json::to_string(&trace).unwrap();
        assert!(json.contains("\"function_name\":\"add\""));
        let parsed: FunctionTrace = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, trace);
    }

    #[test]
    fn test_function_trace_void_output() {
        let trace = FunctionTrace {
            function_name: "print_value".to_string(),
            inputs: vec![TraceValue {
                c_type: "int".to_string(),
                value: "7".to_string(),
            }],
            output: None,
            call_index: 0,
        };
        let json = serde_json::to_string(&trace).unwrap();
        let parsed: FunctionTrace = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.output, None);
    }

    #[test]
    fn test_trace_prefix_format() {
        assert_eq!(TRACE_PREFIX, "NORICUM_TRACE|");
    }

    // --- Task 2: Signature extraction ---

    #[test]
    fn test_find_instrumentable_add() {
        let source = "int add(int a, int b) { return a + b; }\nint main() { return 0; }";
        let fns = find_instrumentable_functions(source);
        let add = fns.iter().find(|f| f.name == "add").expect("should find add");
        assert!(add.instrumentable, "add should be instrumentable");
        assert_eq!(add.params.len(), 2);
        assert_eq!(add.return_type, "int");
    }

    #[test]
    fn test_find_instrumentable_rejects_variadic() {
        let source = "#include <stdarg.h>\nvoid my_printf(const char *fmt, ...) {}\nint main() { return 0; }";
        let fns = find_instrumentable_functions(source);
        let pf = fns
            .iter()
            .find(|f| f.name == "my_printf")
            .expect("should find my_printf");
        assert!(!pf.instrumentable, "variadic should not be instrumentable");
        assert!(pf.skip_reason.as_ref().unwrap().contains("variadic"));
    }

    #[test]
    fn test_find_instrumentable_rejects_void_ptr() {
        let source = "void process(void *data, int len) {}\nint main() { return 0; }";
        let fns = find_instrumentable_functions(source);
        let p = fns
            .iter()
            .find(|f| f.name == "process")
            .expect("should find process");
        assert!(!p.instrumentable, "void* param should not be instrumentable");
    }

    #[test]
    fn test_find_instrumentable_rejects_struct_param() {
        let source = "typedef struct { int x; } Point;\nvoid move_pt(struct Point p) {}\nint main() { return 0; }";
        let fns = find_instrumentable_functions(source);
        let m = fns.iter().find(|f| f.name == "move_pt");
        if let Some(func) = m {
            assert!(
                !func.instrumentable,
                "struct param should not be instrumentable"
            );
        }
    }

    #[test]
    fn test_find_instrumentable_accepts_string_param() {
        let source =
            "unsigned long hash_key(const char *str) { return 0; }\nint main() { return 0; }";
        let fns = find_instrumentable_functions(source);
        let hk = fns
            .iter()
            .find(|f| f.name == "hash_key")
            .expect("should find hash_key");
        assert!(hk.instrumentable, "const char* should be instrumentable");
    }

    #[test]
    fn test_parse_params_simple() {
        let params = parse_params_from_signature("int add(int a, int b)", "add");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "a");
        assert_eq!(params[1].0, "b");
    }

    #[test]
    fn test_parse_params_void() {
        let params = parse_params_from_signature("void init(void)", "init");
        assert!(params.is_empty());
    }

    // --- Task 3: Instrumentation ---

    #[test]
    fn test_generate_instrumented_source_simple() {
        let source = "int add(int a, int b) { return a + b; }\nint main(void) { return 0; }";
        let result = generate_instrumented_source(source);
        assert!(result.instrumented_functions.contains(&"add".to_string()));
        assert!(!result
            .instrumented_functions
            .contains(&"main".to_string()));
        assert!(result.source.contains("__noricum_orig_add"));
        assert!(result.source.contains("NORICUM_TRACE"));
    }

    #[test]
    fn test_generate_instrumented_source_skips_variadic() {
        let source = "void my_log(const char *fmt, ...) {}\nint main(void) { return 0; }";
        let result = generate_instrumented_source(source);
        assert!(!result
            .instrumented_functions
            .contains(&"my_log".to_string()));
    }

    #[test]
    fn test_trace_format_spec_types() {
        assert_eq!(trace_format_spec("int"), "%d");
        assert_eq!(trace_format_spec("double"), "%.17g");
        assert_eq!(trace_format_spec("const char*"), "%s");
        assert_eq!(trace_format_spec("unsigned long"), "%lu");
    }

    #[test]
    fn test_trace_fprintf_arg_null_safety() {
        let arg = trace_fprintf_arg("name", "const char*");
        assert!(arg.contains("null"), "string args should have NULL safety");
    }

    // --- Task 4: Trace parsing ---

    #[test]
    fn test_parse_traces_valid() {
        let stderr = "some other output\nNORICUM_TRACE|{\"function_name\":\"add\",\"inputs\":[{\"c_type\":\"int\",\"value\":\"2\"},{\"c_type\":\"int\",\"value\":\"3\"}],\"output\":{\"c_type\":\"int\",\"value\":\"5\"},\"call_index\":0}\nmore output\nNORICUM_TRACE|{\"function_name\":\"add\",\"inputs\":[{\"c_type\":\"int\",\"value\":\"-1\"},{\"c_type\":\"int\",\"value\":\"1\"}],\"output\":{\"c_type\":\"int\",\"value\":\"0\"},\"call_index\":1}\n";
        let traces = parse_traces(stderr);
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].function_name, "add");
        assert_eq!(traces[0].inputs[0].value, "2");
        assert_eq!(traces[0].output.as_ref().unwrap().value, "5");
        assert_eq!(traces[1].call_index, 1);
    }

    #[test]
    fn test_parse_traces_empty() {
        let traces = parse_traces("");
        assert!(traces.is_empty());
    }

    #[test]
    fn test_parse_traces_malformed_json() {
        let stderr = "NORICUM_TRACE|{not valid json}\nNORICUM_TRACE|{\"function_name\":\"ok\",\"inputs\":[],\"output\":null,\"call_index\":0}\n";
        let traces = parse_traces(stderr);
        assert_eq!(traces.len(), 1, "should skip malformed, keep valid");
        assert_eq!(traces[0].function_name, "ok");
    }

    #[test]
    fn test_collect_traces_add_function() {
        let c_source = r#"
#include <stdio.h>
int add(int a, int b) { return a + b; }
int main(void) {
    printf("%d\n", add(2, 3));
    printf("%d\n", add(-1, 1));
    printf("%d\n", add(0, 0));
    return 0;
}
"#;
        let traces = mine_specs(c_source).unwrap();
        assert_eq!(traces.len(), 3, "should capture 3 calls to add");
        assert_eq!(traces[0].function_name, "add");
        assert_eq!(traces[0].output.as_ref().unwrap().value, "5");
        assert_eq!(traces[1].output.as_ref().unwrap().value, "0");
        assert_eq!(traces[2].output.as_ref().unwrap().value, "0");
    }

    #[test]
    fn test_collect_traces_no_instrumentable_functions() {
        let c_source = "int main(void) { return 0; }";
        let traces = mine_specs(c_source).unwrap();
        assert!(traces.is_empty(), "no functions to instrument");
    }

    #[test]
    fn test_mine_specs_hash_key() {
        let c_source = r#"
#include <stdio.h>
unsigned long hash_key(const char *str) {
    unsigned long hash = 5381;
    int c;
    while ((c = *str++))
        hash = ((hash << 5) + hash) + c;
    return hash;
}
int main(void) {
    printf("%lu\n", hash_key("hello"));
    printf("%lu\n", hash_key(""));
    printf("%lu\n", hash_key("a"));
    return 0;
}
"#;
        let traces = mine_specs(c_source).unwrap();
        assert!(
            traces.len() >= 3,
            "should capture hash_key calls, got {}",
            traces.len()
        );
        assert_eq!(traces[0].function_name, "hash_key");
        let hash_hello = &traces[0].output.as_ref().unwrap().value;
        assert!(!hash_hello.is_empty());
    }

    // --- Task 5: Rust test generation ---

    #[test]
    fn test_generate_spec_tests_simple() {
        let traces = vec![
            FunctionTrace {
                function_name: "add".to_string(),
                inputs: vec![
                    TraceValue {
                        c_type: "int".to_string(),
                        value: "2".to_string(),
                    },
                    TraceValue {
                        c_type: "int".to_string(),
                        value: "3".to_string(),
                    },
                ],
                output: Some(TraceValue {
                    c_type: "int".to_string(),
                    value: "5".to_string(),
                }),
                call_index: 0,
            },
            FunctionTrace {
                function_name: "add".to_string(),
                inputs: vec![
                    TraceValue {
                        c_type: "int".to_string(),
                        value: "-1".to_string(),
                    },
                    TraceValue {
                        c_type: "int".to_string(),
                        value: "1".to_string(),
                    },
                ],
                output: Some(TraceValue {
                    c_type: "int".to_string(),
                    value: "0".to_string(),
                }),
                call_index: 1,
            },
        ];
        let tests = generate_spec_tests(&traces);
        assert_eq!(tests.len(), 1, "one function -> one test");
        assert_eq!(tests[0].function_name, "add");
        assert_eq!(tests[0].case_count, 2);
        assert!(tests[0].test_source.contains("assert_eq!(add(2_i32, 3_i32), 5_i32"));
        assert!(tests[0]
            .test_source
            .contains("assert_eq!(add(-1_i32, 1_i32), 0_i32"));
    }

    #[test]
    fn test_generate_spec_tests_float() {
        let traces = vec![FunctionTrace {
            function_name: "sqrt_approx".to_string(),
            inputs: vec![TraceValue {
                c_type: "double".to_string(),
                value: "4.0".to_string(),
            }],
            output: Some(TraceValue {
                c_type: "double".to_string(),
                value: "2.0".to_string(),
            }),
            call_index: 0,
        }];
        let tests = generate_spec_tests(&traces);
        assert!(
            tests[0].test_source.contains(".abs() < 1e-10"),
            "floats should use approximate comparison"
        );
    }

    #[test]
    fn test_generate_spec_tests_string() {
        let traces = vec![FunctionTrace {
            function_name: "hash_key".to_string(),
            inputs: vec![TraceValue {
                c_type: "const char*".to_string(),
                value: "hello".to_string(),
            }],
            output: Some(TraceValue {
                c_type: "unsigned long".to_string(),
                value: "261238937".to_string(),
            }),
            call_index: 0,
        }];
        let tests = generate_spec_tests(&traces);
        assert!(
            tests[0].test_source.contains("\"hello\""),
            "should pass string literal"
        );
        assert!(
            tests[0].test_source.contains("261238937_u64"),
            "unsigned long -> u64"
        );
    }

    #[test]
    fn test_generate_spec_test_module_format() {
        let traces = vec![FunctionTrace {
            function_name: "add".to_string(),
            inputs: vec![
                TraceValue {
                    c_type: "int".to_string(),
                    value: "1".to_string(),
                },
                TraceValue {
                    c_type: "int".to_string(),
                    value: "2".to_string(),
                },
            ],
            output: Some(TraceValue {
                c_type: "int".to_string(),
                value: "3".to_string(),
            }),
            call_index: 0,
        }];
        let module = generate_spec_test_module(&traces);
        assert!(module.contains("#[cfg(test)]"));
        assert!(module.contains("mod spec_tests"));
        assert!(module.contains("use super::*;"));
        assert!(module.contains("fn spec_add()"));
    }

    #[test]
    fn test_generate_spec_test_module_empty() {
        let module = generate_spec_test_module(&[]);
        assert!(module.is_empty(), "no traces -> no module");
    }

    #[test]
    fn test_rust_literal_for_trace_value_types() {
        assert_eq!(
            rust_literal_for_trace_value(&TraceValue {
                c_type: "int".to_string(),
                value: "42".to_string()
            }),
            "42_i32"
        );
        assert_eq!(
            rust_literal_for_trace_value(&TraceValue {
                c_type: "double".to_string(),
                value: "3.14".to_string()
            }),
            "3.14_f64"
        );
        assert_eq!(
            rust_literal_for_trace_value(&TraceValue {
                c_type: "const char*".to_string(),
                value: "hi".to_string()
            }),
            "\"hi\""
        );
        assert_eq!(
            rust_literal_for_trace_value(&TraceValue {
                c_type: "unsigned long".to_string(),
                value: "99".to_string()
            }),
            "99_u64"
        );
    }

    // --- Task 6: Spec validation ---

    #[test]
    fn test_validate_against_specs_passing() {
        let rust_source = "pub fn add(a: i32, b: i32) -> i32 { a + b }";
        let traces = vec![FunctionTrace {
            function_name: "add".to_string(),
            inputs: vec![
                TraceValue {
                    c_type: "int".to_string(),
                    value: "2".to_string(),
                },
                TraceValue {
                    c_type: "int".to_string(),
                    value: "3".to_string(),
                },
            ],
            output: Some(TraceValue {
                c_type: "int".to_string(),
                value: "5".to_string(),
            }),
            call_index: 0,
        }];
        let result = validate_against_specs(rust_source, &traces).unwrap();
        assert_eq!(result.passed, 1);
        assert_eq!(result.failed, 0);
    }

    #[test]
    fn test_validate_against_specs_failing() {
        let rust_source = "pub fn add(a: i32, b: i32) -> i32 { a * b }"; // wrong!
        let traces = vec![FunctionTrace {
            function_name: "add".to_string(),
            inputs: vec![
                TraceValue {
                    c_type: "int".to_string(),
                    value: "2".to_string(),
                },
                TraceValue {
                    c_type: "int".to_string(),
                    value: "3".to_string(),
                },
            ],
            output: Some(TraceValue {
                c_type: "int".to_string(),
                value: "5".to_string(),
            }),
            call_index: 0,
        }];
        let result = validate_against_specs(rust_source, &traces).unwrap();
        assert!(result.failed > 0, "wrong implementation should fail spec");
    }

    #[test]
    fn test_validate_against_specs_empty_traces() {
        let result = validate_against_specs("fn f() {}", &[]).unwrap();
        assert_eq!(result.total_specs, 0);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_mine_specs_add_fixture() {
        let c_source = include_str!("../../../tests/fixtures/simple/add.c");
        let traces = mine_specs(c_source).unwrap();
        assert!(
            traces.iter().any(|t| t.function_name == "add"),
            "should capture add() traces from add.c, got {} traces: {:?}",
            traces.len(),
            traces.iter().map(|t| &t.function_name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_mine_specs_factorial_fixture() {
        let c_source = include_str!("../../../tests/fixtures/simple/factorial.c");
        let traces = mine_specs(c_source).unwrap();

        let fact_traces: Vec<&FunctionTrace> = traces
            .iter()
            .filter(|t| t.function_name == "factorial")
            .collect();

        assert!(
            !fact_traces.is_empty(),
            "should capture factorial traces from factorial.c"
        );

        // Verify a known factorial value if available
        if let Some(trace) = fact_traces
            .iter()
            .find(|t| t.inputs.first().map(|v| v.value.as_str()) == Some("5"))
        {
            assert_eq!(
                trace.output.as_ref().unwrap().value,
                "120",
                "factorial(5) should be 120"
            );
        }
    }

    #[test]
    fn test_mine_specs_gcd_fixture() {
        let c_source = include_str!("../../../tests/fixtures/simple/gcd.c");
        let traces = mine_specs(c_source).unwrap();

        let gcd_traces: Vec<&FunctionTrace> = traces
            .iter()
            .filter(|t| t.function_name == "gcd")
            .collect();

        assert!(
            !gcd_traces.is_empty(),
            "should capture gcd traces from gcd.c"
        );
    }

    #[test]
    fn test_mine_specs_hash_table_fixture() {
        let c_source = include_str!("../../../tests/fixtures/medium/hash_table.c");

        let functions = find_instrumentable_functions(c_source);
        let instrumentable: Vec<&InstrumentableFunction> =
            functions.iter().filter(|f| f.instrumentable).collect();

        // hash_key takes (const char*) -> unsigned long: should be instrumentable
        assert!(
            instrumentable.iter().any(|f| f.name == "hash_key"),
            "hash_key should be instrumentable, found: {:?}",
            functions
                .iter()
                .map(|f| (&f.name, f.instrumentable, &f.skip_reason))
                .collect::<Vec<_>>()
        );

        let traces = mine_specs(c_source).unwrap();

        let hash_traces: Vec<&FunctionTrace> = traces
            .iter()
            .filter(|t| t.function_name == "hash_key")
            .collect();

        assert!(
            !hash_traces.is_empty(),
            "should capture hash_key traces from hash_table.c"
        );

        // Generate spec tests and verify format
        let spec_tests = generate_spec_tests(&traces);
        assert!(
            !spec_tests.is_empty(),
            "should generate at least one spec test"
        );

        let module = generate_spec_test_module(&traces);
        assert!(module.contains("#[test]"));
        assert!(module.contains("fn spec_hash_key"));
    }

    #[test]
    fn test_mine_specs_large_fixture() {
        let c_source = include_str!("../../../tests/fixtures/large/expr_eval.c");
        let functions = find_instrumentable_functions(c_source);
        let instrumentable_count = functions.iter().filter(|f| f.instrumentable).count();
        let skipped_count = functions.iter().filter(|f| !f.instrumentable).count();

        assert!(
            instrumentable_count >= 1 || skipped_count >= 1,
            "should analyze functions in the large fixture, found {} instrumentable + {} skipped",
            instrumentable_count,
            skipped_count
        );
    }
}
