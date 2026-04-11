/// CRUST-Bench integration: evaluate Noricum against the CRUST-Bench dataset.
///
/// Operates in **interface-aware mode**: reads C source from CBench/, reads Rust
/// interface skeletons (with `unimplemented!()`) from RBench/, fills in
/// implementations via LLM, and validates with `cargo test`.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use noricum_agents::extract_rust_code;
use noricum_agents::providers::{self, LlmClient, ProviderConfig};
use noricum_core::MigrationConfig;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// System prompts loaded at compile time.
const TRANSLATION_PREAMBLE: &str = include_str!("../../../prompts/crust_bench_translation.md");
const REPAIR_PREAMBLE: &str = include_str!("../../../prompts/crust_bench_repair.md");

/// Base repair iterations for simple (single-module) projects.
const BASE_REPAIR_ITERATIONS: u32 = 5;

/// Maximum repair iterations for complex multi-module projects.
const MAX_REPAIR_ITERATIONS: u32 = 10;

/// Temperature step per repair iteration (slower ramp = more stable repairs).
const REPAIR_TEMP_STEP: f64 = 0.10;

/// Compute the effective max repair iterations based on project complexity.
/// Multi-module projects get more iterations since failures are often localized.
fn effective_max_repairs(interface_count: usize, c_loc: u32) -> u32 {
    let module_bonus = (interface_count / 3) as u32;
    let size_bonus = if c_loc > 2000 {
        2
    } else if c_loc > 1000 {
        1
    } else {
        0
    };
    (BASE_REPAIR_ITERATIONS + module_bonus + size_bonus).min(MAX_REPAIR_ITERATIONS)
}

/// Configuration for CRUST-Bench evaluation.
pub struct CrustBenchConfig {
    pub dataset_path: PathBuf,
    pub filter: Option<String>,
    pub limit: Option<usize>,
    pub migration_config: MigrationConfig,
}

/// Result for a single project in CRUST-Bench.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectResult {
    pub name: String,
    pub c_loc: u32,
    pub rust_loc: u32,
    pub compilation_success: bool,
    pub tests_passed: bool,
    pub test_output: Option<String>,
    pub idiomatic_score_avg: f64,
    pub unsafe_count: u32,
    pub repair_iterations: u32,
    pub llm_calls: u32,
    pub total_ms: u64,
    pub error: Option<String>,
}

/// Aggregate report for all projects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrustBenchReport {
    pub total_projects: usize,
    pub compilation_rate: f64,
    pub test_pass_rate: f64,
    pub avg_idiomatic_score: f64,
    pub total_llm_calls: u32,
    pub total_repair_iterations: u32,
    pub projects: Vec<ProjectResult>,
}

/// A discovered CRUST-Bench project with both C and Rust paths.
struct CrustProject {
    name: String,
    cbench_dir: PathBuf,
    rbench_dir: PathBuf,
}

/// Discover matching projects in CBench/ and RBench/ directories.
fn discover_projects(dataset_path: &Path) -> Result<Vec<CrustProject>> {
    let cbench = dataset_path.join("CBench");
    let rbench = dataset_path.join("RBench");

    if !cbench.is_dir() {
        anyhow::bail!("CBench directory not found at {}", cbench.display());
    }
    if !rbench.is_dir() {
        anyhow::bail!("RBench directory not found at {}", rbench.display());
    }

    let mut rbench_names: std::collections::HashMap<String, PathBuf> =
        std::collections::HashMap::new();
    for entry in std::fs::read_dir(&rbench)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            rbench_names.insert(name, path);
        }
    }

    let mut projects = Vec::new();
    for entry in std::fs::read_dir(&cbench)? {
        let entry = entry?;
        let cb_path = entry.path();
        if !cb_path.is_dir() {
            continue;
        }
        let cb_name = entry.file_name().to_string_lossy().to_string();
        let rb_name = cb_name.replace('-', "_");
        if let Some(rb_path) = rbench_names
            .get(&cb_name)
            .or_else(|| rbench_names.get(&rb_name))
        {
            projects.push(CrustProject {
                name: cb_name,
                cbench_dir: cb_path,
                rbench_dir: rb_path.clone(),
            });
        } else {
            debug!(project = %cb_name, "no matching RBench project found, skipping");
        }
    }

    projects.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(projects)
}

fn read_c_sources(cbench_dir: &Path) -> Result<String> {
    let mut sources = Vec::new();
    collect_c_files(cbench_dir, &mut sources)?;
    sources.sort();

    let mut combined = String::new();
    for path in &sources {
        let name = path.strip_prefix(cbench_dir).unwrap_or(path);
        let content =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        combined.push_str(&format!("// === {} ===\n", name.display()));
        combined.push_str(&content);
        combined.push_str("\n\n");
    }
    Ok(combined)
}

