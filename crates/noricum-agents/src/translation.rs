/// Translation Agent: converts C source code to safe, idiomatic Rust.
///
/// Takes the original C source, optional C2Rust mechanical translation output,
/// and the analysis results to produce high-quality Rust code.
/// When relevant patterns are available from the PatternStore, they are included
/// as few-shot examples in the prompt.
use noricum_ir::pattern_store::MigrationPattern;
use tracing::{debug, info};

use crate::AgentError;
use crate::analysis::AnalysisResult;
use crate::providers::LlmClient;

/// System prompt for the translation agent, loaded from the prompts directory at compile time.
const TRANSLATION_PREAMBLE: &str = include_str!("../../../prompts/translation.md");

/// Translate a C function to safe, idiomatic Rust.
///
/// Uses Claude API with the original C source, optional C2Rust output, prior
/// analysis, and relevant migration patterns (RAG) to produce the best possible
/// Rust translation.
pub async fn translate_function(
    client: &LlmClient,
    model: &str,
    c_source: &str,
    c2rust_output: Option<&str>,
    analysis: &AnalysisResult,
) -> Result<String, AgentError> {
    translate_function_with_patterns(client, model, c_source, c2rust_output, analysis, &[]).await
}

/// Translate with explicit pattern context (for testability and orchestrator integration).
pub async fn translate_function_with_patterns(
    client: &LlmClient,
    model: &str,
    c_source: &str,
    c2rust_output: Option<&str>,
    analysis: &AnalysisResult,
    patterns: &[&MigrationPattern],
) -> Result<String, AgentError> {
    translate_function_with_patterns_and_temperature(
        client,
        model,
        c_source,
        c2rust_output,
        analysis,
        patterns,
        None,
    )
    .await
}

/// Translate with pattern context and optional temperature override.
pub async fn translate_function_with_patterns_and_temperature(
    client: &LlmClient,
    model: &str,
    c_source: &str,
    c2rust_output: Option<&str>,
    analysis: &AnalysisResult,
    patterns: &[&MigrationPattern],
    temperature: Option<f64>,
) -> Result<String, AgentError> {
    let temp = temperature.unwrap_or(0.3);
    info!(
        model,
        difficulty = %analysis.difficulty,
        pattern_count = patterns.len(),
        temperature = temp,
        "starting translation"
    );

    let analysis_json = serde_json::to_string_pretty(analysis)
        .map_err(|e| AgentError::Provider(format!("failed to serialize analysis: {e}")))?;

    let mut user_message = String::new();

    // For very large files (>2000 LOC), prepend a structural summary
    // to help the LLM understand the codebase before translating.
    let c_lines = c_source.lines().count();
    if c_lines > 2000 {
        let summary = build_structural_summary(c_source);
        user_message.push_str(&format!(
            "## Structural summary ({c_lines} lines)\n{summary}\n\n"
        ));
    }

    user_message.push_str(&format!(
        "## Original C source\n<c_source>\n{c_source}\n</c_source>\n\n\
         ## Analysis\n```json\n{analysis_json}\n```\n"
    ));

    if let Some(c2rust) = c2rust_output {
        user_message.push_str(&format!(
            "\n## C2Rust output (unsafe Rust)\n```rust\n{c2rust}\n```\n"
        ));
    }

    if !patterns.is_empty() {
        user_message
            .push_str("\n## Relevant migration patterns (examples from past translations)\n");
        for pattern in patterns {
            user_message.push_str(&format!(
                "\n### Pattern: {}\nC:\n```c\n{}\n```\nRust:\n```rust\n{}\n```\n",
                pattern.name, pattern.c_pattern, pattern.rust_pattern
            ));
        }
    }

    user_message
        .push_str("\nTranslate the C function to safe, idiomatic Rust. Output ONLY the Rust code.");

    // Scale max_tokens based on C source size: Rust output is typically 1.5x C input.
    // Estimate ~4 chars per token, multiply by 2 for headroom.
    let max_tokens = ((c_source.len() as u64 / 4) * 2).clamp(8192, 65536);

    debug!(max_tokens, "sending translation prompt to LLM");

    let response = client
        .run_prompt(model, TRANSLATION_PREAMBLE, temp, max_tokens, &user_message)
        .await?;

    debug!(
        response_len = response.len(),
        "received translation response"
    );

    Ok(crate::extract_rust_code(&response))
}

