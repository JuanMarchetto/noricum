/// Semantic Code Map: tracks migration state and metadata for each code unit.
///
/// This is NOT a compiler IR. C2Rust produces Rust code, LLMs read/write code.
/// The Semantic Code Map tracks what we know about each function being migrated:
/// its source, current state, analysis results, and migration history.
pub mod pattern_store;

use serde::{Deserialize, Serialize};

/// The migration state of a single function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationState {
    /// Not yet processed
    Pending,
    /// Source code extracted and parsed
    Extracted,
    /// Complexity and patterns characterized
    Characterized,
    /// C2Rust mechanical translation done
    C2RustDone,
    /// LLM analysis complete
    Analyzed,
    /// LLM refinement to idiomatic Rust complete
    Refined,
    /// All validation checks passed
    Validated,
    /// In repair loop (iteration count)
    Repairing(u32),
    /// Compiles with unsafe blocks, score >= 50 (P23)
    CompilesUnsafe,
    /// Compiles but below score threshold (P23)
    CompilesLowScore,
    /// Doesn't compile but very close (<=5 errors, score >= 50) (P23)
    NearlyCompiles,
    /// Gave up on safe Rust, keeping unsafe as fallback
    FallbackUnsafe,
}

/// Difficulty classification for model routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    /// Simple arithmetic, string ops, no pointers
    Easy,
    /// Moderate pointer usage, simple structs
    Medium,
    /// Complex pointer arithmetic, void*, callbacks, unions
    Hard,
}

/// A semantic hint detected in C source code.
///
/// These hints provide algorithmic context to the translation agent.
/// They are suggestions, not directives — the LLM uses them as
/// additional context for producing idiomatic Rust translations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticHint {
    /// malloc/free pair detected — suggest RAII (Vec, Box, String)
    MemoryManagement {
        /// C functions involved (e.g., "malloc_node", "free_node")
        functions: Vec<String>,
        /// Suggested Rust approach
        suggestion: String,
    },
    /// Data structure pattern detected
    DataStructure {
        /// Kind: "linked_list", "hash_table", "tree", "stack", "queue", "ring_buffer"
        kind: String,
        /// C functions/structs involved
        involved: Vec<String>,
        /// Suggested Rust type (e.g., `Vec<T>`, `HashMap<K, V>`, `BTreeMap<K, V>`)
        rust_type: String,
    },
    /// Algorithm pattern detected
    Algorithm {
        /// Kind: "sort", "binary_search", "compression", "checksum", "crypto"
        kind: String,
        /// C functions involved
        functions: Vec<String>,
        /// Suggested Rust approach
        suggestion: String,
    },
    /// Control flow pattern detected
    ControlFlow {
        /// Kind: "state_machine", "recursive_descent_parser", "event_loop", "goto_cleanup"
        kind: String,
        /// C functions involved
        functions: Vec<String>,
        /// Suggested Rust approach
        suggestion: String,
    },
    /// I/O pattern detected
    IoPattern {
        /// Kind: "file_readwrite", "buffer_management", "serialization"
        kind: String,
        /// C functions involved
        functions: Vec<String>,
        /// Suggested Rust approach
        suggestion: String,
    },
    /// Concurrency pattern detected
    Concurrency {
        /// Kind: "mutex", "thread_creation", "atomic"
        kind: String,
        /// C functions involved
        functions: Vec<String>,
        /// Suggested Rust approach
        suggestion: String,
    },
}

impl SemanticHint {
    /// Format this hint as a human-readable comment for the translation prompt.
    pub fn to_prompt_line(&self) -> String {
        match self {
            SemanticHint::MemoryManagement {
                functions,
                suggestion,
            } => {
                format!(
                    "- Functions {} implement memory management -> {}",
                    functions.join(", "),
                    suggestion
                )
            }
            SemanticHint::DataStructure {
                kind,
                involved,
                rust_type,
            } => {
                format!(
                    "- {} pattern detected in {} -> consider {}",
                    kind,
                    involved.join(", "),
                    rust_type
                )
            }
            SemanticHint::Algorithm {
                kind,
                functions,
                suggestion,
            } => {
                format!(
                    "- {} algorithm in {} -> {}",
                    kind,
                    functions.join(", "),
                    suggestion
                )
            }
            SemanticHint::ControlFlow {
                kind,
                functions,
                suggestion,
            } => {
                format!(
                    "- {} pattern in {} -> {}",
                    kind,
                    functions.join(", "),
                    suggestion
                )
            }
            SemanticHint::IoPattern {
                kind,
                functions,
                suggestion,
            } => {
                format!(
                    "- {} I/O in {} -> {}",
                    kind,
                    functions.join(", "),
                    suggestion
                )
            }
            SemanticHint::Concurrency {
                kind,
                functions,
                suggestion,
            } => {
                format!(
                    "- {} concurrency in {} -> {}",
                    kind,
                    functions.join(", "),
                    suggestion
                )
            }
        }
    }
}