fn collect_c_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_c_files(&path, out)?;
        } else if let Some(ext) = path.extension()
            && (ext == "c" || ext == "h")
        {
            out.push(path);
        }
    }
    Ok(())
}

fn read_interface_skeletons(rbench_dir: &Path) -> Result<Vec<(PathBuf, String)>> {
    let interfaces_dir = rbench_dir.join("src").join("interfaces");
    if !interfaces_dir.is_dir() {
        anyhow::bail!(
            "interfaces directory not found: {}",
            interfaces_dir.display()
        );
    }

    let mut files = Vec::new();
    for entry in std::fs::read_dir(&interfaces_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "rs") {
            let content = std::fs::read_to_string(&path)?;
            files.push((path, content));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn copy_rbench_to_workdir(rbench_dir: &Path, workdir: &Path) -> Result<()> {
    copy_dir_recursive(rbench_dir, workdir)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn run_cargo_test(project_dir: &Path) -> (bool, String) {
    let result = std::process::Command::new("cargo")
        .arg("test")
        .arg("--")
        .arg("--test-threads=1")
        .current_dir(project_dir)
        .env("CARGO_TERM_COLOR", "never")
        .output();

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let combined = format!("{stderr}\n{stdout}");
            (output.status.success(), combined)
        }
        Err(e) => (false, format!("Failed to run cargo test: {e}")),
    }
}

/// Run cargo test per test binary, collecting per-binary pass/fail info.
/// Returns (all_pass, combined_output, per-binary failures).
fn run_cargo_test_per_binary(project_dir: &Path) -> (bool, String, Vec<TestBinaryResult>) {
    let result = std::process::Command::new("cargo")
        .args(["test", "--no-run", "--message-format=json"])
        .current_dir(project_dir)
        .env("CARGO_TERM_COLOR", "never")
        .output();

    let test_binaries: Vec<String> = match &result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout
                .lines()
                .filter_map(|line| {
                    let v: serde_json::Value = serde_json::from_str(line).ok()?;
                    if v.get("reason")?.as_str()? == "compiler-artifact"
                        && v.get("profile")?.get("test")?.as_bool()?
                    {
                        let exec = v.get("executable")?.as_str()?;
                        Some(exec.to_string())
                    } else {
                        None
                    }
                })
                .collect()
        }
        Err(_) => Vec::new(),
    };

    // Fallback: if we can't discover individual binaries, run all at once
    if test_binaries.is_empty() {
        let (ok, output) = run_cargo_test(project_dir);
        return (ok, output, Vec::new());
    }

    let mut all_pass = true;
    let mut combined_output = String::new();
    let mut binary_results = Vec::new();

    for bin_path in &test_binaries {
        let bin_name = Path::new(bin_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let res = std::process::Command::new(bin_path)
            .args(["--test-threads=1"])
            .current_dir(project_dir)
            .env("CARGO_TERM_COLOR", "never")
            .output();

        match res {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let bin_output = format!("--- {bin_name} ---\n{stderr}\n{stdout}\n");
                let passed = output.status.success();

                if !passed {
                    all_pass = false;
                }

                binary_results.push(TestBinaryResult {
                    name: bin_name,
                    passed,
                    output: bin_output.clone(),
                });
                combined_output.push_str(&bin_output);
            }
            Err(e) => {
                all_pass = false;
                let msg = format!("--- {bin_name} ---\nFailed to run: {e}\n");
                binary_results.push(TestBinaryResult {
                    name: bin_name,
                    passed: false,
                    output: msg.clone(),
                });
                combined_output.push_str(&msg);
            }
        }
    }

    (all_pass, combined_output, binary_results)
}

/// Result from running a single test binary.
#[derive(Debug, Clone)]
struct TestBinaryResult {
    name: String,
    passed: bool,
    output: String,
}

fn run_cargo_build(project_dir: &Path) -> (bool, String) {
    let result = std::process::Command::new("cargo")
        .arg("build")
        .current_dir(project_dir)
        .env("CARGO_TERM_COLOR", "never")
        .output();

    match result {
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            (output.status.success(), stderr)
        }
        Err(e) => (false, format!("Failed to run cargo build: {e}")),
    }
}

fn count_lines(s: &str) -> u32 {
    s.lines().count() as u32
}

fn count_unsafe(rust_source: &str) -> u32 {
    noricum_tools::compiler::count_unsafe_blocks(rust_source)
}

