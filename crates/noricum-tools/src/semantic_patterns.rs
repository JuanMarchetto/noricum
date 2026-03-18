/// Semantic pattern detection for C source code.
///
/// Scans C source using regex-based heuristics to identify algorithmic patterns
/// and produce [`SemanticHint`] values that guide the translation agent.
use noricum_ir::SemanticHint;
use regex::Regex;
use tracing::debug;

/// Detect semantic patterns in C source code.
///
/// Returns a list of [`SemanticHint`] values describing algorithmic patterns,
/// data structures, memory management approaches, and control flow idioms
/// found in the source. These hints are injected into the translation prompt
/// as additional context.
///
/// This function is deterministic and has zero LLM cost.
pub fn detect_semantic_patterns(c_source: &str) -> Vec<SemanticHint> {
    let mut hints = Vec::new();

    detect_memory_management(c_source, &mut hints);
    detect_data_structures(c_source, &mut hints);
    detect_algorithms(c_source, &mut hints);
    detect_control_flow(c_source, &mut hints);
    detect_io_patterns(c_source, &mut hints);
    detect_concurrency(c_source, &mut hints);

    debug!(hint_count = hints.len(), "semantic pattern detection complete");
    hints
}

/// Detect malloc/free/calloc/realloc patterns and identify memory management functions.
fn detect_memory_management(c_source: &str, hints: &mut Vec<SemanticHint>) {
    let has_malloc = c_source.contains("malloc(");
    let has_calloc = c_source.contains("calloc(");
    let has_realloc = c_source.contains("realloc(");
    let has_free = c_source.contains("free(");

    if !has_malloc && !has_calloc && !has_realloc && !has_free {
        return;
    }

    // Extract function names that contain malloc/calloc/realloc
    let fn_re = Regex::new(r"(?m)^\w[\w\s\*]*\s+(\w+)\s*\([^)]*\)\s*\{")
        .expect("static regex is valid");

    let mut alloc_fns = Vec::new();
    let mut free_fns = Vec::new();

    let lines: Vec<&str> = c_source.lines().collect();
    let mut current_fn: Option<String> = None;
    let mut brace_depth: i32 = 0;

    for line in &lines {
        let trimmed = line.trim();

        // Track function boundaries
        if brace_depth == 0 {
            if let Some(caps) = fn_re.captures(trimmed) {
                current_fn = caps.get(1).map(|m| m.as_str().to_string());
            }
        }

        let opens = trimmed.chars().filter(|&c| c == '{').count() as i32;
        let closes = trimmed.chars().filter(|&c| c == '}').count() as i32;
        brace_depth += opens - closes;
        if brace_depth < 0 {
            brace_depth = 0;
        }

        if let Some(ref fn_name) = current_fn {
            if trimmed.contains("malloc(")
                || trimmed.contains("calloc(")
                || trimmed.contains("realloc(")
            {
                if !alloc_fns.contains(fn_name) {
                    alloc_fns.push(fn_name.clone());
                }
            }
            if trimmed.contains("free(") {
                if !free_fns.contains(fn_name) {
                    free_fns.push(fn_name.clone());
                }
            }
        }

        if brace_depth == 0 {
            current_fn = None;
        }
    }

    let mut functions = alloc_fns.clone();
    functions.extend(free_fns);
    functions.sort();
    functions.dedup();

    if !functions.is_empty() {
        let suggestion = if has_realloc {
            "Vec<T> with push/resize (realloc pattern detected)".to_string()
        } else if has_malloc && has_free {
            "RAII with Vec<T>, Box<T>, or String — malloc/free pairs detected".to_string()
        } else if has_calloc {
            "vec![0; n] for zero-initialized allocation".to_string()
        } else {
            "RAII ownership (Vec, Box, String)".to_string()
        };

        hints.push(SemanticHint::MemoryManagement {
            functions,
            suggestion,
        });
    }
}

fn detect_data_structures(_c_source: &str, _hints: &mut Vec<SemanticHint>) {
    // Implemented in Task 3
}

fn detect_algorithms(_c_source: &str, _hints: &mut Vec<SemanticHint>) {
    // Implemented in Task 4
}

fn detect_control_flow(_c_source: &str, _hints: &mut Vec<SemanticHint>) {
    // Implemented in Task 5
}

fn detect_io_patterns(_c_source: &str, _hints: &mut Vec<SemanticHint>) {
    // Implemented in Task 6a
}

fn detect_concurrency(_c_source: &str, _hints: &mut Vec<SemanticHint>) {
    // Implemented in Task 6b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_malloc_free_pair() {
        let c_source = r#"
Node *create_node(int val) {
    Node *n = (Node *)malloc(sizeof(Node));
    n->value = val;
    n->next = NULL;
    return n;
}

void destroy_node(Node *n) {
    free(n);
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let mem_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(h, SemanticHint::MemoryManagement { .. }))
            .collect();
        assert!(
            !mem_hints.is_empty(),
            "should detect memory management pattern"
        );
    }

    #[test]
    fn test_no_false_positive_on_simple_code() {
        let c_source = "int add(int a, int b) { return a + b; }";
        let hints = detect_semantic_patterns(c_source);
        assert!(hints.is_empty(), "simple arithmetic should produce no hints");
    }

    #[test]
    fn test_detect_realloc_pattern() {
        let c_source = r#"
void grow_buffer(Buffer *buf) {
    buf->capacity *= 2;
    buf->data = (char *)realloc(buf->data, buf->capacity);
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let mem_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(h, SemanticHint::MemoryManagement { .. }))
            .collect();
        assert!(!mem_hints.is_empty(), "should detect realloc as memory management");
    }
}
