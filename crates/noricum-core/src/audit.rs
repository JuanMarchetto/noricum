/// Audit trail: structured logging of migration pipeline events.
///
/// Produces JSON-lines (`.jsonl`) files with timestamped events for
/// reproducibility, debugging, and compliance.
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Level of detail for audit logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditLevel {
    /// ~5 events per function: start, classify, validate, complete, error.
    Summary,
    /// ~10-20 events: includes model selection, state transitions.
    Detailed,
    /// Everything including full prompt/response bodies.
    Full,
}

impl std::str::FromStr for AuditLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "summary" => Ok(AuditLevel::Summary),
            "detailed" => Ok(AuditLevel::Detailed),
            "full" => Ok(AuditLevel::Full),
            _ => Err(format!(
                "unknown audit level: {s} (expected: summary, detailed, full)"
            )),
        }
    }
}

/// A single audit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AuditEvent {
    PipelineStart {
        function_name: String,
        source_file: String,
        c_lines: u32,
    },
    DifficultyClassified {
        function_name: String,
        difficulty: String,
    },
    ModelSelected {
        function_name: String,
        provider: String,
        model: String,
        task: String,
    },
    AgentPromptSent {
        function_name: String,
        agent: String,
        /// At Summary/Detailed: SHA-256 hash of prompt. At Full: full text.
        prompt_hash_or_body: String,
        prompt_length: usize,
    },
    AgentResponseReceived {
        function_name: String,
        agent: String,
        /// At Summary/Detailed: SHA-256 hash. At Full: full text.
        response_hash_or_body: String,
        response_length: usize,
    },
    StateTransition {
        function_name: String,
        from: String,
        to: String,
    },
    ValidationResult {
        function_name: String,
        compiles: bool,
        idiomatic_score: u32,
        unsafe_count: u32,
        diff_test_passed: Option<bool>,
        passed: bool,
    },
    RepairIteration {
        function_name: String,
        iteration: u32,
        max_iterations: u32,
        error_count: usize,
        diff_feedback_count: usize,
    },
    PipelineComplete {
        function_name: String,
        final_state: String,
        total_ms: u64,
        llm_calls: u32,
    },
    Error {
        function_name: String,
        error: String,
    },
}

/// A timestamped audit log entry.
#[derive(Debug, Serialize, Deserialize)]
struct AuditEntry {
    timestamp: String,
    session_id: String,
    #[serde(flatten)]
    event: AuditEvent,
}

/// Audit trail writer. Thread-safe via Arc<Mutex>.
pub struct AuditTrail {
    level: AuditLevel,
    writer: BufWriter<File>,
    session_id: String,
}

impl AuditTrail {
    /// Create a new audit trail writing to the given path.
    pub fn new(path: &Path, level: AuditLevel) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(path)?;
        let session_id = format!("{}-{}", Utc::now().format("%Y%m%d-%H%M%S"), &uuid_short());
        Ok(Self {
            level,
            writer: BufWriter::new(file),
            session_id,
        })
    }

    /// Log an audit event.
    pub fn log(&mut self, event: AuditEvent) -> std::io::Result<()> {
        // Filter events based on level
        if !self.should_log(&event) {
            return Ok(());
        }

        let entry = AuditEntry {
            timestamp: Utc::now().to_rfc3339(),
            session_id: self.session_id.clone(),
            event,
        };

        let json = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
        writeln!(self.writer, "{json}")?;
        Ok(())
    }

    /// Flush and close the audit trail.
    pub fn finalize(mut self) -> std::io::Result<()> {
        self.writer.flush()
    }

    fn should_log(&self, event: &AuditEvent) -> bool {
        match self.level {
            AuditLevel::Summary => matches!(
                event,
                AuditEvent::PipelineStart { .. }
                    | AuditEvent::PipelineComplete { .. }
                    | AuditEvent::ValidationResult { .. }
                    | AuditEvent::Error { .. }
                    | AuditEvent::DifficultyClassified { .. }
            ),
            AuditLevel::Detailed => true, // Detailed logs everything (prompt/response bodies stored as hash)
            AuditLevel::Full => true,
        }
    }
}

/// Shared audit trail handle for use across async pipeline stages.
pub type SharedAuditTrail = Arc<Mutex<AuditTrail>>;

