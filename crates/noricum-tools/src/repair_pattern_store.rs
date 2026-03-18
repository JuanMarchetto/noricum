/// Persistent store for repair patterns extracted from successful fix cycles.
///
/// When the repair agent (or rule engine) fixes a compilation error, the
/// error-fix pair is captured as a [`RepairPattern`]. Before future LLM repair
/// calls, the store is consulted: if a stored pattern matches the current error,
/// the fix is applied deterministically (zero LLM cost).
///
/// Patterns accumulate a `success_count` / `failure_count` ratio. Patterns with
/// a success rate below 50% are excluded from matching to avoid regressions.
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};

/// A single repair pattern: maps a compiler error to a known fix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairPattern {
    /// Rustc error code (e.g., "E0599", "SYNTAX").
    pub error_code: String,
    /// Regex pattern matching the error message text.
    /// Use `.+` for wildcards, literal text for specifics.
    pub error_message_regex: String,
    /// Human-readable description of the fix.
    pub fix_description: String,
    /// Unified diff of the fix (before/after). Used for logging and debugging;
    /// the actual fix is applied via the repair agent or rule engine.
    pub fix_diff: String,
    /// Number of times this pattern was applied and the resulting code compiled.
    pub success_count: u32,
    /// Number of times this pattern was applied and the resulting code did NOT compile.
    pub failure_count: u32,
    /// Project that first produced this pattern.
    pub source_project: String,
    /// ISO date when this pattern was first created.
    pub created_at: String,
}

impl RepairPattern {
    /// Whether this pattern has a >= 50% success rate and at least 1 success.
    pub fn is_reliable(&self) -> bool {
        self.success_count > 0
            && (self.failure_count == 0
                || self.success_count as f64 / (self.success_count + self.failure_count) as f64
                    >= 0.5)
    }

    /// Check if this pattern matches a given error code and message.
    pub fn matches(&self, error_code: &str, error_message: &str) -> bool {
        if self.error_code != error_code {
            return false;
        }
        // Try exact substring first (fast path)
        if error_message.contains(&self.error_message_regex) {
            return true;
        }
        // Try as regex
        match Regex::new(&self.error_message_regex) {
            Ok(re) => re.is_match(error_message),
            Err(_) => false,
        }
    }
}

/// In-memory store of repair patterns with disk persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairPatternStore {
    patterns: Vec<RepairPattern>,
}

