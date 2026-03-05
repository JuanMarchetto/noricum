//! Noricum MCP server (stub for v1).
//!
//! Will expose Noricum tools as MCP server:
//! - migrate_function
//! - analyze_function
//! - check_status
//! - get_score
//!
//! Implementation deferred to Phase 3.

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