/// Translate a large C file in multiple passes (one per chunk).
///
/// Each chunk is translated independently with shared context (types, globals)
/// and accumulated Rust signatures from previously translated chunks.
/// The outputs are combined and `use` statements are deduplicated.
pub async fn translate_chunked(
    client: &LlmClient,
    model: &str,
    chunks: &[noricum_tools::ast::CChunk],
    c2rust_output: Option<&str>,
    analysis: &AnalysisResult,
    patterns: &[&MigrationPattern],
    temperature: Option<f64>,
) -> Result<String, AgentError> {
    info!(
        chunks = chunks.len(),
        model,
        "starting chunked multi-pass translation"
    );

    let mut accumulated_rust = Vec::new();
    let mut accumulated_sigs: Vec<String> = Vec::new();

    for (i, chunk) in chunks.iter().enumerate() {
        info!(
            chunk = i + 1,
            total = chunks.len(),
            functions = chunk.function_names.len(),
            lines = chunk.line_count,
            "translating chunk"
        );

        // Build a synthetic C source: shared context + this chunk's functions
        let mut chunk_source = chunk.shared_context.clone();
        chunk_source.push_str("\n\n// === Functions to translate ===\n");
        chunk_source.push_str(&chunk.functions_source);

        // If we have previously translated signatures, add them as context
        let mut chunk_c2rust = c2rust_output.map(|s| s.to_string());
        if !accumulated_sigs.is_empty() {
            let sig_context = format!(
                "\n\n// === Already translated Rust signatures (for reference) ===\n{}",
                accumulated_sigs.join("\n")
            );
            chunk_source.push_str(&sig_context);
            // Don't pass c2rust for later chunks (it's for the whole file, not chunks)
            if i > 0 {
                chunk_c2rust = None;
            }
        }

        let rust_code = translate_function_with_patterns_and_temperature(
            client,
            model,
            &chunk_source,
            chunk_c2rust.as_deref(),
            analysis,
            patterns,
            temperature,
        )
        .await?;

        // Extract signatures from this chunk's output for next chunk's context
        let new_sigs = noricum_tools::ast::extract_rust_signatures(&rust_code);
        accumulated_sigs.extend(new_sigs);

        accumulated_rust.push(rust_code);
    }

    // Combine all chunks, deduplicating `use` statements
    let mut use_statements = Vec::new();
    let mut code_parts = Vec::new();

    for part in &accumulated_rust {
        let mut code_lines = Vec::new();
        for line in part.lines() {
            if line.trim().starts_with("use ") {
                if !use_statements.contains(&line.trim().to_string()) {
                    use_statements.push(line.trim().to_string());
                }
            } else {
                code_lines.push(line);
            }
        }
        let code = code_lines.join("\n").trim().to_string();
        if !code.is_empty() {
            code_parts.push(code);
        }
    }

    let mut final_output = String::new();
    for stmt in &use_statements {
        final_output.push_str(stmt);
        final_output.push('\n');
    }
    if !use_statements.is_empty() {
        final_output.push('\n');
    }
    final_output.push_str(&code_parts.join("\n\n"));

    info!(
        total_len = final_output.len(),
        chunks = chunks.len(),
        "chunked translation complete"
    );

    Ok(final_output)
}

