//! Noricum MCP server.
//!
//! Exposes Noricum tools via the Model Context Protocol (MCP) over stdio:
//! - `migrate_function`: takes C source, returns migrated Rust
//! - `analyze_function`: takes C source, returns difficulty + analysis
//! - `check_compilation`: takes Rust source, returns success/errors
//! - `get_idiomatic_score`: takes Rust source, returns score
//!
//! Uses JSON-RPC 2.0 over newline-delimited stdin/stdout (MCP stdio transport).

pub mod protocol;
pub mod server;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(version(), "0.1.0");
    }
}