/// Metadata about a single C function being migrated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionUnit {
    /// Name of the function
    pub name: String,
    /// Source file path
    pub source_path: String,
    /// Original C source code
    pub c_source: String,
    /// Preprocessed C source (macro-expanded, if applicable)
    pub preprocessed_source: Option<String>,
    /// C2Rust output (unsafe Rust)
    pub c2rust_output: Option<String>,
    /// Current best Rust translation
    pub rust_output: Option<String>,
    /// Current migration state
    pub state: MigrationState,
    /// Estimated difficulty
    pub difficulty: Option<Difficulty>,
    /// Functions this one depends on
    pub dependencies: Vec<String>,
    /// Compiler errors from last attempt
    pub last_errors: Vec<String>,
    /// Diff test feedback from last attempt (behavioral mismatch details)
    pub last_diff_feedback: Vec<String>,
    /// Idiomatic score (0-100)
    pub idiomatic_score: Option<u32>,
    /// Number of unsafe blocks in current output
    pub unsafe_count: Option<u32>,
    /// Generated equivalence test code (if any)
    pub generated_tests: Option<String>,
    /// Migration metrics (timing, costs, repair iterations)
    #[serde(default)]
    pub metrics: MigrationMetrics,
}

/// Metrics collected during migration of a single function.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrationMetrics {
    /// Total wall-clock time in milliseconds
    pub total_ms: u64,
    /// Time spent on analysis agent (ms)
    pub analysis_ms: u64,
    /// Time spent on translation agent (ms)
    pub translation_ms: u64,
    /// Time spent on repair iterations total (ms)
    pub repair_ms: u64,
    /// Time spent on test generation (ms)
    pub test_gen_ms: u64,
    /// Number of repair iterations used
    pub repair_iterations: u32,
    /// Number of LLM API calls made
    pub llm_calls: u32,
    /// C source lines of code
    pub c_lines: u32,
    /// Rust output lines of code
    pub rust_lines: u32,
    /// Whether diff test was run and passed
    pub diff_test_passed: Option<bool>,
    /// Whether fuzz test was run and passed
    pub fuzz_test_passed: Option<bool>,
    /// Number of fuzz divergences found
    pub fuzz_divergence_count: u32,
    /// Estimated total input tokens across all LLM calls
    pub input_tokens: u64,
    /// Estimated total output tokens across all LLM calls
    pub output_tokens: u64,
    /// Estimated cost in USD based on token usage and model pricing
    pub estimated_cost_usd: f64,
    /// LLM provider used (e.g. "Anthropic", "Ollama")
    pub provider: Option<String>,
    /// LLM model used (e.g. "claude-sonnet-4-20250514")
    pub model: Option<String>,
    /// Number of behavioral specs mined from C source (P37)
    #[serde(default)]
    pub spec_count: usize,
    /// Number of specs that passed validation (P37)
    #[serde(default)]
    pub specs_passed: usize,
    /// Whether ensemble was used for this module (P38)
    #[serde(default)]
    pub ensemble_used: bool,
    /// Number of ensemble candidates generated (P38)
    #[serde(default)]
    pub ensemble_candidates: u32,
    /// Number of ensemble candidates that compiled (P38)
    #[serde(default)]
    pub ensemble_compiled: u32,
    /// Cost of the ensemble run in USD (P38)
    #[serde(default)]
    pub ensemble_cost_usd: f64,
}

impl MigrationMetrics {
    /// Estimate cost in USD from token counts.
    ///
    /// Uses Claude Sonnet 4 pricing as default: $3/MTok input, $15/MTok output.
    /// For Hard difficulty (Opus), costs are higher but this provides a baseline.
    pub fn compute_cost(&mut self) {
        const INPUT_COST_PER_MTOK: f64 = 3.0;
        const OUTPUT_COST_PER_MTOK: f64 = 15.0;
        self.estimated_cost_usd = (self.input_tokens as f64 / 1_000_000.0) * INPUT_COST_PER_MTOK
            + (self.output_tokens as f64 / 1_000_000.0) * OUTPUT_COST_PER_MTOK;
    }
}

