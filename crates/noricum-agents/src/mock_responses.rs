//! Mock LLM responses for testing agent parsing logic without real API calls.
//!
//! These constants simulate realistic LLM outputs for the analysis, translation,
//! and repair agents. They are used in unit tests and integration tests to verify
//! that prompt response parsing works correctly without requiring actual API calls.

pub const MOCK_ANALYSIS_SIMPLE: &str = r#"{"difficulty":"easy","patterns":["pure_function"],"rust_equivalents":{"pure_function":"direct translation"},"dependencies":[],"risks":[],"strategy":"Direct translation"}"#;

pub const MOCK_ANALYSIS_COMPLEX: &str = r#"{"difficulty":"hard","patterns":["malloc_free","ptr_arithmetic"],"rust_equivalents":{"malloc_free":"Vec/Box allocation","ptr_arithmetic":"slice indexing"},"dependencies":["helper_alloc"],"risks":["dangling pointers","buffer overflow"],"strategy":"Convert to Vec-based allocation with bounds checking"}"#;

pub const MOCK_ANALYSIS_MALFORMED: &str = "Sure! Here's some analysis without JSON";

pub const MOCK_TRANSLATION_FENCED: &str = "```rust\nfn add(a: i32, b: i32) -> i32 { a + b }\n```";

pub const MOCK_TRANSLATION_BARE: &str = "fn add(a: i32, b: i32) -> i32 { a + b }";

pub const MOCK_TRANSLATION_WITH_PROSE: &str = "Here's the Rust translation:\n\n```rust\nfn add(a: i32, b: i32) -> i32 { a + b }\n```\n\nThis is a simple function.";

pub const MOCK_REPAIR_NESTED_FENCES: &str =
    "```rust\nfn fixed() -> i32 {\n    // uses ```backticks``` in comment\n    42\n}\n```";

pub const MOCK_ANALYSIS_EMPTY_FIELDS: &str = r#"{"difficulty":"easy","patterns":[],"rust_equivalents":{},"dependencies":[],"risks":[],"strategy":""}"#;

pub const MOCK_ANALYSIS_MISSING_FIELD: &str = r#"{"difficulty":"easy","patterns":[]}"#;

pub const MOCK_TRANSLATION_EMPTY: &str = "";

pub const MOCK_TRANSLATION_ONLY_PROSE: &str =
    "I cannot translate this code because it is too complex.";

/// Mock analysis response wrapped in markdown code fences (as LLMs typically return).
pub const MOCK_ANALYSIS_FENCED: &str = "Here is the analysis:\n```json\n{\"difficulty\":\"medium\",\"patterns\":[\"ptr_arithmetic\",\"error_codes\"],\"rust_equivalents\":{\"ptr_arithmetic\":\"slice indexing\",\"error_codes\":\"Result type\"},\"dependencies\":[],\"risks\":[\"null pointer dereference\"],\"strategy\":\"Use slices and Result\"}\n```\n";

/// Mock repair response with a compilable fix.
pub const MOCK_REPAIR_COMPILABLE: &str =
    "```rust\npub fn add(a: i32, b: i32) -> i32 {\n    a.wrapping_add(b)\n}\n```";

/// Mock test generation response.
pub const MOCK_TEST_GEN: &str = "```rust\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn test_add_positive() {\n        assert_eq!(add(2, 3), 5);\n    }\n\n    #[test]\n    fn test_add_negative() {\n        assert_eq!(add(-1, 1), 0);\n    }\n\n    #[test]\n    fn test_add_zero() {\n        assert_eq!(add(0, 0), 0);\n    }\n}\n```";
