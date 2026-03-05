/// RAG Pattern Store: holds successful migration examples for retrieval.
///
/// Provides context to the translation agent by showing similar past translations.
/// Patterns are scored by tag matches, content keyword overlap, and usage count.
use serde::{Deserialize, Serialize};

/// A single migration pattern: a C code snippet and its Rust equivalent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPattern {
    /// Descriptive name (e.g., "ptr_to_slice", "error_to_result")
    pub name: String,
    /// C code pattern
    pub c_pattern: String,
    /// Corresponding Rust code
    pub rust_pattern: String,
    /// Tags for matching (e.g., ["pointer", "slice", "array"])
    pub tags: Vec<String>,
    /// How many times this pattern was successfully used
    pub usage_count: u32,
}

/// In-memory store of migration patterns with relevance-based retrieval.
pub struct PatternStore {
    patterns: Vec<MigrationPattern>,
}

impl PatternStore {
    /// Create an empty pattern store.
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// Load seed patterns from the `patterns/` directory at the project root.
    ///
    /// Each markdown file is expected to have `## C Pattern` and `## Rust Equivalent`
    /// sections with fenced code blocks. The file stem becomes the pattern name and
    /// is also split on `_` to derive tags.
    pub fn load_seed_patterns() -> Self {
        let mut store = Self::new();

        // Try to find the patterns directory relative to the project root.
        // We walk up from the crate manifest dir, or use a known absolute path.
        let candidates = [
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../patterns"),
            std::path::PathBuf::from("patterns"),
        ];

        let patterns_dir = candidates.iter().find(|p| p.is_dir());

        let Some(patterns_dir) = patterns_dir else {
            return store;
        };

        let Ok(entries) = std::fs::read_dir(patterns_dir) else {
            return store;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }

            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };

            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            let tags: Vec<String> = name.split('_').map(|s| s.to_string()).collect();

            if let Some(pattern) = parse_pattern_markdown(&content, &name, tags) {
                store.add_pattern(pattern);
            }
        }

        store
    }

    /// Add a new pattern from a successful migration.
    pub fn add_pattern(&mut self, pattern: MigrationPattern) {
        self.patterns.push(pattern);
    }

    /// Find the N most relevant patterns for a given C source.
    ///
    /// Scoring:
    /// 1. Tag matches: +10 per tag that appears in the C source
    /// 2. Content keyword overlap: +1 per matching word between c_pattern and c_source
    /// 3. Usage count bonus: +usage_count
    ///
    /// Returns sorted by score descending.
    pub fn find_relevant(&self, c_source: &str, max_results: usize) -> Vec<&MigrationPattern> {
        let source_lower = c_source.to_lowercase();
        let source_words: std::collections::HashSet<&str> = source_lower
            .split_whitespace()
            .chain(source_lower.split(|c: char| !c.is_alphanumeric()))
            .filter(|w| w.len() >= 2)
            .collect();

        let mut scored: Vec<(usize, &MigrationPattern)> = self
            .patterns
            .iter()
            .map(|pattern| {
                let mut score: usize = 0;

                // Tag matches: +10 per tag found in the C source
                for tag in &pattern.tags {
                    if source_lower.contains(&tag.to_lowercase()) {
                        score += 10;
                    }
                }

                // Content keyword overlap: +1 per matching word
                let pattern_lower = pattern.c_pattern.to_lowercase();
                let pattern_words: std::collections::HashSet<&str> = pattern_lower
                    .split_whitespace()
                    .chain(pattern_lower.split(|c: char| !c.is_alphanumeric()))
                    .filter(|w| w.len() >= 2)
                    .collect();
                for word in &pattern_words {
                    if source_words.contains(word) {
                        score += 1;
                    }
                }

                // Usage count bonus
                score += pattern.usage_count as usize;

                (score, pattern)
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.0.cmp(&a.0));

        scored
            .into_iter()
            .take(max_results)
            .map(|(_, pattern)| pattern)
            .collect()
    }

    /// Record a successful use of a pattern (increments its usage count).
    pub fn record_usage(&mut self, pattern_name: &str) {
        if let Some(pattern) = self.patterns.iter_mut().find(|p| p.name == pattern_name) {
            pattern.usage_count += 1;
        }
    }

    /// Number of patterns in the store.
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }
}