/// Build a translation prompt for a single module (per-module mode).
/// `target` is the interface file to implement; `other_interfaces` provides
/// the signatures of sibling modules as read-only context.
fn build_single_module_prompt(
    c_source: &str,
    target: &(PathBuf, String),
    other_interfaces: &[(PathBuf, String)],
) -> String {
    let mut prompt = String::new();

    prompt.push_str("## C Source Code\n<c_source>\n");
    prompt.push_str(c_source);
    prompt.push_str("\n</c_source>\n\n");

    let filename = target.0.file_name().unwrap_or_default().to_string_lossy();
    prompt.push_str(&format!(
        "## Target: implement {filename}\n\
         Replace every `unimplemented!()` with a correct implementation. \
         Do NOT change any function signatures, struct definitions, or field types.\n\n\
         <rust_interface>\n{}\n</rust_interface>\n\n",
        target.1
    ));

    if !other_interfaces.is_empty() {
        prompt.push_str(
            "## Other module interfaces (read-only context — do NOT implement these, \
             but you may call their public functions):\n",
        );
        for (path, content) in other_interfaces {
            let other_name = path.file_name().unwrap_or_default().to_string_lossy();
            prompt.push_str(&format!(
                "### {other_name}\n<rust_interface>\n{content}\n</rust_interface>\n\n"
            ));
        }
    }

    prompt.push_str(&format!(
        "Output ONLY the complete Rust implementation for `{filename}`. No explanations."
    ));

    prompt
}

fn build_translation_prompt(c_source: &str, interface_skeletons: &[(PathBuf, String)]) -> String {
    let mut prompt = String::new();

    prompt.push_str("## C Source Code\n<c_source>\n");
    prompt.push_str(c_source);
    prompt.push_str("\n</c_source>\n\n");

    prompt.push_str("## Rust Interface Skeletons\n");
    prompt.push_str(
        "Replace every `unimplemented!()` with a correct implementation. \
         Do NOT change any function signatures, struct definitions, or field types.\n\n",
    );

    for (path, content) in interface_skeletons {
        let filename = path.file_name().unwrap_or_default().to_string_lossy();
        prompt.push_str(&format!(
            "### File: {filename}\n<rust_interface>\n{content}\n</rust_interface>\n\n"
        ));
    }

    prompt.push_str(
        "Output the complete implementation for each interface file. \
         If there are multiple files, separate them with `// === filename.rs ===` headers.\n\
         Output ONLY Rust code.",
    );

    prompt
}

/// Select model for a single module based on its interface LOC,
/// using a cheaper model when the module is small.
/// Respects the `--provider` flag for model selection.
fn select_module_model(interface_loc: u32, config: &MigrationConfig) -> String {
    if config.ollama_model.is_some() {
        return config
            .ollama_model
            .as_deref()
            .unwrap_or("qwen2.5-coder:32b")
            .to_string();
    }
    let provider = config.primary_provider.as_deref().unwrap_or("anthropic");
    match provider {
        "deepseek" => "deepseek-chat".to_string(),
        _ => match interface_loc {
            0..=100 => "claude-haiku-4-5-20251001".to_string(),
            101..=500 => "claude-sonnet-4-6".to_string(),
            _ => "claude-opus-4-6".to_string(),
        },
    }
}

fn build_repair_prompt(
    c_source: &str,
    interface_skeletons: &[(PathBuf, String)],
    current_impl: &str,
    errors: &str,
    iteration: u32,
    max_iterations: u32,
) -> String {
    let mut prompt = String::new();

    prompt.push_str(&format!(
        "## Repair iteration {iteration}/{max_iterations}\n\n"
    ));

    prompt.push_str("## Error output\n```\n");
    let truncated = if errors.len() > 8000 {
        &errors[..8000]
    } else {
        errors
    };
    prompt.push_str(truncated);
    prompt.push_str("\n```\n\n");

    prompt.push_str("## Current Rust implementation\n```rust\n");
    prompt.push_str(current_impl);
    prompt.push_str("\n```\n\n");

    prompt.push_str("## Original C source\n<c_source>\n");
    prompt.push_str(c_source);
    prompt.push_str("\n</c_source>\n\n");

    prompt.push_str("## Interface contract (signatures MUST NOT change)\n");
    for (path, content) in interface_skeletons {
        let filename = path.file_name().unwrap_or_default().to_string_lossy();
        prompt.push_str(&format!(
            "<rust_interface>\n// {filename}\n{content}\n</rust_interface>\n\n"
        ));
    }

    prompt.push_str("Fix ALL errors. Output the complete corrected Rust file. No explanations.");

    prompt
}

