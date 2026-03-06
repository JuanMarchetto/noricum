//! MCP server: reads JSON-RPC from stdin, dispatches to tool handlers, writes to stdout.
//!
//! Implements the MCP stdio transport. The server exposes these tools:
//! - `migrate_function`: takes C source, returns migrated Rust
//! - `analyze_function`: takes C source, returns difficulty + analysis
//! - `check_compilation`: takes Rust source, returns success/errors
//! - `get_idiomatic_score`: takes Rust source, returns score
//! - `diff_test`: compares C and Rust output byte-by-byte
//! - `repair`: re-checks compilation and provides structured diagnostics

use std::io::{self, BufRead, Write};
use std::sync::LazyLock;

use serde_json::json;
use tracing::{debug, error, info};

/// Maximum input source size for MCP tool calls: 10 MB.
const MAX_MCP_SOURCE_SIZE: usize = 10 * 1024 * 1024;

/// Migration call timeout: 5 minutes.
const MIGRATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Static Tokio runtime shared across MCP tool calls.
static MCP_RUNTIME: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().expect("failed to create MCP tokio runtime"));

use crate::protocol::{
    INVALID_PARAMS, InitializeResult, JsonRpcRequest, JsonRpcResponse, METHOD_NOT_FOUND,
    PARSE_ERROR, ServerCapabilities, ServerInfo, ToolCallParams, ToolCapability, ToolDefinition,
    ToolResult,
};

/// Run the MCP server loop: read JSON-RPC from stdin, dispatch, write to stdout.
///
/// This blocks the current thread. Each line of stdin is expected to be a
/// complete JSON-RPC request (newline-delimited JSON).
pub fn run_server() -> io::Result<()> {
    info!("noricum MCP server starting");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let response = handle_message(line);
        let response_json = serde_json::to_string(&response).unwrap_or_else(|e| {
            error!(error = %e, "failed to serialize response");
            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"serialization error"}}"#
                .to_string()
        });

        writeln!(stdout, "{response_json}")?;
        stdout.flush()?;
    }

    info!("noricum MCP server shutting down");
    Ok(())
}

/// Parse and dispatch a single JSON-RPC message.
fn handle_message(raw: &str) -> JsonRpcResponse {
    let request: JsonRpcRequest = match serde_json::from_str(raw) {
        Ok(req) => req,
        Err(e) => {
            debug!(error = %e, "failed to parse request");
            return JsonRpcResponse::error(
                serde_json::Value::Null,
                PARSE_ERROR,
                format!("parse error: {e}"),
            );
        }
    };

    dispatch(request)
}

/// Dispatch a parsed JSON-RPC request to the appropriate handler.
fn dispatch(request: JsonRpcRequest) -> JsonRpcResponse {
    let id = request.id.clone();

    match request.method.as_str() {
        "initialize" => handle_initialize(id),
        "initialized" => {
            // Notification, no response needed but we send one for consistency
            JsonRpcResponse::success(id, json!({}))
        }
        "tools/list" => handle_tools_list(id),
        "tools/call" => handle_tools_call(id, request.params),
        _ => {
            debug!(method = %request.method, "unknown method");
            JsonRpcResponse::error(
                id,
                METHOD_NOT_FOUND,
                format!("method not found: {}", request.method),
            )
        }
    }
}

/// Handle `initialize` request.
fn handle_initialize(id: serde_json::Value) -> JsonRpcResponse {
    let result = InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ServerCapabilities {
            tools: ToolCapability {
                list_changed: false,
            },
        },
        server_info: ServerInfo {
            name: "noricum".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };

    match serde_json::to_value(result) {
        Ok(val) => JsonRpcResponse::success(id, val),
        Err(e) => {
            error!(error = %e, "failed to serialize InitializeResult");
            JsonRpcResponse::error(id, -32603, format!("serialization error: {e}"))
        }
    }
}

