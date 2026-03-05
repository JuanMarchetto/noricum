//! MCP server: reads JSON-RPC from stdin, dispatches to tool handlers, writes to stdout.
//!
//! Implements the MCP stdio transport. The server exposes these tools:
//! - `migrate_function`: takes C source, returns migrated Rust
//! - `analyze_function`: takes C source, returns difficulty + analysis
//! - `check_compilation`: takes Rust source, returns success/errors
//! - `get_idiomatic_score`: takes Rust source, returns score

use std::io::{self, BufRead, Write};

use serde_json::json;
use tracing::{debug, error, info};

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

    JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
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

    let result = match call_params.name.as_str() {
        "migrate_function" => tool_migrate_function(source),
        "analyze_function" => tool_analyze_function(source),
        "check_compilation" => tool_check_compilation(source),
        "get_idiomatic_score" => tool_get_idiomatic_score(source),
        other => ToolResult::error(format!("unknown tool: {other}")),
    };

    JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
}

// ---------------------------------------------------------------------------
// Tool handlers (v0: synchronous, no LLM)
// ---------------------------------------------------------------------------

/// `migrate_function`: For v0, performs a naive rule-based translation.
/// A real implementation will use LLM agents in Phase 1.
fn tool_migrate_function(source: Option<String>) -> ToolResult {
    let source = match source {
        Some(s) => s,
        None => return ToolResult::error("missing 'source' parameter".to_string()),
    };

    // v0 stub: return a placeholder indicating that LLM migration is not yet wired
    let difficulty = noricum_core::router::classify_difficulty(&source);
    let result = json!({
        "difficulty": format!("{difficulty:?}"),
        "rust_source": format!(
            "// TODO: LLM-based migration not yet available in v0\n\
             // Difficulty: {difficulty:?}\n\
             // Original C source ({lines} lines) would be migrated here.\n",
            lines = source.lines().count()
        ),
        "note": "LLM-based migration will be available in Phase 1. Use c2rust for mechanical translation."
    });
    ToolResult::text(serde_json::to_string_pretty(&result).unwrap())
}

/// `analyze_function`: Classify difficulty and report characteristics.
fn tool_analyze_function(source: Option<String>) -> ToolResult {
    let source = match source {
        Some(s) => s,
        None => return ToolResult::error("missing 'source' parameter".to_string()),
    };

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
    ToolResult::text(serde_json::to_string_pretty(&result).unwrap())
}

/// `check_compilation`: Compile Rust source and report results.
fn tool_check_compilation(source: Option<String>) -> ToolResult {
    let source = match source {
        Some(s) => s,
        None => return ToolResult::error("missing 'source' parameter".to_string()),
    };

    match noricum_tools::compiler::check_rust_compiles(&source) {
        Ok(compile_result) => {
            let result = json!({
                "success": compile_result.success,
                "errors": compile_result.stderr,
            });
            ToolResult::text(serde_json::to_string_pretty(&result).unwrap())
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

    let unsafe_count = noricum_tools::compiler::count_unsafe_blocks(&source);
    let clippy_warnings =
        noricum_tools::compiler::run_clippy_on_source(&source).unwrap_or_default();

    let score =
        noricum_validation::compute_idiomatic_score(unsafe_count, clippy_warnings.len() as u32);

    let result = json!({
        "score": score,
        "unsafe_count": unsafe_count,
        "clippy_warning_count": clippy_warnings.len(),
        "clippy_warnings": clippy_warnings,
    });
    ToolResult::text(serde_json::to_string_pretty(&result).unwrap())
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
        assert_eq!(tools.len(), 4);

        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"migrate_function"));
        assert!(names.contains(&"analyze_function"));
        assert!(names.contains(&"check_compilation"));
        assert!(names.contains(&"get_idiomatic_score"));
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
}