/// Build a targeted repair prompt that focuses on specific failing test binaries.
/// This gives the LLM more focused feedback instead of the full cargo test output.
fn build_targeted_repair_prompt(
    c_source: &str,
    interface_skeletons: &[(PathBuf, String)],
    current_impl: &str,
    failed_tests: &[TestBinaryResult],
    iteration: u32,
    max_iterations: u32,
) -> String {
    let mut prompt = String::new();

    prompt.push_str(&format!(
        "## Repair iteration {iteration}/{max_iterations}\n\n"
    ));

    prompt.push_str(&format!(
        "## Failing tests ({} of {} binaries failed)\n",
        failed_tests.len(),
        failed_tests.len() // We only pass the failed ones
    ));

    for fail in failed_tests {
        let truncated = if fail.output.len() > 3000 {
            &fail.output[..3000]
        } else {
            &fail.output
        };
        prompt.push_str(&format!(
            "### Test binary: {}\n```\n{}\n```\n\n",
            fail.name, truncated
        ));
    }

    prompt.push_str("## Current Rust implementation\n```rust\n");
    prompt.push_str(current_impl);
    prompt.push_str("\n```\n\n");

    prompt.push_str("## Original C source\n<c_source>\n");
    prompt.push_str(c_source);
    prompt.push_str("\n</c_source>\n\n");

    prompt.push_str("## Interface contract (signatures MUST NOT change)\n");
    for (path, content) in interface_skeletons {
        let filename = path.file_name().unwrap_or_default().to_string_lossy();
        prompt.push_str(&format!(
            "<rust_interface>\n// {filename}\n{content}\n</rust_interface>\n\n"
        ));
    }

    prompt.push_str(
        "Focus on fixing the specific failing test binaries listed above. \
         Analyze the test names and error messages to identify which module(s) need repair. \
         Fix ALL errors. Output the complete corrected Rust file. No explanations.",
    );

    prompt
}

fn parse_multi_file_output(
    output: &str,
    interface_files: &[(PathBuf, String)],
) -> Vec<(PathBuf, String)> {
    let code = extract_rust_code(output);

    if interface_files.len() == 1 {
        return vec![(interface_files[0].0.clone(), code)];
    }

    let mut results = Vec::new();
    let mut current_file: Option<&Path> = None;
    let mut current_content = String::new();

    for line in code.lines() {
        if line.starts_with("// === ") && line.ends_with(" ===") {
            if let Some(path) = current_file {
                results.push((path.to_path_buf(), current_content.trim().to_string()));
                current_content.clear();
            }
            let filename = line
                .trim_start_matches("// === ")
                .trim_end_matches(" ===")
                .trim();
            current_file = interface_files
                .iter()
                .find(|(p, _)| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().as_ref() == filename)
                        .unwrap_or(false)
                })
                .map(|(p, _)| p.as_path());
        } else {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    if let Some(path) = current_file {
        results.push((path.to_path_buf(), current_content.trim().to_string()));
    }

    if results.is_empty() && !interface_files.is_empty() {
        results.push((interface_files[0].0.clone(), code));
    }

    results
}

/// Read all current implementation files from disk to rebuild current_impl.
fn read_current_impls(workdir: &Path, interface_files: &[(PathBuf, String)]) -> String {
    let mut combined = String::new();
    for (iface_path, _) in interface_files {
        let filename = iface_path.file_name().unwrap_or_default();
        let src_path = workdir.join("src").join(filename);
        if let Ok(content) = std::fs::read_to_string(&src_path) {
            if !combined.is_empty() {
                combined.push_str("\n\n");
            }
            combined.push_str(&format!("// === {} ===\n", filename.to_string_lossy()));
            combined.push_str(&content);
        }
    }
    combined
}

fn select_model(c_loc: u32, config: &MigrationConfig) -> String {
    if config.ollama_model.is_some() {
        return config
            .ollama_model
            .as_deref()
            .unwrap_or("qwen2.5-coder:32b")
            .to_string();
    }
    // Respect --provider flag for model selection
    let provider = config.primary_provider.as_deref().unwrap_or("anthropic");
    match provider {
        "deepseek" => "deepseek-chat".to_string(),
        _ => match c_loc {
            0..=300 => "claude-haiku-4-5-20251001".to_string(),
            301..=1500 => "claude-sonnet-4-6".to_string(),
            _ => "claude-opus-4-6".to_string(),
        },
    }
}

/// Write parsed files to workdir and return updated current_impl from disk.
fn write_and_read_impls(
    workdir: &Path,
    parsed: &[(PathBuf, String)],
    interface_files: &[(PathBuf, String)],
) -> String {
    for (orig_path, content) in parsed {
        let filename = orig_path.file_name().unwrap_or_default();
        let dest = workdir.join("src").join(filename);
        let _ = std::fs::write(&dest, content);
    }
    read_current_impls(workdir, interface_files)
}