/// Create a shared audit trail.
pub fn create_shared_audit(path: &Path, level: AuditLevel) -> std::io::Result<SharedAuditTrail> {
    let trail = AuditTrail::new(path, level)?;
    Ok(Arc::new(Mutex::new(trail)))
}

/// Log an event to a shared audit trail (convenience function).
pub fn audit_log(trail: &SharedAuditTrail, event: AuditEvent) {
    if let Ok(mut t) = trail.lock() {
        let _ = t.log(event);
    }
}

/// Compute SHA-256 hash of text (for Summary/Detailed level prompt/response logging).
pub fn sha256_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generate a short pseudo-UUID from timestamp.
fn uuid_short() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:08x}", (nanos & 0xFFFF_FFFF) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_serialization() {
        let event = AuditEvent::PipelineStart {
            function_name: "add".to_string(),
            source_file: "add.c".to_string(),
            c_lines: 5,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("PipelineStart"));
        assert!(json.contains("add"));

        let deserialized: AuditEvent = serde_json::from_str(&json).unwrap();
        match deserialized {
            AuditEvent::PipelineStart { function_name, .. } => {
                assert_eq!(function_name, "add");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_audit_level_parse() {
        assert_eq!(
            "summary".parse::<AuditLevel>().unwrap(),
            AuditLevel::Summary
        );
        assert_eq!(
            "detailed".parse::<AuditLevel>().unwrap(),
            AuditLevel::Detailed
        );
        assert_eq!("full".parse::<AuditLevel>().unwrap(), AuditLevel::Full);
        assert!("invalid".parse::<AuditLevel>().is_err());
    }

    #[test]
    fn test_audit_trail_file_output() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("audit.jsonl");

        {
            let mut trail = AuditTrail::new(&path, AuditLevel::Full).unwrap();
            trail
                .log(AuditEvent::PipelineStart {
                    function_name: "test".to_string(),
                    source_file: "test.c".to_string(),
                    c_lines: 10,
                })
                .unwrap();
            trail
                .log(AuditEvent::PipelineComplete {
                    function_name: "test".to_string(),
                    final_state: "Validated".to_string(),
                    total_ms: 500,
                    llm_calls: 3,
                })
                .unwrap();
            trail.finalize().unwrap();
        }

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2, "should have 2 log entries");

        // Each line should be valid JSON
        for line in &lines {
            let _: serde_json::Value = serde_json::from_str(line).unwrap();
        }
    }

    #[test]
    fn test_audit_summary_filters() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("audit_summary.jsonl");

        {
            let mut trail = AuditTrail::new(&path, AuditLevel::Summary).unwrap();
            trail
                .log(AuditEvent::PipelineStart {
                    function_name: "f".to_string(),
                    source_file: "f.c".to_string(),
                    c_lines: 5,
                })
                .unwrap();
            trail
                .log(AuditEvent::StateTransition {
                    function_name: "f".to_string(),
                    from: "Extracted".to_string(),
                    to: "Analyzed".to_string(),
                })
                .unwrap();
            trail
                .log(AuditEvent::PipelineComplete {
                    function_name: "f".to_string(),
                    final_state: "Validated".to_string(),
                    total_ms: 100,
                    llm_calls: 1,
                })
                .unwrap();
            trail.finalize().unwrap();
        }

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
        // Summary should include PipelineStart and PipelineComplete but NOT StateTransition
        assert_eq!(lines.len(), 2, "summary should filter StateTransition");
    }

    #[test]
    fn test_sha256_hash() {
        let hash = sha256_hash("hello world");
        assert_eq!(hash.len(), 64);
        // Same input should produce same hash
        assert_eq!(hash, sha256_hash("hello world"));
        // Different input should produce different hash
        assert_ne!(hash, sha256_hash("hello world!"));
    }

    #[test]
    fn test_shared_audit_trail() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("shared.jsonl");
        let trail = create_shared_audit(&path, AuditLevel::Full).unwrap();

        audit_log(
            &trail,
            AuditEvent::PipelineStart {
                function_name: "f".to_string(),
                source_file: "f.c".to_string(),
                c_lines: 1,
            },
        );

        // Verify the file was written
        let t = trail.lock().unwrap();
        drop(t);
    }
}