/// Build a condensed structural summary of a large C source file.
///
/// Extracts struct/enum/union declarations, typedefs, function signatures,
/// and global variables to give the LLM an architectural overview before
/// attempting full translation.
fn build_structural_summary(c_source: &str) -> String {
    let mut structs = Vec::new();
    let mut functions = Vec::new();
    let mut typedefs = Vec::new();
    let mut globals = Vec::new();

    let mut in_struct = false;
    let mut brace_depth: i32 = 0;

    for line in c_source.lines() {
        let trimmed = line.trim();

        // Track brace depth
        let opens = trimmed.chars().filter(|&c| c == '{').count() as i32;
        let closes = trimmed.chars().filter(|&c| c == '}').count() as i32;

        if in_struct {
            brace_depth += opens - closes;
            if brace_depth <= 0 {
                in_struct = false;
                brace_depth = 0;
            }
            continue;
        }

        // Struct/enum/union declarations
        if (trimmed.starts_with("struct ")
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("union "))
            && trimmed.contains('{')
        {
            structs.push(
                trimmed
                    .split('{')
                    .next()
                    .unwrap_or(trimmed)
                    .trim()
                    .to_string(),
            );
            in_struct = true;
            brace_depth = opens - closes;
            if brace_depth <= 0 {
                in_struct = false;
                brace_depth = 0;
            }
            continue;
        }

        // Typedefs
        if trimmed.starts_with("typedef") {
            typedefs.push(trimmed.to_string());
            continue;
        }

        // Function signatures (top-level, not inside structs)
        if brace_depth == 0
            && !trimmed.starts_with("//")
            && !trimmed.starts_with('#')
            && !trimmed.starts_with("typedef")
            && trimmed.contains('(')
            && (trimmed.ends_with('{') || trimmed.ends_with(')'))
            && !trimmed.starts_with("if")
            && !trimmed.starts_with("while")
            && !trimmed.starts_with("for")
        {
            let sig = trimmed.split('{').next().unwrap_or(trimmed).trim();
            if !sig.is_empty() {
                functions.push(sig.to_string());
            }
        }

        // Global variables (top-level assignments)
        if brace_depth == 0
            && !trimmed.starts_with("//")
            && !trimmed.starts_with('#')
            && trimmed.contains('=')
            && trimmed.ends_with(';')
            && !trimmed.contains('(')
        {
            globals.push(trimmed.to_string());
        }

        brace_depth += opens - closes;
        if brace_depth < 0 {
            brace_depth = 0;
        }
    }

    let mut summary = String::new();

    if !typedefs.is_empty() {
        summary.push_str(&format!("**Typedefs ({}):** ", typedefs.len()));
        summary.push_str(&typedefs.join("; "));
        summary.push('\n');
    }
    if !structs.is_empty() {
        summary.push_str(&format!("**Structs/Enums ({}):** ", structs.len()));
        summary.push_str(&structs.join(", "));
        summary.push('\n');
    }
    if !functions.is_empty() {
        summary.push_str(&format!("**Functions ({}):**\n", functions.len()));
        for f in &functions {
            summary.push_str(&format!("- `{f}`\n"));
        }
    }
    if !globals.is_empty() {
        summary.push_str(&format!("**Globals ({}):** ", globals.len()));
        summary.push_str(
            &globals
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join("; "),
        );
        if globals.len() > 10 {
            summary.push_str(&format!(" ... and {} more", globals.len() - 10));
        }
        summary.push('\n');
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_typedefs() {
        let source = "typedef int myint;\ntypedef unsigned long ulong;\nvoid f() {}\n";
        let summary = build_structural_summary(source);
        assert!(
            summary.contains("Typedefs (2)"),
            "should report 2 typedefs, got: {summary}"
        );
    }

    #[test]
    fn test_summary_structs() {
        let source = "\
struct Foo {
    int x;
};
enum Color {
    RED, GREEN, BLUE
};
void f() {}
";
        let summary = build_structural_summary(source);
        assert!(
            summary.contains("Structs/Enums (2)"),
            "should report 2 structs/enums, got: {summary}"
        );
    }

    #[test]
    fn test_summary_functions() {
        let source = "\
void foo(int a) {
    return;
}
int bar(int b) {
    return b;
}
double baz(double x) {
    return x;
}
";
        let summary = build_structural_summary(source);
        assert!(
            summary.contains("Functions (3)"),
            "should report 3 functions, got: {summary}"
        );
        assert!(summary.contains("foo"), "should contain foo");
        assert!(summary.contains("bar"), "should contain bar");
        assert!(summary.contains("baz"), "should contain baz");
    }

    #[test]
    fn test_summary_globals_truncate() {
        // Generate 15 globals
        let mut source = String::new();
        for i in 0..15 {
            source.push_str(&format!("int g{i} = {i};\n"));
        }
        source.push_str("void f() {}\n");
        let summary = build_structural_summary(&source);
        assert!(
            summary.contains("... and 5 more"),
            "should truncate globals beyond 10, got: {summary}"
        );
    }

    #[test]
    fn test_summary_empty() {
        let summary = build_structural_summary("");
        assert!(summary.is_empty(), "empty source should produce empty summary");
    }
}
