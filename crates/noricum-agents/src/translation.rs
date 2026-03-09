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

/// Output from a single chunk's translation.
#[derive(Debug, Clone)]
pub struct ChunkOutput {
    /// Chunk index (0-based).
    pub index: usize,
    /// Translated Rust source for this chunk.
    pub rust_source: String,
}

/// Result of chunked translation including per-chunk details.
#[derive(Debug, Clone)]
pub struct ChunkedTranslationResult {
    /// Combined Rust source from all chunks.
    pub combined: String,
    /// Individual chunk outputs (before combination).
    pub chunks: Vec<ChunkOutput>,
    /// P11 agreed signatures (if generated).
    pub agreed_signatures: Option<String>,
    /// P9 foundation context from chunk 0 (if generated).
    pub foundation: Option<String>,
}

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

    // For medium+ files (>800 LOC), prepend a structural summary
    // to help the LLM understand the codebase before translating.
    let c_lines = c_source.lines().count();
    if c_lines > 800 {
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
        // Cap c2rust context to avoid blowing the API token limit.
        // First ~3000 lines contain type definitions and struct mappings (most useful).
        const MAX_C2RUST_LINES: usize = 3000;
        let lines: Vec<&str> = c2rust.lines().collect();
        if lines.len() > MAX_C2RUST_LINES {
            let truncated: String = lines[..MAX_C2RUST_LINES].join("\n");
            info!(
                original_lines = lines.len(),
                kept_lines = MAX_C2RUST_LINES,
                "truncating c2rust output to fit token budget"
            );
            user_message.push_str(&format!(
                "\n## C2Rust output (unsafe Rust, truncated)\n```rust\n{truncated}\n// ... ({} more lines truncated)\n```\n",
                lines.len() - MAX_C2RUST_LINES
            ));
        } else {
            user_message.push_str(&format!(
                "\n## C2Rust output (unsafe Rust)\n```rust\n{c2rust}\n```\n"
            ));
        }
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
) -> Result<ChunkedTranslationResult, AgentError> {
    info!(
        chunks = chunks.len(),
        model, "starting chunked multi-pass translation"
    );

    let mut accumulated_rust = Vec::new();
    let mut accumulated_sigs: Vec<String> = Vec::new();
    // P9: Foundation context — chunk 0's output (types, data model) is passed to all
    // subsequent chunks so they can reference the translated Rust types.
    let mut foundation_rust: Option<String> = None;
    let mut chunk_outputs: Vec<ChunkOutput> = Vec::new();

    // P11: Signature agreement pass — for files with many chunks, generate agreed-upon
    // Rust signatures for all functions before translating bodies. This prevents
    // signature mismatches between chunks (e.g., chunk 2 calls a function from chunk 1
    // with the wrong parameter types).
    let mut agreed_signatures: Option<String> = None;
    if chunks.len() > 2 {
        // Collect all function signatures from all chunks
        let mut all_sigs = String::new();
        for chunk in chunks {
            if chunk.is_data_chunk {
                continue;
            }
            for line in chunk.functions_source.lines() {
                let trimmed = line.trim();
                // Capture C function signatures (lines with parens that look like declarations)
                if !trimmed.starts_with("//")
                    && !trimmed.starts_with('#')
                    && trimmed.contains('(')
                    && (trimmed.ends_with('{') || trimmed.ends_with(')') || trimmed.ends_with(");"))
                    && !trimmed.starts_with("if")
                    && !trimmed.starts_with("while")
                    && !trimmed.starts_with("for")
                    && !trimmed.starts_with("return")
                {
                    let sig = trimmed.split('{').next().unwrap_or(trimmed).trim();
                    if !sig.is_empty() && sig.contains('(') {
                        all_sigs.push_str(sig);
                        all_sigs.push('\n');
                    }
                }
            }
        }

        if !all_sigs.is_empty() {
            let sig_prompt = format!(
                "{}\n\n## C function signatures\n```c\n{}\n```\n\n\
                 Produce ONLY the Rust function signatures (fn declarations without bodies) for all functions above.\n\
                 Use idiomatic Rust types: &str instead of *const char, Vec<T> instead of *T + length, \
                 Option<T> for nullable pointers, &mut T for output pointers.\n\
                 Output ONLY the Rust signatures, one per line, no bodies, no explanation.",
                chunks[0].shared_context, all_sigs
            );

            match client
                .run_prompt(model, TRANSLATION_PREAMBLE, 0.2, 4096, &sig_prompt)
                .await
            {
                Ok(response) => {
                    let sigs = crate::extract_rust_code(&response);
                    if sigs.contains("fn ") {
                        info!(
                            sig_count = sigs.lines().filter(|l| l.contains("fn ")).count(),
                            "P11: generated agreed Rust signatures"
                        );
                        agreed_signatures = Some(sigs);
                    }
                }
                Err(e) => {
                    debug!(error = %e, "P11: signature agreement pass failed, continuing without");
                }
            }
        }
    }

    for (i, chunk) in chunks.iter().enumerate() {
        info!(
            chunk = i + 1,
            total = chunks.len(),
            functions = chunk.function_names.len(),
            lines = chunk.line_count,
            is_data = chunk.is_data_chunk,
            "translating chunk"
        );

        // P8: Data chunks get specialized transcription instructions
        if chunk.is_data_chunk {
            let mut data_source = chunk.shared_context.clone();
            data_source.push_str(
                "\n\n// === INSTRUCTION: This chunk contains ONLY static data (arrays, lookup tables). ===\n\
                 // Transcribe each C array to an equivalent Rust const array.\n\
                 // Use `const NAME: [[type; N]; M] = [...]` syntax.\n\
                 // Preserve all values exactly. Do NOT add functions.\n",
            );
            data_source.push_str("\n\n// === Data to transcribe ===\n");
            data_source.push_str(&chunk.functions_source);

            let rust_code = translate_function_with_patterns_and_temperature(
                client,
                model,
                &data_source,
                None,
                analysis,
                patterns,
                temperature,
            )
            .await?;
            chunk_outputs.push(ChunkOutput {
                index: i,
                rust_source: rust_code.clone(),
            });
            accumulated_rust.push(rust_code);
            continue;
        }

        // Build a synthetic C source: shared context + this chunk's functions
        let mut chunk_source = chunk.shared_context.clone();
        // For chunk 0 with type definitions, add data model translation guidance
        if i == 0 && chunk.shared_context.contains("struct ") {
            chunk_source.push_str(
                "\n\n// === INSTRUCTION: Translate data model to idiomatic Rust types FIRST. ===\n\
                 // Use enum variants instead of type tags. Convert linked-lists to Vec.\n\
                 // Use String instead of *char. Replace malloc/free with RAII.\n",
            );
        }
        chunk_source.push_str("\n\n// === Functions to translate ===\n");
        chunk_source.push_str(&chunk.functions_source);

        // P9: Include foundation context (chunk 0's types) for all subsequent chunks
        if i > 0
            && let Some(ref foundation) = foundation_rust
        {
            let type_lines: Vec<&str> = foundation
                .lines()
                .filter(|l| {
                    let t = l.trim();
                    t.starts_with("pub struct ")
                        || t.starts_with("struct ")
                        || t.starts_with("pub enum ")
                        || t.starts_with("enum ")
                        || t.starts_with("pub type ")
                        || t.starts_with("type ")
                        || t.starts_with("pub const ")
                        || t.starts_with("const ")
                        || t.starts_with("impl ")
                        || t.starts_with("pub fn new(")
                        || t.starts_with("    pub ")
                        || t == "}"
                        || t == "{"
                })
                .collect();
            if !type_lines.is_empty() {
                chunk_source.push_str(&format!(
                    "\n\n// === P9: Rust types from data model (already translated) ===\n{}",
                    type_lines.join("\n")
                ));
            }
        }

        // If we have previously translated signatures, add them as context
        if !accumulated_sigs.is_empty() {
            let sig_context = format!(
                "\n\n// === Already translated Rust signatures (for reference) ===\n{}",
                accumulated_sigs.join("\n")
            );
            chunk_source.push_str(&sig_context);
        }

        // P11: Include agreed-upon signatures so all chunks use consistent types
        if let Some(ref sigs) = agreed_signatures {
            chunk_source.push_str(&format!(
                "\n\n// === P11: Agreed Rust function signatures (use these exact types) ===\n{}",
                sigs
            ));
        }

        // P2: Per-function C2Rust context — extract only the c2rust functions
        // matching this chunk's function names, plus type definitions (first chunk).
        // This gives the LLM targeted reference without noise and saves tokens.
        let chunk_c2rust = c2rust_output.and_then(|s| {
            let extracted = extract_c2rust_for_chunk(s, &chunk.function_names, i == 0);
            if extracted.is_empty() {
                None
            } else {
                Some(extracted)
            }
        });

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

        // P3-fix: Per-chunk substance validation — detect empty stubs before accumulating.
        // If a chunk produces mostly empty functions, retry once with higher temperature.
        let chunk_c_lines = chunk
            .functions_source
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count();
        let chunk_rust_lines = rust_code.lines().filter(|l| !l.trim().is_empty()).count();
        let empty_fns = count_empty_fns_quick(&rust_code);
        let total_fns = count_total_fns_quick(&rust_code);
        let chunk_is_stub = (chunk_c_lines > 10 && chunk_rust_lines < chunk_c_lines / 4)
            || (total_fns > 2 && empty_fns as f32 / total_fns as f32 > 0.3);

        let rust_code = if chunk_is_stub {
            tracing::warn!(
                chunk = i + 1,
                chunk_c_lines,
                chunk_rust_lines,
                empty_fns,
                total_fns,
                "chunk produced empty stubs, retrying with temperature 0.5"
            );
            let retry = translate_function_with_patterns_and_temperature(
                client,
                model,
                &chunk_source,
                chunk_c2rust.as_deref(),
                analysis,
                patterns,
                Some(0.5),
            )
            .await?;
            let retry_lines = retry.lines().filter(|l| !l.trim().is_empty()).count();
            let retry_empty = count_empty_fns_quick(&retry);
            let retry_total = count_total_fns_quick(&retry);
            let still_stub = (chunk_c_lines > 10 && retry_lines < chunk_c_lines / 4)
                || (retry_total > 2 && retry_empty as f32 / retry_total as f32 > 0.3);
            if !still_stub {
                info!(chunk = i + 1, "chunk retry produced substantial code");
                retry
            } else {
                tracing::warn!(
                    chunk = i + 1,
                    "chunk retry still produced stubs, keeping original"
                );
                rust_code
            }
        } else {
            rust_code
        };

        // Capture per-chunk output before combination
        chunk_outputs.push(ChunkOutput {
            index: i,
            rust_source: rust_code.clone(),
        });

        // Extract signatures from this chunk's output for next chunk's context
        let new_sigs = noricum_tools::ast::extract_rust_signatures(&rust_code);
        accumulated_sigs.extend(new_sigs);

        // P9: Save chunk 0's output as foundation context for subsequent chunks
        if i == 0 && foundation_rust.is_none() {
            foundation_rust = Some(rust_code.clone());
        }

        accumulated_rust.push(rust_code);

        // P10: Incremental compilation check — combine accumulated chunks and verify
        // they compile together. This catches type mismatches early instead of at the end.
        if chunks.len() > 2 && i < chunks.len() - 1 {
            let partial = combine_accumulated_chunks(&accumulated_rust);
            match noricum_tools::compiler::check_rust_compiles(&partial) {
                Ok(result) if !result.success => {
                    let error_count = result
                        .stderr
                        .lines()
                        .filter(|l| l.contains("error"))
                        .count();
                    if error_count > 0 {
                        info!(
                            chunk = i + 1,
                            errors = error_count,
                            "P10: incremental compilation has errors (will resolve in later chunks or repair)"
                        );
                    }
                }
                Ok(_) => {
                    info!(chunk = i + 1, "P10: incremental compilation OK");
                }
                Err(e) => {
                    debug!(chunk = i + 1, error = %e, "P10: incremental compilation check failed");
                }
            }
        }
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

    Ok(ChunkedTranslationResult {
        combined: final_output,
        chunks: chunk_outputs,
        agreed_signatures,
        foundation: foundation_rust,
    })
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

/// P2: Extract only the c2rust functions matching a chunk's function names.
///
/// For chunk 0 (`include_types=true`), also includes struct/enum/type definitions.
/// Keeps the output under a reasonable size by extracting only relevant functions
/// instead of the entire c2rust output. Falls back to truncation for very large output.
fn extract_c2rust_for_chunk(
    c2rust_output: &str,
    function_names: &[String],
    include_types: bool,
) -> String {
    let lines: Vec<&str> = c2rust_output.lines().collect();
    let mut result = Vec::new();

    // Always include use/extern statements (first ~50 lines typically)
    let mut header_end = 0;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("use ")
            || trimmed.starts_with("extern ")
            || trimmed.starts_with("#[")
            || trimmed.starts_with("//")
            || trimmed.is_empty()
        {
            result.push(*line);
            header_end = i + 1;
        } else {
            break;
        }
    }

    // For chunk 0, include type definitions (struct, enum, type aliases)
    if include_types {
        let mut in_type_def = false;
        let mut brace_depth: i32 = 0;
        for line in &lines[header_end..] {
            let trimmed = line.trim();
            let is_type = trimmed.starts_with("pub struct ")
                || trimmed.starts_with("pub enum ")
                || trimmed.starts_with("pub type ")
                || trimmed.starts_with("pub union ")
                || trimmed.starts_with("struct ")
                || trimmed.starts_with("enum ")
                || trimmed.starts_with("type ")
                || trimmed.starts_with("#[repr(");

            if is_type || in_type_def {
                result.push(*line);
                let opens = trimmed.chars().filter(|&c| c == '{').count() as i32;
                let closes = trimmed.chars().filter(|&c| c == '}').count() as i32;
                brace_depth += opens - closes;
                in_type_def = brace_depth > 0;
            }
        }
    }

    // Extract functions matching this chunk's names
    let name_set: std::collections::HashSet<&str> =
        function_names.iter().map(|s| s.as_str()).collect();
    let mut in_function = false;
    let mut brace_depth: i32 = 0;
    let mut current_fn_lines: Vec<&str> = Vec::new();

    for line in &lines[header_end..] {
        let trimmed = line.trim();

        if !in_function {
            // Check if this line starts a function matching our names
            let is_matching_fn = (trimmed.starts_with("pub unsafe extern ")
                || trimmed.starts_with("pub extern ")
                || trimmed.starts_with("unsafe extern ")
                || trimmed.starts_with("pub fn ")
                || trimmed.starts_with("fn "))
                && name_set.iter().any(|name| {
                    // Match C function name to c2rust mangled name
                    let snake = to_snake_case(name);
                    trimmed.contains(&format!("fn {name}("))
                        || trimmed.contains(&format!("fn {snake}("))
                        || trimmed.contains(&format!("fn {name} ("))
                });

            if is_matching_fn {
                in_function = true;
                brace_depth = 0;
                current_fn_lines.clear();
            }
        }

        if in_function {
            current_fn_lines.push(line);
            let opens = trimmed.chars().filter(|&c| c == '{').count() as i32;
            let closes = trimmed.chars().filter(|&c| c == '}').count() as i32;
            brace_depth += opens - closes;

            if brace_depth <= 0 && current_fn_lines.len() > 1 {
                result.push(""); // blank line separator
                result.append(&mut current_fn_lines);
                in_function = false;
            }
        }
    }

    // Cap total output to avoid token overflow
    const MAX_C2RUST_LINES_CHUNK: usize = 1500;
    if result.len() > MAX_C2RUST_LINES_CHUNK {
        let kept: String = result[..MAX_C2RUST_LINES_CHUNK].join("\n");
        info!(
            original_lines = result.len(),
            kept_lines = MAX_C2RUST_LINES_CHUNK,
            "P2: truncating per-chunk c2rust context"
        );
        format!(
            "{}\n// ... ({} more lines truncated)",
            kept,
            result.len() - MAX_C2RUST_LINES_CHUNK
        )
    } else {
        result.join("\n")
    }
}

/// Convert a C identifier to snake_case for matching c2rust output.
fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_lowercase().next().unwrap_or(c));
    }
    result
}

