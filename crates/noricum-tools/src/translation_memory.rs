/// Translation memory: stores successful C-to-Rust function translations
/// for few-shot injection into future translation prompts.
///
/// Entries are indexed by structural signature similarity -- when translating
/// a new C function, the top-k most similar previously-translated functions
/// are retrieved and injected as examples.
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};

/// Maximum number of entries in the translation memory.
/// Oldest/lowest-scored entries are evicted when this is exceeded.
const MAX_ENTRIES: usize = 1000;

/// A single translation memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationEntry {
    /// C function signature (return type + name + parameter types).
    pub c_signature: String,
    /// Full C source of the function.
    pub c_source: String,
    /// Validated Rust translation.
    pub rust_source: String,
    /// Hash of the normalized signature for fast lookup.
    pub signature_hash: String,
    /// Idiomatic score of the Rust translation (0-100).
    pub idiomatic_score: u32,
    /// Project that produced this translation.
    pub source_project: String,
    /// ISO date when this entry was created.
    pub created_at: String,
}

/// In-memory translation memory with disk persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationMemory {
    entries: Vec<TranslationEntry>,
}

impl TranslationMemory {
    /// Create an empty translation memory.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Load from a JSON file. Returns empty memory if file doesn't exist.
    pub fn load_from(path: &Path) -> Result<Self, std::io::Error> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)?;
        let memory: Self = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        info!(entries = memory.entries.len(), "loaded translation memory");
        Ok(memory)
    }

    /// Save to a JSON file.
    pub fn save_to(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json =
            serde_json::to_string_pretty(self).map_err(|e| std::io::Error::other(e.to_string()))?;
        std::fs::write(path, json)?;
        debug!(entries = self.entries.len(), path = %path.display(), "saved translation memory");
        Ok(())
    }

    /// Load from default path: `.noricum-cache/translation-memory.json`.
    pub fn load_default() -> Self {
        let path = std::path::PathBuf::from(".noricum-cache/translation-memory.json");
        Self::load_from(&path).unwrap_or_else(|e| {
            warn!(error = %e, "failed to load translation memory");
            Self::new()
        })
    }

    /// Save to default path.
    pub fn save_default(&self) {
        let path = std::path::PathBuf::from(".noricum-cache/translation-memory.json");
        if let Err(e) = self.save_to(&path) {
            warn!(error = %e, "failed to save translation memory");
        }
    }

    /// Add a translation entry, enforcing MAX_ENTRIES by evicting lowest-scored.
    pub fn add_entry(&mut self, entry: TranslationEntry) {
        // Deduplicate: if same signature hash exists, keep higher score
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.signature_hash == entry.signature_hash)
        {
            if entry.idiomatic_score > existing.idiomatic_score {
                *existing = entry;
            }
            return;
        }

        self.entries.push(entry);

        // Evict lowest-scored entries if over limit
        if self.entries.len() > MAX_ENTRIES {
            self.entries
                .sort_by(|a, b| b.idiomatic_score.cmp(&a.idiomatic_score));
            self.entries.truncate(MAX_ENTRIES);
        }
    }

    /// Find the top-k most similar translations for a given C function signature.
    ///
    /// Similarity is based on:
    /// 1. Return type match (+10)
    /// 2. Parameter count match (+5)
    /// 3. Parameter type overlap (+3 each)
    /// 4. Idiomatic score bonus (+score/10)
    pub fn find_similar(&self, c_signature: &str, max_results: usize) -> Vec<&TranslationEntry> {
        let query = parse_signature(c_signature);

        let mut scored: Vec<(u32, &TranslationEntry)> = self
            .entries
            .iter()
            .map(|entry| {
                let stored = parse_signature(&entry.c_signature);
                let mut score: u32 = 0;

                // Return type match
                if query.return_type == stored.return_type {
                    score += 10;
                }

                // Parameter count match
                if query.param_types.len() == stored.param_types.len() {
                    score += 5;
                }

                // Parameter type overlap
                for qt in &query.param_types {
                    if stored.param_types.contains(qt) {
                        score += 3;
                    }
                }

                // Score bonus (higher quality translations are more useful as examples)
                score += entry.idiomatic_score / 10;

                (score, entry)
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored
            .into_iter()
            .take(max_results)
            .map(|(_, entry)| entry)
            .collect()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the memory is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Read-only access to all entries.
    pub fn entries(&self) -> &[TranslationEntry] {
        &self.entries
    }
}

impl Default for TranslationMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute a deterministic hash of a normalized C function signature.
pub fn signature_hash(signature: &str) -> String {
    use sha2::{Digest, Sha256};
    let normalized = normalize_signature(signature);
    let hash = Sha256::digest(normalized.as_bytes());
    format!(
        "{:016x}",
        u64::from_be_bytes(hash[..8].try_into().unwrap_or([0; 8]))
    )
}

/// Normalize a C signature by collapsing whitespace and removing parameter names.
fn normalize_signature(sig: &str) -> String {
    // Collapse whitespace
    let collapsed = Regex::new(r"\s+")
        .expect("static regex")
        .replace_all(sig, " ")
        .to_string();
    // Normalize spaces around punctuation: "( int a , int b )" -> "(int a, int b)"
    let normalized = Regex::new(r"\s*([(),])\s*")
        .expect("static regex")
        .replace_all(&collapsed, "$1")
        .to_string();
    // Re-add space after comma for readability and consistent parsing
    let normalized = normalized.replace(",", ", ");
    // Remove parameter names (keep types only)
    let no_names = Regex::new(r"(\w+)\s+\w+([,)])")
        .expect("static regex")
        .replace_all(&normalized, "$1$2")
        .to_string();
    no_names.trim().to_string()
}

/// Parsed components of a C function signature.
struct ParsedSignature {
    return_type: String,
    param_types: Vec<String>,
}

/// Extract return type and parameter types from a C signature.
fn parse_signature(sig: &str) -> ParsedSignature {
    let normalized = normalize_signature(sig);

    // Extract return type (everything before the function name and '(')
    let return_type = if let Some(paren_pos) = normalized.find('(') {
        let before_paren = &normalized[..paren_pos];
        // Return type is everything before the last word (function name)
        let parts: Vec<&str> = before_paren.trim().rsplitn(2, ' ').collect();
        parts.last().unwrap_or(&"void").to_string()
    } else {
        "void".to_string()
    };

    // Extract parameter types
    let param_str = if let (Some(start), Some(end)) = (normalized.find('('), normalized.rfind(')'))
    {
        &normalized[start + 1..end]
    } else {
        ""
    };

    let param_types: Vec<String> = if param_str.trim().is_empty() || param_str.trim() == "void" {
        Vec::new()
    } else {
        param_str
            .split(',')
            .map(|p| {
                let trimmed = p.trim();
                // Extract type (first word(s), ignoring parameter name)
                let words: Vec<&str> = trimmed.split_whitespace().collect();
                if words.len() >= 2 {
                    // All but last word are the type
                    words[..words.len() - 1].join(" ")
                } else {
                    trimmed.to_string()
                }
            })
            .collect()
    };

    ParsedSignature {
        return_type,
        param_types,
    }
}

/// Truncate source code to the first N lines.
pub fn truncate_to_lines(source: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = source.lines().collect();
    if lines.len() <= max_lines {
        source.to_string()
    } else {
        let kept: String = lines[..max_lines].join("\n");
        format!("{kept}\n// ... ({} more lines)", lines.len() - max_lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_entry_creation() {
        let entry = TranslationEntry {
            c_signature: "int add(int a, int b)".to_string(),
            c_source: "int add(int a, int b) { return a + b; }".to_string(),
            rust_source: "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
            signature_hash: signature_hash("int add(int a, int b)"),
            idiomatic_score: 95,
            source_project: "simple".to_string(),
            created_at: "2026-03-17".to_string(),
        };
        assert!(!entry.signature_hash.is_empty());
    }

    #[test]
    fn test_memory_add_and_find_exact() {
        let mut memory = TranslationMemory::new();
        memory.add_entry(TranslationEntry {
            c_signature: "int add(int a, int b)".to_string(),
            c_source: "int add(int a, int b) { return a + b; }".to_string(),
            rust_source: "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
            signature_hash: signature_hash("int add(int a, int b)"),
            idiomatic_score: 95,
            source_project: "simple".to_string(),
            created_at: "2026-03-17".to_string(),
        });

        let results = memory.find_similar("int subtract(int a, int b)", 3);
        assert!(
            !results.is_empty(),
            "similar signature (int,int->int) should match"
        );
    }

    #[test]
    fn test_memory_returns_highest_score_first() {
        let mut memory = TranslationMemory::new();
        memory.add_entry(TranslationEntry {
            c_signature: "int foo(int x)".to_string(),
            c_source: "int foo(int x) { return x; }".to_string(),
            rust_source: "fn foo(x: i32) -> i32 { x }".to_string(),
            signature_hash: signature_hash("int foo(int x)"),
            idiomatic_score: 60,
            source_project: "low".to_string(),
            created_at: "2026-03-17".to_string(),
        });
        memory.add_entry(TranslationEntry {
            c_signature: "int bar(int y)".to_string(),
            c_source: "int bar(int y) { return y * 2; }".to_string(),
            rust_source: "fn bar(y: i32) -> i32 { y * 2 }".to_string(),
            signature_hash: signature_hash("int bar(int y)"),
            idiomatic_score: 95,
            source_project: "high".to_string(),
            created_at: "2026-03-17".to_string(),
        });

        let results = memory.find_similar("int baz(int z)", 2);
        assert!(results.len() >= 2);
        assert!(
            results[0].idiomatic_score >= results[1].idiomatic_score,
            "higher score should come first"
        );
    }

    #[test]
    fn test_memory_persistence_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("translation-memory.json");

        let mut memory = TranslationMemory::new();
        memory.add_entry(TranslationEntry {
            c_signature: "int add(int a, int b)".to_string(),
            c_source: "int add(int a, int b) { return a + b; }".to_string(),
            rust_source: "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
            signature_hash: signature_hash("int add(int a, int b)"),
            idiomatic_score: 95,
            source_project: "test".to_string(),
            created_at: "2026-03-17".to_string(),
        });

        memory.save_to(&path).unwrap();
        let loaded = TranslationMemory::load_from(&path).unwrap();
        assert_eq!(loaded.len(), 1);
    }

    #[test]
    fn test_memory_max_entries() {
        let mut memory = TranslationMemory::new();
        for i in 0..1500 {
            memory.add_entry(TranslationEntry {
                c_signature: format!("int fn{i}(int x)"),
                c_source: format!("int fn{i}(int x) {{ return x; }}"),
                rust_source: format!("fn fn{i}(x: i32) -> i32 {{ x }}"),
                signature_hash: signature_hash(&format!("int fn{i}(int x)")),
                idiomatic_score: 50,
                source_project: "bulk".to_string(),
                created_at: "2026-03-17".to_string(),
            });
        }
        // Should cap at MAX_ENTRIES, evicting lowest-scored entries
        assert!(memory.len() <= 1000, "should cap at MAX_ENTRIES");
    }

    #[test]
    fn test_signature_hash_deterministic() {
        let h1 = signature_hash("int add(int a, int b)");
        let h2 = signature_hash("int add(int a, int b)");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_signature_hash_normalized() {
        // Extra whitespace should not change the hash
        let h1 = signature_hash("int  add( int a , int  b )");
        let h2 = signature_hash("int add(int a, int b)");
        assert_eq!(h1, h2, "normalized signatures should produce same hash");
    }

    #[test]
    fn test_memory_dedup_keeps_higher_score() {
        let mut memory = TranslationMemory::new();
        let sig = "int add(int a, int b)";
        let hash = signature_hash(sig);

        memory.add_entry(TranslationEntry {
            c_signature: sig.to_string(),
            c_source: "int add(int a, int b) { return a + b; }".to_string(),
            rust_source: "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
            signature_hash: hash.clone(),
            idiomatic_score: 60,
            source_project: "first".to_string(),
            created_at: "2026-03-17".to_string(),
        });
        memory.add_entry(TranslationEntry {
            c_signature: sig.to_string(),
            c_source: "int add(int a, int b) { return a + b; }".to_string(),
            rust_source: "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
            signature_hash: hash,
            idiomatic_score: 95,
            source_project: "second".to_string(),
            created_at: "2026-03-17".to_string(),
        });

        assert_eq!(memory.len(), 1);
        assert_eq!(memory.entries()[0].idiomatic_score, 95);
        assert_eq!(memory.entries()[0].source_project, "second");
    }

    #[test]
    fn test_memory_load_nonexistent_returns_empty() {
        let memory = TranslationMemory::load_from(std::path::Path::new("/nonexistent/path.json"));
        assert!(memory.is_ok());
        assert!(memory.unwrap().is_empty());
    }

    #[test]
    fn test_parse_signature_basic() {
        let parsed = parse_signature("int add(int a, int b)");
        assert_eq!(parsed.return_type, "int");
        assert_eq!(parsed.param_types, vec!["int", "int"]);
    }

    #[test]
    fn test_parse_signature_pointer() {
        let parsed = parse_signature("char *strdup(const char *s)");
        assert_eq!(parsed.return_type, "char");
        assert_eq!(parsed.param_types.len(), 1);
    }

    #[test]
    fn test_parse_signature_void() {
        let parsed = parse_signature("void cleanup(void)");
        assert_eq!(parsed.return_type, "void");
        assert!(parsed.param_types.is_empty());
    }

    #[test]
    fn test_truncate_to_lines() {
        let source = "line1\nline2\nline3\nline4\nline5";
        assert_eq!(
            truncate_to_lines(source, 3),
            "line1\nline2\nline3\n// ... (2 more lines)"
        );
        assert_eq!(truncate_to_lines(source, 10), source);
    }

    #[test]
    fn test_translation_memory_integration_flow() {
        let mut memory = TranslationMemory::new();

        // Simulate adding entries from cJSON validation
        memory.add_entry(TranslationEntry {
            c_signature: "cJSON *cJSON_Parse(const char *value)".to_string(),
            c_source: "cJSON *cJSON_Parse(const char *value) { ... }".to_string(),
            rust_source: "fn parse(value: &str) -> Result<JsonValue, ParseError> { ... }"
                .to_string(),
            signature_hash: signature_hash("cJSON *cJSON_Parse(const char *value)"),
            idiomatic_score: 100,
            source_project: "cjson".to_string(),
            created_at: "2026-03-17".to_string(),
        });

        // Query with similar signature
        let results = memory.find_similar("json_t *json_parse(const char *input)", 3);
        assert!(
            !results.is_empty(),
            "should find similar JSON parsing function"
        );
        assert_eq!(results[0].source_project, "cjson");
    }
}