/// Handle `tools/list` request.
fn handle_tools_list(id: serde_json::Value) -> JsonRpcResponse {
    let tools = vec![
        ToolDefinition {
            name: "migrate_function".to_string(),
            description: "Migrate a C function to idiomatic Rust. Returns the translated Rust source code.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "The C source code to migrate"
                    }
                },
                "required": ["source"]
            }),
        },
        ToolDefinition {
            name: "analyze_function".to_string(),
            description: "Analyze a C function and return its difficulty classification and characteristics.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "The C source code to analyze"
                    }
                },
                "required": ["source"]
            }),
        },
        ToolDefinition {
            name: "check_compilation".to_string(),
            description: "Check if Rust source code compiles. Returns success/failure and any compiler errors.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "The Rust source code to compile-check"
                    }
                },
                "required": ["source"]
            }),
        },
        ToolDefinition {
            name: "get_idiomatic_score".to_string(),
            description: "Score Rust source code for idiomatic quality (0-100). Checks unsafe usage and clippy warnings.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "The Rust source code to score"
                    }
                },
                "required": ["source"]
            }),
        },
        ToolDefinition {
            name: "diff_test".to_string(),
            description: "Run differential test: compile C and Rust source, run both, compare outputs byte-by-byte.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "c_source": {
                        "type": "string",
                        "description": "The C source code (must have main())"
                    },
                    "rust_source": {
                        "type": "string",
                        "description": "The Rust source code (must have main())"
                    }
                },
                "required": ["c_source", "rust_source"]
            }),
        },
        ToolDefinition {
            name: "repair".to_string(),
            description: "Attempt to fix Rust code that has compiler errors or test failures. Uses rule-based fixes.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "The Rust source code to repair"
                    },
                    "errors": {
                        "type": "string",
                        "description": "Compiler errors or test failure descriptions"
                    }
                },
                "required": ["source", "errors"]
            }),
        },
    ];

    JsonRpcResponse::success(id, json!({ "tools": tools }))
}

/// Handle `tools/call` request: dispatch to the appropriate tool handler.
fn handle_tools_call(id: serde_json::Value, params: serde_json::Value) -> JsonRpcResponse {
    let call_params: ToolCallParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                INVALID_PARAMS,
                format!("invalid tool call params: {e}"),
            );
        }
    };

    let source = call_params
        .arguments
        .get("source")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let c_source = call_params
        .arguments
        .get("c_source")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let rust_source = call_params
        .arguments
        .get("rust_source")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let errors = call_params
        .arguments
        .get("errors")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let result = match call_params.name.as_str() {
        "migrate_function" => tool_migrate_function(source),
        "analyze_function" => tool_analyze_function(source),
        "check_compilation" => tool_check_compilation(source),
        "get_idiomatic_score" => tool_get_idiomatic_score(source),
        "diff_test" => tool_diff_test(c_source, rust_source),
        "repair" => tool_repair(source, errors),
        other => ToolResult::error(format!("unknown tool: {other}")),
    };

    match serde_json::to_value(result) {
        Ok(val) => JsonRpcResponse::success(id, val),
        Err(e) => {
            error!(error = %e, "failed to serialize tool result");
            JsonRpcResponse::error(id, -32603, format!("serialization error: {e}"))
        }
    }
}

