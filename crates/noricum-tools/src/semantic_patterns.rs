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

/// Detect data structure patterns: linked lists, hash tables, trees, stacks, queues.
fn detect_data_structures(c_source: &str, hints: &mut Vec<SemanticHint>) {
    let source_lower = c_source.to_lowercase();

    // --- Linked list detection ---
    // Heuristic: struct with `*next` pointer + traversal with `curr = curr->next`
    let has_next_ptr = Regex::new(r"struct\s+\w+\s*\*\s*next\s*;")
        .expect("static regex")
        .is_match(c_source);
    let has_traversal = c_source.contains("->next");
    let has_prev_ptr = c_source.contains("*prev;") || c_source.contains("->prev");

    if has_next_ptr && has_traversal {
        // Extract struct names containing next pointers
        let struct_re =
            Regex::new(r"(?s)(?:typedef\s+)?struct\s+(\w+)\s*\{[^}]*\*\s*next\s*;[^}]*\}")
                .expect("static regex");
        let involved: Vec<String> = struct_re
            .captures_iter(c_source)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();
        let kind = if has_prev_ptr {
            "doubly_linked_list"
        } else {
            "linked_list"
        };
        hints.push(SemanticHint::DataStructure {
            kind: kind.to_string(),
            involved: if involved.is_empty() {
                vec!["(struct with ->next)".to_string()]
            } else {
                involved
            },
            rust_type: "Vec<T> (flatten the linked structure)".to_string(),
        });
    }

    // --- Hash table detection ---
    // Heuristic: struct with `**buckets` or name contains "hash", plus a hash function
    let has_buckets =
        c_source.contains("**buckets") || c_source.contains("** buckets");
    let has_hash_fn = Regex::new(r"(?i)\b(hash|djb2|fnv|murmur)\b")
        .expect("static regex")
        .is_match(c_source);
    let has_capacity = source_lower.contains("capacity") && source_lower.contains("size");

    if (has_buckets || (has_hash_fn && has_capacity))
        && (c_source.contains("hash") || c_source.contains("Hash"))
    {
        let mut involved = Vec::new();
        let struct_re = Regex::new(r"(?:typedef\s+)?struct\s+(\w*[Hh]ash\w*)")
            .expect("static regex");
        for cap in struct_re.captures_iter(c_source) {
            if let Some(name) = cap.get(1) {
                involved.push(name.as_str().to_string());
            }
        }
        if involved.is_empty() {
            involved.push("(hash table structs)".to_string());
        }
        hints.push(SemanticHint::DataStructure {
            kind: "hash_table".to_string(),
            involved,
            rust_type: "HashMap<K, V> from std::collections".to_string(),
        });
    }

    // --- Tree detection ---
    // Heuristic: struct with `*left` and `*right` pointers
    let has_left = c_source.contains("*left") || c_source.contains("-> left");
    let has_right = c_source.contains("*right") || c_source.contains("-> right");
    if has_left && has_right {
        let struct_re =
            Regex::new(r"(?s)(?:typedef\s+)?struct\s+(\w+)\s*\{[^}]*\*\s*left\s*;[^}]*\*\s*right\s*;[^}]*\}")
                .expect("static regex");
        let involved: Vec<String> = struct_re
            .captures_iter(c_source)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();
        hints.push(SemanticHint::DataStructure {
            kind: "tree".to_string(),
            involved: if involved.is_empty() {
                vec!["(struct with left/right)".to_string()]
            } else {
                involved
            },
            rust_type: "Box<TreeNode> with Option<Box<TreeNode>> children, or arena-based"
                .to_string(),
        });
    }

    // --- Stack detection ---
    // Heuristic: functions named push/pop with array + top/sp index
    let has_push = Regex::new(r"\bpush\s*\(").expect("static regex").is_match(c_source);
    let has_pop = Regex::new(r"\bpop\s*\(").expect("static regex").is_match(c_source);
    let has_top = source_lower.contains("top") || source_lower.contains("->sp");
    if has_push && has_pop && has_top {
        hints.push(SemanticHint::DataStructure {
            kind: "stack".to_string(),
            involved: vec!["push".to_string(), "pop".to_string()],
            rust_type: "Vec<T> with push()/pop()".to_string(),
        });
    }

    // --- Queue detection ---
    // Heuristic: enqueue/dequeue functions or head/tail with FIFO semantics
    let has_enqueue = source_lower.contains("enqueue") || source_lower.contains("queue_push");
    let has_dequeue = source_lower.contains("dequeue") || source_lower.contains("queue_pop");
    if has_enqueue && has_dequeue {
        hints.push(SemanticHint::DataStructure {
            kind: "queue".to_string(),
            involved: vec!["enqueue".to_string(), "dequeue".to_string()],
            rust_type: "VecDeque<T> from std::collections".to_string(),
        });
    }
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

    #[test]
    fn test_detect_linked_list() {
        let c_source = r#"
typedef struct Node {
    int value;
    struct Node *next;
} Node;

void traverse(Node *head) {
    Node *curr = head;
    while (curr != NULL) {
        printf("%d\n", curr->value);
        curr = curr->next;
    }
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let ds_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(h, SemanticHint::DataStructure { kind, .. } if kind == "linked_list"))
            .collect();
        assert!(!ds_hints.is_empty(), "should detect linked list");
    }

    #[test]
    fn test_detect_hash_table() {
        let c_source = r#"
typedef struct Entry {
    char *key;
    int value;
    struct Entry *next;
} Entry;

typedef struct {
    Entry **buckets;
    int capacity;
    int size;
} HashTable;

unsigned long hash_key(const char *str) {
    unsigned long hash = 5381;
    int c;
    while ((c = *str++))
        hash = ((hash << 5) + hash) + c;
    return hash;
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let ds_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(h, SemanticHint::DataStructure { kind, .. } if kind == "hash_table"))
            .collect();
        assert!(!ds_hints.is_empty(), "should detect hash table");
    }

    #[test]
    fn test_detect_tree() {
        let c_source = r#"
typedef struct TreeNode {
    int key;
    struct TreeNode *left;
    struct TreeNode *right;
} TreeNode;

TreeNode *insert(TreeNode *root, int key) {
    if (root == NULL) return create_node(key);
    if (key < root->key) root->left = insert(root->left, key);
    else root->right = insert(root->right, key);
    return root;
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let ds_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(h, SemanticHint::DataStructure { kind, .. } if kind == "tree"))
            .collect();
        assert!(!ds_hints.is_empty(), "should detect tree structure");
    }

    #[test]
    fn test_detect_stack_queue() {
        let c_source = r#"
void push(Stack *s, int val) { s->data[s->top++] = val; }
int pop(Stack *s) { return s->data[--s->top]; }
int peek(Stack *s) { return s->data[s->top - 1]; }
"#;
        let hints = detect_semantic_patterns(c_source);
        let ds_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(h, SemanticHint::DataStructure { kind, .. } if kind == "stack"))
            .collect();
        assert!(!ds_hints.is_empty(), "should detect stack pattern");
    }
}