impl Default for PatternStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a pattern markdown file into a `MigrationPattern`.
///
/// Expected format: a markdown file with `## C Pattern` and `## Rust Equivalent`
/// headers, each followed by a fenced code block.
fn parse_pattern_markdown(
    content: &str,
    name: &str,
    tags: Vec<String>,
) -> Option<MigrationPattern> {
    let c_pattern = extract_code_after_header(content, "C Pattern")?;
    // Try both "Rust Equivalent" and "Rust Equivalent (Vec)" style headers
    let rust_pattern = extract_code_after_header(content, "Rust Equivalent")?;

    Some(MigrationPattern {
        name: name.to_string(),
        c_pattern,
        rust_pattern,
        tags,
        usage_count: 0,
    })
}

/// Extract the first fenced code block after a markdown header containing `header_text`.
fn extract_code_after_header(content: &str, header_text: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let header_idx = lines
        .iter()
        .position(|line| line.starts_with("## ") && line.contains(header_text))?;

    // Find the first code fence after the header
    let mut in_fence = false;
    let mut code_lines = Vec::new();

    for line in &lines[header_idx + 1..] {
        if line.starts_with("```") {
            if in_fence {
                // End of code block
                break;
            } else {
                // Start of code block
                in_fence = true;
                continue;
            }
        }
        if in_fence {
            code_lines.push(*line);
        }
    }

    if code_lines.is_empty() {
        None
    } else {
        Some(code_lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pattern(name: &str, tags: &[&str], usage: u32) -> MigrationPattern {
        MigrationPattern {
            name: name.to_string(),
            c_pattern: format!("void {}(const char *data, size_t len);", name),
            rust_pattern: format!("fn {}(data: &[u8]);", name),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            usage_count: usage,
        }
    }

    #[test]
    fn test_pattern_creation() {
        let pattern = MigrationPattern {
            name: "ptr_to_slice".to_string(),
            c_pattern: "void f(const char *data, size_t len)".to_string(),
            rust_pattern: "fn f(data: &[u8])".to_string(),
            tags: vec!["pointer".to_string(), "slice".to_string()],
            usage_count: 0,
        };
        assert_eq!(pattern.name, "ptr_to_slice");
        assert_eq!(pattern.tags.len(), 2);
        assert_eq!(pattern.usage_count, 0);
    }

    #[test]
    fn test_store_add_and_len() {
        let mut store = PatternStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        store.add_pattern(sample_pattern("p1", &["pointer"], 0));
        assert_eq!(store.len(), 1);
        assert!(!store.is_empty());

        store.add_pattern(sample_pattern("p2", &["error"], 0));
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn test_find_relevant_tag_scoring() {
        let mut store = PatternStore::new();
        store.add_pattern(sample_pattern("ptr_pattern", &["pointer", "slice"], 0));
        store.add_pattern(sample_pattern("err_pattern", &["error", "result"], 0));

        // Source with "pointer" should rank ptr_pattern higher
        let results = store.find_relevant("void f(char *pointer, size_t len)", 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "ptr_pattern");
    }

    #[test]
    fn test_find_relevant_usage_bonus() {
        let mut store = PatternStore::new();
        store.add_pattern(sample_pattern("low_use", &["common"], 0));
        store.add_pattern(sample_pattern("high_use", &["common"], 100));

        // Both match "common" tag equally (+10), but high_use has +100 usage bonus
        let results = store.find_relevant("common function", 2);
        assert_eq!(results[0].name, "high_use");
    }

    #[test]
    fn test_find_relevant_content_overlap() {
        let mut store = PatternStore::new();
        // Pattern with malloc in c_pattern
        store.add_pattern(MigrationPattern {
            name: "malloc_pattern".to_string(),
            c_pattern: "int *arr = (int *)malloc(n * sizeof(int));".to_string(),
            rust_pattern: "let arr: Vec<i32> = vec![0; n];".to_string(),
            tags: vec!["malloc".to_string()],
            usage_count: 0,
        });
        // Pattern without malloc
        store.add_pattern(MigrationPattern {
            name: "other_pattern".to_string(),
            c_pattern: "void process(const char *data, size_t len);".to_string(),
            rust_pattern: "fn process(data: &[u8]);".to_string(),
            tags: vec!["other".to_string()],
            usage_count: 0,
        });

        let results = store.find_relevant("int *buf = malloc(size);", 2);
        assert_eq!(results[0].name, "malloc_pattern");
    }

    #[test]
    fn test_find_relevant_max_results() {
        let mut store = PatternStore::new();
        for i in 0..10 {
            store.add_pattern(sample_pattern(&format!("p{}", i), &["tag"], 0));
        }
        let results = store.find_relevant("tag", 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_record_usage() {
        let mut store = PatternStore::new();
        store.add_pattern(sample_pattern("p1", &["tag"], 0));
        store.add_pattern(sample_pattern("p2", &["tag"], 0));

        store.record_usage("p1");
        store.record_usage("p1");
        store.record_usage("p2");

        let results = store.find_relevant("tag", 2);
        // p1 has usage 2, p2 has usage 1, so p1 should come first
        assert_eq!(results[0].name, "p1");
    }

    #[test]
    fn test_record_usage_nonexistent_pattern() {
        let mut store = PatternStore::new();
        store.add_pattern(sample_pattern("p1", &["tag"], 0));
        // Should not panic
        store.record_usage("nonexistent");
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_parse_pattern_markdown() {
        let md = r#"# Pattern: Test

## C Pattern
```c
void f(int *p, size_t n) {
    for (size_t i = 0; i < n; i++) p[i] = 0;
}
```

## Rust Equivalent
```rust
fn f(p: &mut [i32]) {
    p.fill(0);
}
```

## When to Apply
- something
"#;
        let pattern = parse_pattern_markdown(md, "test", vec!["test".to_string()]).unwrap();
        assert_eq!(pattern.name, "test");
        assert!(pattern.c_pattern.contains("void f(int *p"));
        assert!(pattern.rust_pattern.contains("fn f(p: &mut [i32])"));
    }

    #[test]
    fn test_parse_pattern_markdown_missing_section() {
        let md = "# Just a title\nSome text without code blocks.";
        let result = parse_pattern_markdown(md, "test", vec![]);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_code_after_header() {
        let content = r#"## C Pattern
```c
int add(int a, int b) { return a + b; }
```
"#;
        let code = extract_code_after_header(content, "C Pattern").unwrap();
        assert_eq!(code, "int add(int a, int b) { return a + b; }");
    }

    #[test]
    fn test_load_seed_patterns() {
        let store = PatternStore::load_seed_patterns();
        // Should load the 3 pattern files from patterns/
        assert!(
            store.len() >= 3,
            "expected at least 3 seed patterns, got {}",
            store.len()
        );

        // Verify a known pattern exists
        let names: Vec<&str> = store.patterns.iter().map(|p| p.name.as_str()).collect();
        assert!(
            names.contains(&"ptr_to_slice"),
            "missing ptr_to_slice pattern"
        );
        assert!(
            names.contains(&"error_to_result"),
            "missing error_to_result pattern"
        );
        assert!(
            names.contains(&"malloc_to_vec"),
            "missing malloc_to_vec pattern"
        );
    }

    #[test]
    fn test_seed_patterns_have_content() {
        let store = PatternStore::load_seed_patterns();
        for pattern in &store.patterns {
            assert!(
                !pattern.c_pattern.is_empty(),
                "{} has empty c_pattern",
                pattern.name
            );
            assert!(
                !pattern.rust_pattern.is_empty(),
                "{} has empty rust_pattern",
                pattern.name
            );
            assert!(!pattern.tags.is_empty(), "{} has no tags", pattern.name);
        }
    }

    #[test]
    fn test_default_trait() {
        let store = PatternStore::default();
        assert!(store.is_empty());
    }
}