/// Quick check for empty function bodies in Rust code (no external dependency).
///
/// Counts functions with bodies that are empty, contain only `todo!()`, or just `unimplemented!()`.
fn count_empty_fns_quick(rust_source: &str) -> usize {
    let mut count = 0;
    let lines: Vec<&str> = rust_source.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if (trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ")) && trimmed.contains('(') {
            // Look at the next few non-empty lines for empty body
            let mut body_content = String::new();
            for inner_line in lines.iter().skip(i + 1).take(4) {
                let inner = inner_line.trim();
                if inner == "{" || inner == "}" || inner.is_empty() {
                    continue;
                }
                body_content.push_str(inner);
                break;
            }
            if body_content.is_empty()
                || body_content == "todo!()"
                || body_content == "unimplemented!()"
                || (trimmed.ends_with("{}") || trimmed.ends_with("{ }"))
            {
                count += 1;
            }
        }
    }
    count
}

/// Quick count of function definitions in Rust code (no external dependency).
fn count_total_fns_quick(rust_source: &str) -> usize {
    rust_source
        .lines()
        .filter(|l| {
            let t = l.trim();
            (t.starts_with("fn ") || t.starts_with("pub fn ") || t.starts_with("pub(crate) fn "))
                && t.contains('(')
        })
        .count()
}