fn make_error_result(
    name: String,
    c_loc: u32,
    start: std::time::Instant,
    llm_calls: u32,
    error: String,
) -> ProjectResult {
    ProjectResult {
        name,
        c_loc,
        rust_loc: 0,
        compilation_success: false,
        tests_passed: false,
        test_output: None,
        idiomatic_score_avg: 0.0,
        unsafe_count: 0,
        repair_iterations: 0,
        llm_calls,
        total_ms: start.elapsed().as_millis() as u64,
        error: Some(error),
    }
}

async fn run_project(project: &CrustProject, config: &MigrationConfig) -> ProjectResult {
    let start = std::time::Instant::now();
    let name = project.name.clone();

    info!(project = %name, "starting CRUST-Bench project");

    // Step 1: Read C sources
    let c_source = match read_c_sources(&project.cbench_dir) {
        Ok(s) => s,
        Err(e) => return make_error_result(name, 0, start, 0, e.to_string()),
    };
    let c_loc = count_lines(&c_source);

    // Step 2: Read interface skeletons
    let interface_files = match read_interface_skeletons(&project.rbench_dir) {
        Ok(f) => f,
        Err(e) => return make_error_result(name, c_loc, start, 0, e.to_string()),
    };

    // Step 3: Create working copy of RBench project
    let workdir = std::env::temp_dir().join("noricum-crust-bench").join(&name);
    if workdir.exists() {
        let _ = std::fs::remove_dir_all(&workdir);
    }
    if let Err(e) = copy_rbench_to_workdir(&project.rbench_dir, &workdir) {
        return make_error_result(name, c_loc, start, 0, format!("copy failed: {e}"));
    }

    // Step 4: Create LLM client
    let client = match create_llm_client(config) {
        Ok(c) => c,
        Err(e) => return make_error_result(name, c_loc, start, 0, format!("no LLM: {e}")),
    };

    let max_repairs = effective_max_repairs(interface_files.len(), c_loc);
    let use_per_module = interface_files.len() > 1;
    let mut llm_calls = 0u32;
    let mut repair_iterations = 0u32;

    // Step 5: Translation — per-module for multi-file projects, single-shot for single-file
    if use_per_module {
        info!(
            project = %name,
            c_loc,
            interfaces = interface_files.len(),
            "translating per-module (cost-optimized)"
        );
        for (idx, target) in interface_files.iter().enumerate() {
            let others: Vec<_> = interface_files
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != idx)
                .map(|(_, f)| f.clone())
                .collect();

            let iface_loc = count_lines(&target.1);
            let module_model = select_module_model(iface_loc, config);
            let module_prompt = build_single_module_prompt(&c_source, target, &others);
            let module_tokens = ((target.1.len() as u64 / 4) * 3).clamp(4096, 32768);
            let module_name = target.0.file_name().unwrap_or_default().to_string_lossy();

            info!(
                project = %name,
                module = %module_name,
                model = %module_model,
                iface_loc,
                progress = format!("[{}/{}]", idx + 1, interface_files.len()),
                "translating module"
            );

            match client
                .run_prompt(
                    &module_model,
                    TRANSLATION_PREAMBLE,
                    0.3,
                    module_tokens,
                    &module_prompt,
                )
                .await
            {
                Ok(response) => {
                    llm_calls += 1;
                    let code = extract_rust_code(&response);
                    let dest = workdir.join("src").join(module_name.as_ref());
                    let _ = std::fs::write(&dest, &code);
                }
                Err(e) => {
                    llm_calls += 1;
                    warn!(project = %name, module = %module_name, error = %e, "module translation failed");
                }
            }
        }
    } else {
        let model = select_model(c_loc, config);
        let user_prompt = build_translation_prompt(&c_source, &interface_files);
        let max_tokens = ((c_source.len() as u64 / 4) * 3).clamp(8192, 65536);

        info!(project = %name, model = %model, c_loc, interfaces = interface_files.len(), "translating");

        match client
            .run_prompt(&model, TRANSLATION_PREAMBLE, 0.3, max_tokens, &user_prompt)
            .await
        {
            Ok(translation) => {
                llm_calls += 1;
                let parsed_files = parse_multi_file_output(&translation, &interface_files);
                write_and_read_impls(&workdir, &parsed_files, &interface_files);

                if parsed_files.is_empty() && !interface_files.is_empty() {
                    let code = extract_rust_code(&translation);
                    let filename = interface_files[0].0.file_name().unwrap_or_default();
                    let dest = workdir.join("src").join(filename);
                    let _ = std::fs::write(&dest, &code);
                }
            }
            Err(e) => {
                return make_error_result(
                    name,
                    c_loc,
                    start,
                    1,
                    format!("translation failed: {e}"),
                );
            }
        }
    }

    let mut current_impl = read_current_impls(&workdir, &interface_files);
    let repair_model = select_model(c_loc, config);
    let max_tokens = ((c_source.len() as u64 / 4) * 3).clamp(8192, 65536);

    // Step 6: Build check
    let (build_ok, build_errors) = run_cargo_build(&workdir);
    if !build_ok {
        info!(project = %name, "compilation failed, entering repair loop");
    }

    // Step 7: Build repair loop
    let mut last_errors = build_errors;
    let mut compilation_success = build_ok;

    if !build_ok {
        for iter in 1..=max_repairs {
            repair_iterations = iter;
            let temp = 0.3 + (iter as f64 - 1.0) * REPAIR_TEMP_STEP;

            let repair_prompt = build_repair_prompt(
                &c_source,
                &interface_files,
                &current_impl,
                &last_errors,
                iter,
                max_repairs,
            );

            info!(project = %name, iteration = iter, max = max_repairs, temp, "repair attempt (build)");

            match client
                .run_prompt(
                    &repair_model,
                    REPAIR_PREAMBLE,
                    temp,
                    max_tokens,
                    &repair_prompt,
                )
                .await
            {
                Ok(response) => {
                    llm_calls += 1;
                    let repaired = parse_multi_file_output(&response, &interface_files);
                    current_impl = write_and_read_impls(&workdir, &repaired, &interface_files);

                    let (ok, errors) = run_cargo_build(&workdir);
                    compilation_success = ok;
                    last_errors = errors;
                    if ok {
                        info!(project = %name, iteration = iter, "compilation fixed");
                        break;
                    }
                }
                Err(e) => {
                    llm_calls += 1;
                    warn!(project = %name, error = %e, "repair call failed");
                    break;
                }
            }
        }
    }

    // Step 8: Test phase — with per-test-binary feedback
    let mut tests_passed = false;
    let mut test_output = None;

    if compilation_success {
        let (test_ok, test_out, failed_binaries) = run_cargo_test_per_binary(&workdir);
        tests_passed = test_ok;

        if !test_ok {
            let failed_count = failed_binaries.iter().filter(|b| !b.passed).count();
            info!(
                project = %name,
                failed_binaries = failed_count,
                "tests failed, entering targeted repair loop"
            );
            test_output = Some(test_out.clone());

            let remaining_iters = max_repairs.saturating_sub(repair_iterations);
            for iter_offset in 1..=remaining_iters {
                let iter = repair_iterations + iter_offset;
                repair_iterations = iter;
                let temp = 0.3 + (iter as f64 - 1.0) * REPAIR_TEMP_STEP;

                // Use targeted repair if we have per-binary failure info
                let failed_only: Vec<_> = failed_binaries
                    .iter()
                    .filter(|b| !b.passed)
                    .cloned()
                    .collect();
                let repair_prompt = if !failed_only.is_empty() {
                    build_targeted_repair_prompt(
                        &c_source,
                        &interface_files,
                        &current_impl,
                        &failed_only,
                        iter,
                        max_repairs,
                    )
                } else {
                    build_repair_prompt(
                        &c_source,
                        &interface_files,
                        &current_impl,
                        &test_out,
                        iter,
                        max_repairs,
                    )
                };

                info!(project = %name, iteration = iter, max = max_repairs, temp, "repair attempt (tests)");

                match client
                    .run_prompt(
                        &repair_model,
                        REPAIR_PREAMBLE,
                        temp,
                        max_tokens,
                        &repair_prompt,
                    )
                    .await
                {
                    Ok(response) => {
                        llm_calls += 1;
                        let repaired = parse_multi_file_output(&response, &interface_files);
                        current_impl = write_and_read_impls(&workdir, &repaired, &interface_files);

                        let (bld_ok, bld_err) = run_cargo_build(&workdir);
                        if !bld_ok {
                            last_errors = bld_err;
                            compilation_success = false;
                            info!(project = %name, iteration = iter, "repair broke compilation");
                            continue;
                        }
                        compilation_success = true;

                        let (t_ok, t_out, _new_failures) = run_cargo_test_per_binary(&workdir);
                        tests_passed = t_ok;
                        test_output = Some(t_out.clone());
                        if t_ok {
                            info!(project = %name, iteration = iter, "tests fixed");
                            break;
                        }
                        last_errors = t_out;
                    }
                    Err(e) => {
                        llm_calls += 1;
                        warn!(project = %name, error = %e, "repair call failed");
                        break;
                    }
                }
            }
        } else {
            test_output = Some(test_out);
        }
    }

    // Step 9: Score
    let rust_loc = count_lines(&current_impl);
    let unsafe_count = count_unsafe(&current_impl);
    let idiomatic_score = if compilation_success {
        let base = 100.0_f64 - (unsafe_count as f64 * 10.0);
        if tests_passed {
            base.max(0.0)
        } else {
            (base * 0.5).max(0.0)
        }
    } else {
        0.0
    };

    let elapsed = start.elapsed().as_millis() as u64;
    let status = if tests_passed {
        "PASS"
    } else if compilation_success {
        "BUILD_OK"
    } else {
        "FAIL"
    };
    info!(project = %name, status, c_loc, rust_loc, unsafe_count, repair_iterations, llm_calls, elapsed_ms = elapsed, "project complete");

    ProjectResult {
        name,
        c_loc,
        rust_loc,
        compilation_success,
        tests_passed,
        test_output,
        idiomatic_score_avg: idiomatic_score,
        unsafe_count,
        repair_iterations,
        llm_calls,
        total_ms: elapsed,
        error: if compilation_success {
            None
        } else {
            Some(last_errors)
        },
    }
}

