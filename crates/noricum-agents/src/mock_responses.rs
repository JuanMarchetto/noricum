/// Mock LLM responses for testing agent parsing logic without real API calls.

#[cfg(test)]
pub(crate) const MOCK_ANALYSIS_SIMPLE: &str = r#"{"difficulty":"easy","patterns":["pure_function"],"rust_equivalents":{"pure_function":"direct translation"},"dependencies":[],"risks":[],"strategy":"Direct translation"}"#;

#[cfg(test)]
pub(crate) const MOCK_ANALYSIS_COMPLEX: &str = r#"{"difficulty":"hard","patterns":["malloc_free","ptr_arithmetic"],"rust_equivalents":{"malloc_free":"Vec/Box allocation","ptr_arithmetic":"slice indexing"},"dependencies":["helper_alloc"],"risks":["dangling pointers","buffer overflow"],"strategy":"Convert to Vec-based allocation with bounds checking"}"#;

#[cfg(test)]
pub(crate) const MOCK_ANALYSIS_MALFORMED: &str = "Sure! Here's some analysis without JSON";

#[cfg(test)]
pub(crate) const MOCK_TRANSLATION_FENCED: &str =
    "```rust\nfn add(a: i32, b: i32) -> i32 { a + b }\n```";

#[cfg(test)]
pub(crate) const MOCK_TRANSLATION_BARE: &str = "fn add(a: i32, b: i32) -> i32 { a + b }";

#[cfg(test)]
pub(crate) const MOCK_TRANSLATION_WITH_PROSE: &str = "Here's the Rust translation:\n\n```rust\nfn add(a: i32, b: i32) -> i32 { a + b }\n```\n\nThis is a simple function.";

#[cfg(test)]
pub(crate) const MOCK_REPAIR_NESTED_FENCES: &str =
    "```rust\nfn fixed() -> i32 {\n    // uses ```backticks``` in comment\n    42\n}\n```";