impl FunctionUnit {
    /// Create a new function unit in `Pending` state with default metrics.
    pub fn new(name: String, source_path: String, c_source: String) -> Self {
        Self {
            name,
            source_path,
            c_source,
            preprocessed_source: None,
            c2rust_output: None,
            rust_output: None,
            state: MigrationState::Pending,
            difficulty: None,
            dependencies: Vec::new(),
            last_errors: Vec::new(),
            last_diff_feedback: Vec::new(),
            idiomatic_score: None,
            unsafe_count: None,
            generated_tests: None,
            metrics: MigrationMetrics::default(),
        }
    }
}

/// A migration project: collection of function units being migrated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationProject {
    /// Project name
    pub name: String,
    /// Root source directory
    pub source_dir: String,
    /// All function units
    pub units: Vec<FunctionUnit>,
}

impl MigrationProject {
    /// Create a new empty migration project.
    pub fn new(name: String, source_dir: String) -> Self {
        Self {
            name,
            source_dir,
            units: Vec::new(),
        }
    }

    /// Add a function unit to the project.
    pub fn add_unit(&mut self, unit: FunctionUnit) {
        self.units.push(unit);
    }

    /// Get units that are ready for the next pipeline stage.
    pub fn pending_units(&self) -> Vec<&FunctionUnit> {
        self.units
            .iter()
            .filter(|u| u.state == MigrationState::Pending)
            .collect()
    }

    /// Get a summary of migration progress.
    pub fn progress_summary(&self) -> ProgressSummary {
        let total = self.units.len();
        let validated = self
            .units
            .iter()
            .filter(|u| u.state == MigrationState::Validated)
            .count();
        let failed = self
            .units
            .iter()
            .filter(|u| {
                matches!(
                    u.state,
                    MigrationState::FallbackUnsafe
                        | MigrationState::CompilesUnsafe
                        | MigrationState::CompilesLowScore
                        | MigrationState::NearlyCompiles
                )
            })
            .count();
        let in_progress = total - validated - failed;

        ProgressSummary {
            total,
            validated,
            failed,
            in_progress,
        }
    }
}

/// Summary of migration progress across all function units in a project.
#[derive(Debug, Clone)]
pub struct ProgressSummary {
    /// Total number of function units.
    pub total: usize,
    /// Number of units that passed validation.
    pub validated: usize,
    /// Number of units that fell back to unsafe.
    pub failed: usize,
    /// Number of units still being processed.
    pub in_progress: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_unit_creation() {
        let unit = FunctionUnit::new(
            "add".to_string(),
            "tests/fixtures/simple/add.c".to_string(),
            "int add(int a, int b) { return a + b; }".to_string(),
        );
        assert_eq!(unit.state, MigrationState::Pending);
        assert_eq!(unit.name, "add");
        assert!(unit.c2rust_output.is_none());
    }

    #[test]
    fn test_migration_project_progress() {
        let mut project = MigrationProject::new("test".to_string(), ".".to_string());
        project.add_unit(FunctionUnit::new(
            "a".to_string(),
            "a.c".to_string(),
            "".to_string(),
        ));
        project.add_unit(FunctionUnit::new(
            "b".to_string(),
            "b.c".to_string(),
            "".to_string(),
        ));

        let summary = project.progress_summary();
        assert_eq!(summary.total, 2);
        assert_eq!(summary.in_progress, 2);
        assert_eq!(summary.validated, 0);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let unit = FunctionUnit::new(
            "add".to_string(),
            "add.c".to_string(),
            "int add(int a, int b) { return a + b; }".to_string(),
        );
        let json = serde_json::to_string(&unit).unwrap();
        let deserialized: FunctionUnit = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "add");
        assert_eq!(deserialized.state, MigrationState::Pending);
    }

    #[test]
    fn test_semantic_hint_prompt_line() {
        let hint = SemanticHint::DataStructure {
            kind: "linked_list".to_string(),
            involved: vec!["Node".to_string(), "insert_node".to_string()],
            rust_type: "Vec<T>".to_string(),
        };
        let line = hint.to_prompt_line();
        assert!(line.contains("linked_list"), "should mention kind");
        assert!(line.contains("Vec<T>"), "should mention rust_type");
        assert!(line.contains("Node"), "should mention involved items");
    }

    #[test]
    fn test_semantic_hint_serialization_roundtrip() {
        let hint = SemanticHint::MemoryManagement {
            functions: vec!["create_buffer".to_string(), "destroy_buffer".to_string()],
            suggestion: "use Vec<u8> with RAII".to_string(),
        };
        let json = serde_json::to_string(&hint).unwrap();
        let deserialized: SemanticHint = serde_json::from_str(&json).unwrap();
        assert_eq!(hint, deserialized);
    }
}