/// P10: Combine accumulated chunk outputs into a single Rust source for incremental compilation.
///
/// Deduplicates `use` statements and joins code parts — same logic as the final combination
/// but extracted for reuse during incremental checks.
fn combine_accumulated_chunks(chunks: &[String]) -> String {
    let mut use_statements = Vec::new();
    let mut code_parts = Vec::new();

    for part in chunks {
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

    let mut output = String::new();
    for stmt in &use_statements {
        output.push_str(stmt);
        output.push('\n');
    }
    if !use_statements.is_empty() {
        output.push('\n');
    }
    output.push_str(&code_parts.join("\n\n"));
    output
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
        assert!(
            summary.is_empty(),
            "empty source should produce empty summary"
        );
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("cJSON_Parse"), "c_j_s_o_n__parse");
        assert_eq!(to_snake_case("add"), "add");
        assert_eq!(to_snake_case("getValue"), "get_value");
    }

    #[test]
    fn test_extract_c2rust_for_chunk_matching_functions() {
        let c2rust = "\
use std::ffi;\n\
\n\
pub struct Foo {\n\
    pub x: i32,\n\
}\n\
\n\
pub unsafe extern \"C\" fn add(a: i32, b: i32) -> i32 {\n\
    return a + b;\n\
}\n\
\n\
pub unsafe extern \"C\" fn mul(a: i32, b: i32) -> i32 {\n\
    return a * b;\n\
}\n\
\n\
pub unsafe extern \"C\" fn sub(a: i32, b: i32) -> i32 {\n\
    return a - b;\n\
}\n";
        let names = vec!["add".to_string(), "sub".to_string()];
        let result = extract_c2rust_for_chunk(c2rust, &names, false);
        assert!(
            result.contains("fn add("),
            "should contain add, got: {result}"
        );
        assert!(
            result.contains("fn sub("),
            "should contain sub, got: {result}"
        );
        assert!(!result.contains("fn mul("), "should NOT contain mul");
    }

    #[test]
    fn test_extract_c2rust_for_chunk_with_types() {
        let c2rust = "\
use std::ffi;\n\
\n\
pub struct Foo {\n\
    pub x: i32,\n\
}\n\
\n\
pub unsafe extern \"C\" fn add(a: i32, b: i32) -> i32 {\n\
    return a + b;\n\
}\n";
        let names = vec!["add".to_string()];
        let result = extract_c2rust_for_chunk(c2rust, &names, true);
        assert!(
            result.contains("struct Foo"),
            "chunk 0 should include types"
        );
        assert!(
            result.contains("fn add("),
            "should contain matching function"
        );
    }

    #[test]
    fn test_extract_c2rust_for_chunk_empty_names() {
        let c2rust = "pub unsafe extern \"C\" fn add(a: i32, b: i32) -> i32 { a + b }\n";
        let names: Vec<String> = vec![];
        let result = extract_c2rust_for_chunk(c2rust, &names, false);
        // No matching functions, should return only header lines
        assert!(
            !result.contains("fn add("),
            "no names means no functions extracted"
        );
    }

    #[test]
    fn test_count_empty_fns_quick_detects_stubs() {
        let source = "fn add(a: i32, b: i32) -> i32 {}\nfn sub(a: i32, b: i32) -> i32 {\n    a - b\n}\nfn mul(a: i32, b: i32) -> i32 { }";
        assert_eq!(
            count_empty_fns_quick(source),
            2,
            "should detect 2 empty fns"
        );
    }

    #[test]
    fn test_count_empty_fns_quick_detects_todo() {
        let source = "fn add(a: i32, b: i32) -> i32 {\n    todo!()\n}\nfn sub(a: i32, b: i32) -> i32 {\n    a - b\n}";
        assert_eq!(
            count_empty_fns_quick(source),
            1,
            "should detect todo!() as empty"
        );
    }

    #[test]
    fn test_count_total_fns_quick() {
        let source = "fn add(a: i32) -> i32 { a }\npub fn sub(a: i32) -> i32 { a }\nstruct Foo {}";
        assert_eq!(count_total_fns_quick(source), 2, "should count 2 fns");
    }

    #[test]
    fn test_combine_accumulated_chunks_deduplicates_use() {
        let chunks = vec![
            "use std::collections::HashMap;\nfn a() {}".to_string(),
            "use std::collections::HashMap;\nuse std::io;\nfn b() {}".to_string(),
        ];
        let result = combine_accumulated_chunks(&chunks);
        assert_eq!(
            result.matches("use std::collections::HashMap;").count(),
            1,
            "should deduplicate HashMap use"
        );
        assert!(result.contains("use std::io;"));
        assert!(result.contains("fn a()"));
        assert!(result.contains("fn b()"));
    }
}