/// Serialize a JSON value to a pretty-printed string, returning a ToolResult error on failure.
fn json_to_tool_result(value: &serde_json::Value) -> ToolResult {
    match serde_json::to_string_pretty(value) {
        Ok(s) => ToolResult::text(s),
        Err(e) => {
            error!(error = %e, "failed to serialize JSON result");
            ToolResult::error(format!("serialization error: {e}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Tool handlers (v0: synchronous, no LLM)
// ---------------------------------------------------------------------------

/// `migrate_function`: Migrate C source to Rust using the full pipeline.
///
/// Writes the C source to a temp file and runs `migrate_file_sync` (rule-based).
/// If an Anthropic API key is available, uses the async LLM pipeline instead.
fn tool_migrate_function(source: Option<String>) -> ToolResult {
    let source = match source {
        Some(s) => s,
        None => return ToolResult::error("missing 'source' parameter".to_string()),
    };

    if source.len() > MAX_MCP_SOURCE_SIZE {
        return ToolResult::error(format!(
            "source exceeds maximum size of {} bytes",
            MAX_MCP_SOURCE_SIZE
        ));
    }

    let difficulty = noricum_core::router::classify_difficulty(&source);

    // Write to temp file for the pipeline
    let tmp = match tempfile::tempdir() {
        Ok(t) => t,
        Err(e) => return ToolResult::error(format!("failed to create temp dir: {e}")),
    };
    let c_file = tmp.path().join("input.c");
    if let Err(e) = std::fs::write(&c_file, &source) {
        return ToolResult::error(format!("failed to write temp file: {e}"));
    }

    // Try async LLM pipeline with timeout, fall back to sync
    let unit = if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        let config = noricum_core::MigrationConfig::default();
        match MCP_RUNTIME.block_on(async {
            tokio::time::timeout(
                MIGRATION_TIMEOUT,
                noricum_core::orchestrator::migrate_file(&c_file, &config),
            )
            .await
        }) {
            Ok(Ok(unit)) => unit,
            Ok(Err(e)) => return ToolResult::error(format!("migration failed: {e}")),
            Err(_) => return ToolResult::error("migration timed out (5 minute limit)".to_string()),
        }
    } else {
        match noricum_core::orchestrator::migrate_file_sync(&c_file) {
            Ok(unit) => unit,
            Err(e) => return ToolResult::error(format!("migration failed: {e}")),
        }
    };

    let result = json!({
        "difficulty": format!("{difficulty:?}"),
        "state": format!("{:?}", unit.state),
        "rust_source": unit.rust_output.unwrap_or_default(),
        "idiomatic_score": unit.idiomatic_score,
        "unsafe_count": unit.unsafe_count,
        "diff_test_passed": unit.metrics.diff_test_passed,
    });
    json_to_tool_result(&result)
}

/// `analyze_function`: Classify difficulty and report characteristics.
fn tool_analyze_function(source: Option<String>) -> ToolResult {
    let source = match source {
        Some(s) => s,
        None => return ToolResult::error("missing 'source' parameter".to_string()),
    };
    if source.len() > MAX_MCP_SOURCE_SIZE {
        return ToolResult::error(format!(
            "source exceeds maximum size of {} bytes",
            MAX_MCP_SOURCE_SIZE
        ));
    }

    let difficulty = noricum_core::router::classify_difficulty(&source);
    let line_count = source.lines().count();
    let has_pointers = source.contains('*') && !source.contains("/*");
    let has_malloc = source.contains("malloc") || source.contains("calloc");
    let has_goto = source.contains("goto ");

    let result = json!({
        "difficulty": format!("{difficulty:?}"),
        "line_count": line_count,
        "characteristics": {
            "has_pointers": has_pointers,
            "has_malloc": has_malloc,
            "has_goto": has_goto,
        }
    });
    json_to_tool_result(&result)
}

/// `check_compilation`: Compile Rust source and report results.
fn tool_check_compilation(source: Option<String>) -> ToolResult {
    let source = match source {
        Some(s) => s,
        None => return ToolResult::error("missing 'source' parameter".to_string()),
    };
    if source.len() > MAX_MCP_SOURCE_SIZE {
        return ToolResult::error(format!(
            "source exceeds maximum size of {} bytes",
            MAX_MCP_SOURCE_SIZE
        ));
    }

    match noricum_tools::compiler::check_rust_compiles(&source) {
        Ok(compile_result) => {
            let result = json!({
                "success": compile_result.success,
                "errors": compile_result.stderr,
            });
            json_to_tool_result(&result)
        }
        Err(e) => ToolResult::error(format!("compilation check failed: {e}")),
    }
}

/// `get_idiomatic_score`: Score Rust source for idiomatic quality.
fn tool_get_idiomatic_score(source: Option<String>) -> ToolResult {
    let source = match source {
        Some(s) => s,
        None => return ToolResult::error("missing 'source' parameter".to_string()),
    };
    if source.len() > MAX_MCP_SOURCE_SIZE {
        return ToolResult::error(format!(
            "source exceeds maximum size of {} bytes",
            MAX_MCP_SOURCE_SIZE
        ));
    }

    let unsafe_count = noricum_tools::compiler::count_unsafe_blocks(&source);
    let clippy_warnings = match noricum_tools::compiler::run_clippy_on_source(&source) {
        Ok(warnings) => warnings,
        Err(e) => {
            tracing::warn!(error = %e, "clippy analysis failed, score may be incomplete");
            Vec::new()
        }
    };

    let score =
        noricum_validation::compute_idiomatic_score(unsafe_count, clippy_warnings.len() as u32);

    let result = json!({
        "score": score,
        "unsafe_count": unsafe_count,
        "clippy_warning_count": clippy_warnings.len(),
        "clippy_warnings": clippy_warnings,
    });
    json_to_tool_result(&result)
}

/// `diff_test`: Run differential test between C and Rust source.
fn tool_diff_test(c_source: Option<String>, rust_source: Option<String>) -> ToolResult {
    let c_source = match c_source {
        Some(s) => s,
        None => return ToolResult::error("missing 'c_source' parameter".to_string()),
    };
    let rust_source = match rust_source {
        Some(s) => s,
        None => return ToolResult::error("missing 'rust_source' parameter".to_string()),
    };

    match noricum_tools::diff_test::run_diff_test(&c_source, &rust_source) {
        Ok(result) => {
            let res = json!({
                "passed": result.passed,
                "c_compiled": result.c_compiled,
                "rust_compiled": result.rust_compiled,
                "c_output": result.c_output,
                "rust_output": result.rust_output,
            });
            json_to_tool_result(&res)
        }
        Err(e) => ToolResult::error(format!("diff test failed: {e}")),
    }
}

/// `repair`: Attempt to fix Rust code by recompiling and reporting detailed errors.
///
/// For now, re-checks compilation and provides structured diagnostics.
/// With an API key, could use the LLM repair agent.
fn tool_repair(source: Option<String>, errors: Option<String>) -> ToolResult {
    let source = match source {
        Some(s) => s,
        None => return ToolResult::error("missing 'source' parameter".to_string()),
    };
    if source.len() > MAX_MCP_SOURCE_SIZE {
        return ToolResult::error(format!(
            "source exceeds maximum size of {} bytes",
            MAX_MCP_SOURCE_SIZE
        ));
    }
    let errors_str = errors.unwrap_or_default();

    // Re-check compilation to get fresh diagnostics
    let compile_result = match noricum_tools::compiler::check_rust_compiles(&source) {
        Ok(r) => r,
        Err(e) => return ToolResult::error(format!("compilation check failed: {e}")),
    };

    let clippy_warnings = match noricum_tools::compiler::run_clippy_on_source(&source) {
        Ok(warnings) => warnings,
        Err(e) => {
            tracing::warn!(error = %e, "clippy analysis failed in repair, diagnostics may be incomplete");
            Vec::new()
        }
    };
    let unsafe_count = noricum_tools::compiler::count_unsafe_blocks(&source);

    let result = json!({
        "compiles": compile_result.success,
        "compiler_output": compile_result.stderr,
        "clippy_warnings": clippy_warnings,
        "unsafe_count": unsafe_count,
        "original_errors": errors_str,
        "suggestion": if compile_result.success {
            "Code compiles. Check clippy warnings for further improvements."
        } else {
            "Code has compilation errors. Review the compiler_output for details."
        }
    });
    json_to_tool_result(&result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_initialize() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let resp = handle_message(raw);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert_eq!(result["serverInfo"]["name"], "noricum");
    }

    #[test]
    fn test_handle_tools_list() {
        let raw = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;
        let resp = handle_message(raw);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 6);

        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"migrate_function"));
        assert!(names.contains(&"analyze_function"));
        assert!(names.contains(&"check_compilation"));
        assert!(names.contains(&"get_idiomatic_score"));
        assert!(names.contains(&"diff_test"));
        assert!(names.contains(&"repair"));
    }

    #[test]
    fn test_handle_analyze_function() {
        let raw = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"analyze_function","arguments":{"source":"int add(int a, int b) { return a + b; }"}}}"#;
        let resp = handle_message(raw);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let content = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
        assert_eq!(parsed["difficulty"], "Easy");
    }

    #[test]
    fn test_handle_check_compilation_valid() {
        let raw = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"check_compilation","arguments":{"source":"pub fn add(a: i32, b: i32) -> i32 { a + b }"}}}"#;
        let resp = handle_message(raw);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let content = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
        assert_eq!(parsed["success"], true);
    }

    #[test]
    fn test_handle_check_compilation_invalid() {
        let raw = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"check_compilation","arguments":{"source":"fn bad( { }"}}}"#;
        let resp = handle_message(raw);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let content = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
        assert_eq!(parsed["success"], false);
    }

    #[test]
    fn test_handle_get_idiomatic_score() {
        let raw = r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"get_idiomatic_score","arguments":{"source":"pub fn add(a: i32, b: i32) -> i32 { a + b }"}}}"#;
        let resp = handle_message(raw);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let content = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
        assert_eq!(parsed["score"], 100);
        assert_eq!(parsed["unsafe_count"], 0);
    }

    #[test]
    fn test_handle_unknown_method() {
        let raw = r#"{"jsonrpc":"2.0","id":7,"method":"unknown/method","params":{}}"#;
        let resp = handle_message(raw);
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, METHOD_NOT_FOUND);
    }

    #[test]
    fn test_handle_parse_error() {
        let resp = handle_message("this is not json");
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, PARSE_ERROR);
    }

    #[test]
    fn test_handle_missing_source_param() {
        let raw = r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"analyze_function","arguments":{}}}"#;
        let resp = handle_message(raw);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn test_handle_unknown_tool() {
        let raw = r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"nonexistent_tool","arguments":{}}}"#;
        let resp = handle_message(raw);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn test_handle_diff_test() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {
                "name": "diff_test",
                "arguments": {
                    "c_source": "#include <stdio.h>\nint main(void) { printf(\"42\\n\"); return 0; }",
                    "rust_source": "fn main() { println!(\"42\"); }"
                }
            }
        });
        let resp = handle_message(&serde_json::to_string(&request).unwrap());
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let content = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
        assert_eq!(parsed["passed"], true);
    }

    #[test]
    fn test_handle_diff_test_missing_params() {
        let raw = r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"diff_test","arguments":{}}}"#;
        let resp = handle_message(raw);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn test_handle_repair() {
        let raw = r#"{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"repair","arguments":{"source":"pub fn add(a: i32, b: i32) -> i32 { a + b }","errors":"none"}}}"#;
        let resp = handle_message(raw);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let content = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
        assert_eq!(parsed["compiles"], true);
    }

    #[test]
    fn test_handle_migrate_function_simple() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 13,
            "method": "tools/call",
            "params": {
                "name": "migrate_function",
                "arguments": {
                    "source": "int add(int a, int b) { return a + b; }"
                }
            }
        });
        let resp = handle_message(&serde_json::to_string(&request).unwrap());
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let content = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
        assert_eq!(parsed["difficulty"], "Easy");
        assert!(parsed["rust_source"].as_str().unwrap().contains("fn add"));
    }
}
