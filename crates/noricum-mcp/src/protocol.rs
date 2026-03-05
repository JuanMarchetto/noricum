//! JSON-RPC 2.0 message types for the MCP stdio transport.
//!
//! Implements the minimal subset of JSON-RPC needed for MCP:
//! - Request (with method + params)
//! - Response (with result or error)
//! - Tool definitions and tool call/result types.

use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// JSON-RPC 2.0 success response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Create a success response.
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(id: serde_json::Value, code: i64, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// Standard JSON-RPC error codes
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
pub const PARSE_ERROR: i64 = -32700;

/// MCP tool definition, returned by `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// MCP `tools/call` parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

/// MCP tool result content item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// MCP tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<ToolResultContent>,
    #[serde(rename = "isError", default)]
    pub is_error: bool,
}

impl ToolResult {
    /// Create a successful text result.
    pub fn text(text: String) -> Self {
        Self {
            content: vec![ToolResultContent {
                content_type: "text".to_string(),
                text,
            }],
            is_error: false,
        }
    }

    /// Create an error text result.
    pub fn error(text: String) -> Self {
        Self {
            content: vec![ToolResultContent {
                content_type: "text".to_string(),
                text,
            }],
            is_error: true,
        }
    }
}

/// MCP server capabilities, returned in `initialize` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    pub tools: ToolCapability,
}

/// Tool capability descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCapability {
    #[serde(rename = "listChanged", default)]
    pub list_changed: bool,
}

/// MCP `initialize` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

/// MCP server info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_request_deserialization() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
        let req: JsonRpcRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.method, "tools/list");
        assert_eq!(req.id, json!(1));
    }

    #[test]
    fn test_success_response_serialization() {
        let resp = JsonRpcResponse::success(json!(1), json!({"ok": true}));
        let raw = serde_json::to_string(&resp).unwrap();
        assert!(raw.contains("\"result\""));
        assert!(!raw.contains("\"error\""));
    }

    #[test]
    fn test_error_response_serialization() {
        let resp = JsonRpcResponse::error(json!(2), METHOD_NOT_FOUND, "not found".into());
        let raw = serde_json::to_string(&resp).unwrap();
        assert!(raw.contains("\"error\""));
        assert!(!raw.contains("\"result\""));
    }

    #[test]
    fn test_tool_call_params_deserialization() {
        let raw = r#"{"name":"migrate_function","arguments":{"source":"int x;"}}"#;
        let params: ToolCallParams = serde_json::from_str(raw).unwrap();
        assert_eq!(params.name, "migrate_function");
    }

    #[test]
    fn test_tool_result_text() {
        let result = ToolResult::text("hello".to_string());
        assert!(!result.is_error);
        assert_eq!(result.content[0].text, "hello");
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("failed".to_string());
        assert!(result.is_error);
    }
}