impl RepairPatternStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// Load patterns from a JSON file. Returns empty store if file doesn't exist.
    pub fn load_from(path: &Path) -> Result<Self, std::io::Error> {
        if !path.exists() {
            debug!(path = %path.display(), "repair pattern store not found, starting empty");
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)?;
        let store: Self = serde_json::from_str(&content).map_err(|e| {
            warn!(error = %e, "failed to parse repair pattern store, starting empty");
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
        })?;
        info!(count = store.patterns.len(), "loaded repair pattern store");
        Ok(store)
    }

    /// Save patterns to a JSON file, creating parent directories as needed.
    pub fn save_to(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        std::fs::write(path, json)?;
        debug!(count = self.patterns.len(), path = %path.display(), "saved repair pattern store");
        Ok(())
    }

    /// Load from the default location: `.noricum-cache/repair-patterns.json`.
    pub fn load_default() -> Self {
        let path = std::path::PathBuf::from(".noricum-cache/repair-patterns.json");
        Self::load_from(&path).unwrap_or_else(|e| {
            warn!(error = %e, "failed to load repair patterns, using empty store");
            Self::new()
        })
    }

    /// Save to the default location: `.noricum-cache/repair-patterns.json`.
    pub fn save_default(&self) {
        let path = std::path::PathBuf::from(".noricum-cache/repair-patterns.json");
        if let Err(e) = self.save_to(&path) {
            warn!(error = %e, "failed to save repair patterns");
        }
    }

    /// Add a pattern, merging with existing if error_code + error_message_regex match.
    pub fn add_pattern(&mut self, pattern: RepairPattern) {
        // Check for existing pattern with same key
        if let Some(existing) = self.patterns.iter_mut().find(|p| {
            p.error_code == pattern.error_code
                && p.error_message_regex == pattern.error_message_regex
        }) {
            existing.success_count += pattern.success_count;
            existing.failure_count += pattern.failure_count;
            return;
        }
        self.patterns.push(pattern);
    }

    /// Find all reliable patterns matching a given error.
    ///
    /// Returns patterns sorted by success_count descending (most proven first).
    pub fn find_matching(&self, error_code: &str, error_message: &str) -> Vec<&RepairPattern> {
        let mut matches: Vec<&RepairPattern> = self
            .patterns
            .iter()
            .filter(|p| p.is_reliable() && p.matches(error_code, error_message))
            .collect();
        matches.sort_by(|a, b| b.success_count.cmp(&a.success_count));
        matches
    }

    /// Record a successful application of a pattern (error was fixed).
    pub fn record_success(&mut self, error_code: &str, error_message_regex: &str) {
        if let Some(p) = self.patterns.iter_mut().find(|p| {
            p.error_code == error_code && p.error_message_regex == error_message_regex
        }) {
            p.success_count += 1;
        }
    }

    /// Record a failed application of a pattern (error persisted after fix).
    pub fn record_failure(&mut self, error_code: &str, error_message_regex: &str) {
        if let Some(p) = self.patterns.iter_mut().find(|p| {
            p.error_code == error_code && p.error_message_regex == error_message_regex
        }) {
            p.failure_count += 1;
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

    /// Read-only access to all patterns.
    pub fn patterns(&self) -> &[RepairPattern] {
        &self.patterns
    }

    /// Load patterns from file, merging with built-in seed patterns.
    ///
    /// Seed patterns are derived from the manually-extracted R5-R14 repair rules
    /// that eliminated 99.8% of errors in miniz_zip.c Run 15. These provide a
    /// baseline even on first run with an empty store.
    pub fn load_with_seeds(path: &Path) -> Self {
        let mut store = Self::load_from(path).unwrap_or_else(|_| Self::new());

        // Seed patterns from R1-R14 manual analysis
        let seeds = vec![
            RepairPattern {
                error_code: "E0599".to_string(),
                error_message_regex: "trait bounds were not satisfied".to_string(),
                fix_description: "R1: Add Clone bound to generic type parameter".to_string(),
                fix_diff: "fn foo<T: Trait>(x: T) -> fn foo<T: Trait + Clone>(x: T)".to_string(),
                success_count: 50,
                failure_count: 0,
                source_project: "miniz_zip".to_string(),
                created_at: "2026-03-10".to_string(),
            },
            RepairPattern {
                error_code: "E0428".to_string(),
                error_message_regex: "the name .+ is defined multiple times".to_string(),
                fix_description: "R2: Remove duplicate function/struct definition (keep first)"
                    .to_string(),
                fix_diff: "Remove second definition of the duplicated item".to_string(),
                success_count: 30,
                failure_count: 0,
                source_project: "miniz_zip".to_string(),
                created_at: "2026-03-10".to_string(),
            },
            RepairPattern {
                error_code: "E0382".to_string(),
                error_message_regex: "use of moved value.*Option<&mut".to_string(),
                fix_description: "R3: Change Option<&mut T> parameter to use ref mut in match"
                    .to_string(),
                fix_diff: "p: Option<&mut T> -> mut p: Option<&mut T>, match uses ref mut"
                    .to_string(),
                success_count: 15,
                failure_count: 0,
                source_project: "miniz_zip".to_string(),
                created_at: "2026-03-10".to_string(),
            },
            RepairPattern {
                error_code: "E0753".to_string(),
                error_message_regex: "inner attributes.*not permitted in this context".to_string(),
                fix_description: "R5: Convert inner attribute #![...] to outer attribute #[...]"
                    .to_string(),
                fix_diff: "#![allow(...)] -> #[allow(...)]".to_string(),
                success_count: 20,
                failure_count: 0,
                source_project: "miniz_zip".to_string(),
                created_at: "2026-03-10".to_string(),
            },
            RepairPattern {
                error_code: "E0252".to_string(),
                error_message_regex: "the name .+ is defined multiple times".to_string(),
                fix_description: "R7: Remove duplicate use/import statement".to_string(),
                fix_diff: "Remove second `use` statement importing the same item".to_string(),
                success_count: 40,
                failure_count: 0,
                source_project: "miniz_zip".to_string(),
                created_at: "2026-03-10".to_string(),
            },
            RepairPattern {
                error_code: "E0119".to_string(),
                error_message_regex: "conflicting implementations of trait".to_string(),
                fix_description: "R9: Remove duplicate trait impl (keep first)".to_string(),
                fix_diff: "Remove second `impl Trait for Type` block".to_string(),
                success_count: 10,
                failure_count: 0,
                source_project: "miniz_zip".to_string(),
                created_at: "2026-03-10".to_string(),
            },
            RepairPattern {
                error_code: "E0463".to_string(),
                error_message_regex: "can't find crate for".to_string(),
                fix_description:
                    "R10: Strip external crate imports not available in standalone compilation"
                        .to_string(),
                fix_diff: "Remove `use external_crate::...;` or `extern crate ...;`".to_string(),
                success_count: 25,
                failure_count: 0,
                source_project: "miniz_zip".to_string(),
                created_at: "2026-03-10".to_string(),
            },
            RepairPattern {
                error_code: "E0425".to_string(),
                error_message_regex: "cannot find value .+ in this scope".to_string(),
                fix_description:
                    "R13: Field name prefix normalization — m_xyz vs xyz mismatch".to_string(),
                fix_diff:
                    "self.m_field -> self.field (or vice versa, matching struct definition)"
                        .to_string(),
                success_count: 20,
                failure_count: 2,
                source_project: "miniz_zip".to_string(),
                created_at: "2026-03-17".to_string(),
            },
            RepairPattern {
                error_code: "E0599".to_string(),
                error_message_regex: "no method named .+ found for .+ in the current scope"
                    .to_string(),
                fix_description: "R14: Method-to-free-function — self.fn() -> fn(self)".to_string(),
                fix_diff: "self.some_fn(args) -> some_fn(&mut self, args)".to_string(),
                success_count: 17,
                failure_count: 3,
                source_project: "miniz_zip".to_string(),
                created_at: "2026-03-17".to_string(),
            },
        ];

        for seed in seeds {
            // Only add if not already present (don't overwrite learned counts)
            if !store.patterns.iter().any(|p| {
                p.error_code == seed.error_code
                    && p.error_message_regex == seed.error_message_regex
            }) {
                store.patterns.push(seed);
            }
        }

        store
    }

    /// Load from default path with seed patterns.
    pub fn load_default_with_seeds() -> Self {
        let path = std::path::PathBuf::from(".noricum-cache/repair-patterns.json");
        Self::load_with_seeds(&path)
    }
}

impl Default for RepairPatternStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract repair patterns by diffing before/after repair and correlating with fixed errors.
///
/// For each error in `errors_before` that is NOT present in `errors_after` (matched by
/// error code + line proximity), a `RepairPattern` is created with the error details
/// and a text diff of the changed region.
///
/// This is called after each successful repair iteration in the orchestrator.
pub fn extract_patterns_from_diff(
    before: &str,
    after: &str,
    errors_before: &[crate::repair_rules::CompilerError],
    errors_after: &[crate::repair_rules::CompilerError],
    source_project: &str,
) -> Vec<RepairPattern> {
    let mut patterns = Vec::new();

    // Find errors that were fixed (present in before, absent in after)
    let fixed_errors: Vec<&crate::repair_rules::CompilerError> = errors_before
        .iter()
        .filter(|eb| {
            // Error is "fixed" if no error in errors_after has the same code
            // near the same line (within 5 lines, since line numbers shift)
            !errors_after.iter().any(|ea| {
                ea.code == eb.code && (ea.line as i64 - eb.line as i64).unsigned_abs() <= 5
            })
        })
        .collect();

    if fixed_errors.is_empty() {
        return patterns;
    }

    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();

    for error in fixed_errors {
        // Extract the changed region around the error line
        let context = 3; // lines of context
        let start = error.line.saturating_sub(context + 1);
        let end = (error.line + context).min(before_lines.len());

        let before_region: String = before_lines
            .get(start..end)
            .map(|s| s.join("\n"))
            .unwrap_or_default();

        let after_start = start;
        let after_end = (error.line + context).min(after_lines.len());
        let after_region: String = after_lines
            .get(after_start..after_end)
            .map(|s| s.join("\n"))
            .unwrap_or_default();

        // Only create a pattern if there's an actual diff
        if before_region != after_region {
            // Create a simplified error message regex by escaping special chars
            // but replacing specific types/names with .+ wildcards
            let error_regex = simplify_error_message(&error.message);

            let fix_diff = format!(
                "--- before (line {})\n{}\n+++ after\n{}",
                error.line, before_region, after_region
            );

            let today = today_iso();

            patterns.push(RepairPattern {
                error_code: error.code.clone(),
                error_message_regex: error_regex,
                fix_description: format!(
                    "Fix {} at line {}: {}",
                    error.code, error.line, error.message
                ),
                fix_diff,
                success_count: 1,
                failure_count: 0,
                source_project: source_project.to_string(),
                created_at: today,
            });
        }
    }

    patterns
}

/// Simplify an error message into a regex pattern.
///
/// Replaces specific identifiers and type names with `.+` wildcards
/// while keeping the structural error message intact.
fn simplify_error_message(message: &str) -> String {
    let mut result = regex::escape(message);
    // Replace quoted identifiers with wildcards: `foo` -> `.+`
    let backtick_re = Regex::new(r"`[^`]+`").expect("static regex");
    result = backtick_re.replace_all(&result, "`.+`").to_string();
    result
}

/// Get today's date in ISO format (YYYY-MM-DD).
/// Falls back to a static string if system time is unavailable.
fn today_iso() -> String {
    // Use a simple approach that doesn't require chrono
    let now = std::time::SystemTime::now();
    let since_epoch = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let days = since_epoch.as_secs() / 86400;
    // Approximate date calculation (good enough for pattern timestamps)
    let year = 1970 + (days / 365);
    let remaining = days % 365;
    let month = remaining / 30 + 1;
    let day = remaining % 30 + 1;
    format!("{year:04}-{month:02}-{day:02}")
}

/// Build repair pattern context for injection into repair prompts.
///
/// For each error (up to 5), looks up matching patterns from the store and
/// formats them as hints for the repair agent.
pub fn build_repair_pattern_context(
    errors: &[crate::repair_rules::CompilerError],
    store: &RepairPatternStore,
) -> String {
    let mut context = String::new();
    for err in errors.iter().take(5) {
        let matches = store.find_matching(&err.code, &err.message);
        if let Some(pattern) = matches.first() {
            context.push_str(&format!(
                "\n## Known fix for {} (from previous project: {})\n{}\n```\n{}\n```\n",
                err.code, pattern.source_project, pattern.fix_description, pattern.fix_diff
            ));
        }
    }
    context
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_repair_pattern_creation() {
        let pattern = RepairPattern {
            error_code: "E0599".to_string(),
            error_message_regex:
                "no method named `clone`.*trait bounds were not satisfied".to_string(),
            fix_description: "Add Clone bound to generic type parameter".to_string(),
            fix_diff: "- fn foo<T: Trait>(x: T)\n+ fn foo<T: Trait + Clone>(x: T)".to_string(),
            success_count: 1,
            failure_count: 0,
            source_project: "miniz_zip".to_string(),
            created_at: "2026-03-17".to_string(),
        };
        assert_eq!(pattern.error_code, "E0599");
        assert_eq!(pattern.success_count, 1);
        assert!(pattern.is_reliable());
    }

    #[test]
    fn test_store_add_and_find() {
        let mut store = RepairPatternStore::new();
        store.add_pattern(RepairPattern {
            error_code: "E0599".to_string(),
            error_message_regex: "no method named `clone`".to_string(),
            fix_description: "Add Clone bound".to_string(),
            fix_diff: "+Clone".to_string(),
            success_count: 3,
            failure_count: 0,
            source_project: "test".to_string(),
            created_at: "2026-03-17".to_string(),
        });
        store.add_pattern(RepairPattern {
            error_code: "E0425".to_string(),
            error_message_regex: "cannot find value".to_string(),
            fix_description: "Import missing symbol".to_string(),
            fix_diff: "+use".to_string(),
            success_count: 1,
            failure_count: 0,
            source_project: "test".to_string(),
            created_at: "2026-03-17".to_string(),
        });

        let matches = store.find_matching(
            "E0599",
            "no method named `clone` for type T: trait bounds were not satisfied",
        );
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].fix_description, "Add Clone bound");
    }

    #[test]
    fn test_store_no_match() {
        let mut store = RepairPatternStore::new();
        store.add_pattern(RepairPattern {
            error_code: "E0599".to_string(),
            error_message_regex: "no method named `clone`".to_string(),
            fix_description: "Add Clone bound".to_string(),
            fix_diff: "+Clone".to_string(),
            success_count: 1,
            failure_count: 0,
            source_project: "test".to_string(),
            created_at: "2026-03-17".to_string(),
        });

        let matches = store.find_matching("E0308", "mismatched types");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_store_persistence_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store_path = tmp.path().join("repair-patterns.json");

        let mut store = RepairPatternStore::new();
        store.add_pattern(RepairPattern {
            error_code: "E0599".to_string(),
            error_message_regex: "clone".to_string(),
            fix_description: "Add Clone".to_string(),
            fix_diff: "+Clone".to_string(),
            success_count: 5,
            failure_count: 1,
            source_project: "test".to_string(),
            created_at: "2026-03-17".to_string(),
        });

        store.save_to(&store_path).unwrap();
        let loaded = RepairPatternStore::load_from(&store_path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.patterns()[0].success_count, 5);
    }

    #[test]
    fn test_store_load_nonexistent_returns_empty() {
        let store =
            RepairPatternStore::load_from(std::path::Path::new("/nonexistent/path.json"));
        assert!(store.is_ok());
        assert!(store.unwrap().is_empty());
    }

    #[test]
    fn test_record_success_increments_count() {
        let mut store = RepairPatternStore::new();
        store.add_pattern(RepairPattern {
            error_code: "E0599".to_string(),
            error_message_regex: "clone".to_string(),
            fix_description: "Add Clone".to_string(),
            fix_diff: "+Clone".to_string(),
            success_count: 1,
            failure_count: 0,
            source_project: "test".to_string(),
            created_at: "2026-03-17".to_string(),
        });

        store.record_success("E0599", "clone");
        assert_eq!(store.patterns()[0].success_count, 2);
    }

    #[test]
    fn test_record_failure_increments_count() {
        let mut store = RepairPatternStore::new();
        store.add_pattern(RepairPattern {
            error_code: "E0599".to_string(),
            error_message_regex: "clone".to_string(),
            fix_description: "Add Clone".to_string(),
            fix_diff: "+Clone".to_string(),
            success_count: 3,
            failure_count: 0,
            source_project: "test".to_string(),
            created_at: "2026-03-17".to_string(),
        });

        store.record_failure("E0599", "clone");
        assert_eq!(store.patterns()[0].failure_count, 1);
    }

    #[test]
    fn test_unreliable_pattern_excluded_from_find() {
        let mut store = RepairPatternStore::new();
        store.add_pattern(RepairPattern {
            error_code: "E0599".to_string(),
            error_message_regex: "clone".to_string(),
            fix_description: "Add Clone".to_string(),
            fix_diff: "+Clone".to_string(),
            success_count: 1,
            failure_count: 5, // 5 failures, 1 success — unreliable
            source_project: "test".to_string(),
            created_at: "2026-03-17".to_string(),
        });

        let matches = store.find_matching("E0599", "clone error");
        assert!(matches.is_empty(), "unreliable patterns should be excluded");
    }

    #[test]
    fn test_dedup_on_add() {
        let mut store = RepairPatternStore::new();
        let p = RepairPattern {
            error_code: "E0599".to_string(),
            error_message_regex: "clone".to_string(),
            fix_description: "Add Clone".to_string(),
            fix_diff: "+Clone".to_string(),
            success_count: 1,
            failure_count: 0,
            source_project: "test".to_string(),
            created_at: "2026-03-17".to_string(),
        };
        store.add_pattern(p.clone());
        store.add_pattern(p); // duplicate
        assert_eq!(
            store.len(),
            1,
            "duplicate should merge, incrementing success_count"
        );
        assert_eq!(store.patterns()[0].success_count, 2);
    }

    // --- Task 2: extract_patterns_from_diff tests ---

    #[test]
    fn test_extract_patterns_from_diff() {
        let before = "fn foo<T: Display>(x: T) {\n    x.clone();\n}\n";
        let after = "fn foo<T: Display + Clone>(x: T) {\n    x.clone();\n}\n";
        let errors_before = vec![crate::repair_rules::CompilerError {
            code: "E0599".to_string(),
            line: 2,
            message:
                "no method named `clone` found for type `T`: trait bounds were not satisfied"
                    .to_string(),
        }];
        let errors_after: Vec<crate::repair_rules::CompilerError> = vec![]; // no errors

        let patterns =
            extract_patterns_from_diff(before, after, &errors_before, &errors_after, "test_project");
        assert_eq!(patterns.len(), 1, "should extract 1 pattern for fixed error");
        assert_eq!(patterns[0].error_code, "E0599");
        assert!(patterns[0].fix_diff.contains("Clone"));
    }

    #[test]
    fn test_extract_no_patterns_when_errors_remain() {
        let before = "fn foo() { x.clone(); }";
        let after = "fn foo() { x.clone(); }"; // unchanged
        let errors_before = vec![crate::repair_rules::CompilerError {
            code: "E0599".to_string(),
            line: 1,
            message: "no method named `clone`".to_string(),
        }];
        let errors_after = errors_before.clone(); // same errors

        let patterns =
            extract_patterns_from_diff(before, after, &errors_before, &errors_after, "test_project");
        assert!(
            patterns.is_empty(),
            "should not extract patterns when errors persist"
        );
    }

    #[test]
    fn test_extract_multiple_fixed_errors() {
        let before = "fn foo() {}\nfn bar() {}";
        let after = "fn foo() -> i32 { 0 }\nfn bar() -> String { String::new() }";
        let errors_before = vec![
            crate::repair_rules::CompilerError {
                code: "E0308".to_string(),
                line: 1,
                message: "mismatched types: expected i32".to_string(),
            },
            crate::repair_rules::CompilerError {
                code: "E0308".to_string(),
                line: 2,
                message: "mismatched types: expected String".to_string(),
            },
        ];
        let errors_after: Vec<crate::repair_rules::CompilerError> = vec![];

        let patterns =
            extract_patterns_from_diff(before, after, &errors_before, &errors_after, "test");
        assert_eq!(
            patterns.len(),
            2,
            "should extract pattern for each fixed error"
        );
    }

    // --- Task 4: Seed pattern tests ---

    #[test]
    fn test_seed_patterns_loaded() {
        let store = RepairPatternStore::load_with_seeds(std::path::Path::new("/nonexistent"));
        // Should have at least the seed patterns even with no saved file
        assert!(
            store.len() >= 5,
            "should have seed patterns, got {}",
            store.len()
        );

        // Verify known patterns exist
        let clone_matches = store.find_matching("E0599", "trait bounds were not satisfied");
        assert!(
            !clone_matches.is_empty(),
            "should have E0599 Clone bound seed pattern"
        );

        let dup_matches = store.find_matching("E0428", "the name `Foo` is defined multiple times");
        assert!(
            !dup_matches.is_empty(),
            "should have E0428 duplicate definition seed pattern"
        );
    }

    #[test]
    fn test_seed_patterns_not_duplicated_on_reload() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("repair-patterns.json");

        // First load creates seeds
        let store1 = RepairPatternStore::load_with_seeds(&path);
        let count1 = store1.len();
        store1.save_to(&path).unwrap();

        // Second load should not duplicate seeds
        let store2 = RepairPatternStore::load_with_seeds(&path);
        assert_eq!(
            store2.len(),
            count1,
            "seeds should not duplicate on reload"
        );
    }

    // --- Task 3: build_repair_pattern_context ---

    #[test]
    fn test_build_repair_pattern_context_with_matches() {
        let mut store = RepairPatternStore::new();
        store.add_pattern(RepairPattern {
            error_code: "E0599".to_string(),
            error_message_regex: "no method named".to_string(),
            fix_description: "Add Clone bound".to_string(),
            fix_diff: "+Clone".to_string(),
            success_count: 5,
            failure_count: 0,
            source_project: "test".to_string(),
            created_at: "2026-03-17".to_string(),
        });

        let errors = vec![crate::repair_rules::CompilerError {
            code: "E0599".to_string(),
            line: 10,
            message: "no method named `clone`".to_string(),
        }];

        let context = build_repair_pattern_context(&errors, &store);
        assert!(context.contains("Known fix for E0599"));
        assert!(context.contains("Add Clone bound"));
    }

    #[test]
    fn test_build_repair_pattern_context_empty_when_no_matches() {
        let store = RepairPatternStore::new();
        let errors = vec![crate::repair_rules::CompilerError {
            code: "E0308".to_string(),
            line: 1,
            message: "mismatched types".to_string(),
        }];

        let context = build_repair_pattern_context(&errors, &store);
        assert!(context.is_empty());
    }

    #[test]
    fn test_simplify_error_message() {
        let msg = "no method named `clone` found for type `T`";
        let simplified = simplify_error_message(msg);
        // Should replace backtick-quoted identifiers with `.+`
        assert!(simplified.contains("`.+`"));
        assert!(!simplified.contains("`clone`"));
    }
}