fn create_llm_client(config: &MigrationConfig) -> Result<LlmClient> {
    let provider_config = ProviderConfig {
        primary_provider: config
            .primary_provider
            .clone()
            .unwrap_or_else(|| "auto".to_string()),
        anthropic_api_key: config.anthropic_api_key.clone(),
        deepseek_api_key: config.deepseek_api_key.clone(),
        ollama_url: "http://localhost:11434".to_string(),
        ollama_model: config
            .ollama_model
            .clone()
            .unwrap_or_else(|| "qwen2.5-coder:32b".to_string()),
        ollama_num_ctx: config.ollama_num_ctx.unwrap_or(131072),
    };
    providers::create_llm_client(&provider_config)
        .map_err(|e| anyhow::anyhow!("failed to create LLM client: {e}"))
}

pub async fn run_crust_bench(config: &CrustBenchConfig) -> Result<CrustBenchReport> {
    let mut projects = discover_projects(&config.dataset_path)?;

    if let Some(ref filter) = config.filter {
        projects.retain(|p| p.name.contains(filter.as_str()));
    }
    if let Some(limit) = config.limit {
        projects.truncate(limit);
    }

    info!(
        total = projects.len(),
        filter = ?config.filter,
        limit = ?config.limit,
        "starting CRUST-Bench evaluation (interface-aware mode)"
    );

    let mut results = Vec::new();
    for (i, project) in projects.iter().enumerate() {
        info!(project = %project.name, progress = format!("[{}/{}]", i + 1, projects.len()), "starting project");
        let result = run_project(project, &config.migration_config).await;
        let status = if result.tests_passed {
            "PASS"
        } else if result.compilation_success {
            "BUILD_OK"
        } else {
            "FAIL"
        };
        info!(project = %result.name, status, score = format!("{:.0}", result.idiomatic_score_avg), llm_calls = result.llm_calls, time_ms = result.total_ms, progress = format!("[{}/{}]", i + 1, projects.len()), "completed project");
        results.push(result);
    }

    let total = results.len();
    let compiled = results.iter().filter(|r| r.compilation_success).count();
    let passed = results.iter().filter(|r| r.tests_passed).count();
    let avg_score = if total > 0 {
        results.iter().map(|r| r.idiomatic_score_avg).sum::<f64>() / total as f64
    } else {
        0.0
    };
    let total_llm_calls: u32 = results.iter().map(|r| r.llm_calls).sum();
    let total_repair_iterations: u32 = results.iter().map(|r| r.repair_iterations).sum();

    info!(
        total,
        compiled, passed, total_llm_calls, "CRUST-Bench evaluation complete"
    );

    Ok(CrustBenchReport {
        total_projects: total,
        compilation_rate: if total > 0 {
            compiled as f64 / total as f64
        } else {
            0.0
        },
        test_pass_rate: if total > 0 {
            passed as f64 / total as f64
        } else {
            0.0
        },
        avg_idiomatic_score: avg_score,
        total_llm_calls,
        total_repair_iterations,
        projects: results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_projects_empty() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("CBench")).unwrap();
        std::fs::create_dir(tmp.path().join("RBench")).unwrap();
        let projects = discover_projects(tmp.path()).unwrap();
        assert!(projects.is_empty());
    }

    #[test]
    fn test_discover_projects_matching() {
        let tmp = tempfile::tempdir().unwrap();
        let cb = tmp.path().join("CBench").join("my-project");
        let rb = tmp.path().join("RBench").join("my_project");
        std::fs::create_dir_all(&cb).unwrap();
        std::fs::create_dir_all(&rb).unwrap();
        std::fs::write(cb.join("main.c"), "int main() {}").unwrap();
        let projects = discover_projects(tmp.path()).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "my-project");
    }

    #[test]
    fn test_parse_multi_file_single() {
        let files = vec![(PathBuf::from("src/interfaces/foo.rs"), String::new())];
        let output = "fn foo() -> i32 { 42 }";
        let parsed = parse_multi_file_output(output, &files);
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].1.contains("fn foo()"));
    }

    #[test]
    fn test_parse_multi_file_headers() {
        let files = vec![
            (PathBuf::from("src/interfaces/a.rs"), String::new()),
            (PathBuf::from("src/interfaces/b.rs"), String::new()),
        ];
        let output = "// === a.rs ===\nfn a() {}\n// === b.rs ===\nfn b() {}";
        let parsed = parse_multi_file_output(output, &files);
        assert_eq!(parsed.len(), 2);
        assert!(parsed[0].1.contains("fn a()"));
        assert!(parsed[1].1.contains("fn b()"));
    }

    #[test]
    fn test_effective_max_repairs_single_module() {
        assert_eq!(effective_max_repairs(1, 200), 5);
    }

    #[test]
    fn test_effective_max_repairs_multi_module() {
        // 6 interfaces -> bonus of 2, 2500 LOC -> bonus of 2 => 5+2+2=9
        assert_eq!(effective_max_repairs(6, 2500), 9);
    }

    #[test]
    fn test_effective_max_repairs_capped() {
        // Very large project: should cap at MAX_REPAIR_ITERATIONS
        assert_eq!(effective_max_repairs(30, 5000), MAX_REPAIR_ITERATIONS);
    }

    #[test]
    fn test_build_single_module_prompt() {
        let target = (
            PathBuf::from("src/interfaces/foo.rs"),
            "fn foo() { unimplemented!() }".to_string(),
        );
        let others = vec![(
            PathBuf::from("src/interfaces/bar.rs"),
            "fn bar() -> i32 { 42 }".to_string(),
        )];
        let prompt = build_single_module_prompt("int foo() { return 1; }", &target, &others);
        assert!(prompt.contains("Target: implement foo.rs"));
        assert!(prompt.contains("Other module interfaces"));
        assert!(prompt.contains("bar.rs"));
    }

    #[test]
    fn test_select_module_model_haiku() {
        let config = MigrationConfig::default();
        let model = select_module_model(50, &config);
        assert!(
            model.contains("haiku"),
            "small module should use Haiku: {model}"
        );
    }

    #[test]
    fn test_select_module_model_sonnet() {
        let config = MigrationConfig::default();
        let model = select_module_model(200, &config);
        assert!(
            model.contains("sonnet"),
            "medium module should use Sonnet: {model}"
        );
    }

    #[test]
    fn test_select_module_model_deepseek() {
        let config = MigrationConfig {
            primary_provider: Some("deepseek".to_string()),
            ..Default::default()
        };
        // All sizes should route to deepseek-chat
        assert_eq!(select_module_model(50, &config), "deepseek-chat");
        assert_eq!(select_module_model(200, &config), "deepseek-chat");
        assert_eq!(select_module_model(600, &config), "deepseek-chat");
    }

    #[test]
    fn test_count_unsafe() {
        assert_eq!(count_unsafe("fn safe() {}"), 0);
        assert_eq!(count_unsafe("unsafe fn foo() {}"), 1);
        assert_eq!(count_unsafe("unsafe { ptr::read(x) } unsafe { }"), 2);
    }

    #[test]
    fn test_report_serialization() {
        let report = CrustBenchReport {
            total_projects: 1,
            compilation_rate: 1.0,
            test_pass_rate: 1.0,
            avg_idiomatic_score: 90.0,
            total_llm_calls: 3,
            total_repair_iterations: 1,
            projects: vec![ProjectResult {
                name: "test".to_string(),
                c_loc: 100,
                rust_loc: 80,
                compilation_success: true,
                tests_passed: true,
                test_output: None,
                idiomatic_score_avg: 90.0,
                unsafe_count: 0,
                repair_iterations: 0,
                llm_calls: 2,
                total_ms: 1000,
                error: None,
            }],
        };
        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("test"));
        let deser: CrustBenchReport = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.total_projects, 1);
    }
}
