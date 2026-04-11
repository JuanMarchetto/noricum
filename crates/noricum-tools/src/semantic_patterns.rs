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

    debug!(
        hint_count = hints.len(),
        "semantic pattern detection complete"
    );
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
    let fn_re =
        Regex::new(r"(?m)^\w[\w\s\*]*\s+(\w+)\s*\([^)]*\)\s*\{").expect("static regex is valid");

    let mut alloc_fns = Vec::new();
    let mut free_fns = Vec::new();

    let lines: Vec<&str> = c_source.lines().collect();
    let mut current_fn: Option<String> = None;
    let mut brace_depth: i32 = 0;

    for line in &lines {
        let trimmed = line.trim();

        // Track function boundaries
        if brace_depth == 0
            && let Some(caps) = fn_re.captures(trimmed)
        {
            current_fn = caps.get(1).map(|m| m.as_str().to_string());
        }

        let opens = trimmed.chars().filter(|&c| c == '{').count() as i32;
        let closes = trimmed.chars().filter(|&c| c == '}').count() as i32;
        brace_depth += opens - closes;
        if brace_depth < 0 {
            brace_depth = 0;
        }

        if let Some(ref fn_name) = current_fn {
            if (trimmed.contains("malloc(")
                || trimmed.contains("calloc(")
                || trimmed.contains("realloc("))
                && !alloc_fns.contains(fn_name)
            {
                alloc_fns.push(fn_name.clone());
            }
            if trimmed.contains("free(") && !free_fns.contains(fn_name) {
                free_fns.push(fn_name.clone());
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
    let has_buckets = c_source.contains("**buckets") || c_source.contains("** buckets");
    let has_hash_fn = Regex::new(r"(?i)\b(hash|djb2|fnv|murmur)\b")
        .expect("static regex")
        .is_match(c_source);
    let has_capacity = source_lower.contains("capacity") && source_lower.contains("size");

    if (has_buckets || (has_hash_fn && has_capacity))
        && (c_source.contains("hash") || c_source.contains("Hash"))
    {
        let mut involved = Vec::new();
        let struct_re =
            Regex::new(r"(?:typedef\s+)?struct\s+(\w*[Hh]ash\w*)").expect("static regex");
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
        let struct_re = Regex::new(
            r"(?s)(?:typedef\s+)?struct\s+(\w+)\s*\{[^}]*\*\s*left\s*;[^}]*\*\s*right\s*;[^}]*\}",
        )
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
    let has_push = Regex::new(r"\bpush\s*\(")
        .expect("static regex")
        .is_match(c_source);
    let has_pop = Regex::new(r"\bpop\s*\(")
        .expect("static regex")
        .is_match(c_source);
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

/// Detect algorithmic patterns: sort, search, compression, checksum, crypto.
fn detect_algorithms(c_source: &str, hints: &mut Vec<SemanticHint>) {
    let source_lower = c_source.to_lowercase();

    // --- Sort detection ---
    // Heuristic: qsort() call, or swap + nested loops (manual sort)
    let has_qsort = c_source.contains("qsort(");
    // Manual sort: swap + nested loops
    let has_swap = source_lower.contains("swap") || c_source.contains("temp =");
    let has_nested_loop = Regex::new(r"for\s*\([^)]*\)\s*\{[^}]*for\s*\(")
        .expect("static regex")
        .is_match(c_source);

    if has_qsort {
        let mut functions = vec!["qsort".to_string()];
        // Find the comparison function name
        let cmp_re = Regex::new(r"int\s+(\w+)\s*\(\s*const\s+void").expect("static regex");
        for cap in cmp_re.captures_iter(c_source) {
            if let Some(name) = cap.get(1) {
                functions.push(name.as_str().to_string());
            }
        }
        hints.push(SemanticHint::Algorithm {
            kind: "sort".to_string(),
            functions,
            suggestion: "slice::sort_unstable_by() with typed comparison closure".to_string(),
        });
    } else if has_swap && has_nested_loop {
        hints.push(SemanticHint::Algorithm {
            kind: "sort".to_string(),
            functions: vec!["(manual sort detected)".to_string()],
            suggestion: "slice::sort() or sort_unstable_by() — avoid manual swap loops".to_string(),
        });
    }

    // --- Binary search detection ---
    // Heuristic: low/high/mid variables with halving logic
    let has_low_high = (source_lower.contains("low") || source_lower.contains("left"))
        && (source_lower.contains("high") || source_lower.contains("right"));
    let has_mid = Regex::new(r"\bmid\b\s*=.*[/+].*2")
        .expect("static regex")
        .is_match(c_source)
        || c_source.contains(">> 1");
    if has_low_high && has_mid {
        let fn_re = Regex::new(r"(?i)\b(\w*search\w*)\s*\(").expect("static regex");
        let functions: Vec<String> = fn_re
            .captures_iter(c_source)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();
        hints.push(SemanticHint::Algorithm {
            kind: "binary_search".to_string(),
            functions: if functions.is_empty() {
                vec!["(binary search pattern)".to_string()]
            } else {
                functions
            },
            suggestion: "slice::binary_search() or partition_point()".to_string(),
        });
    }

    // --- Compression detection ---
    // Heuristic: deflate/inflate, compress/decompress, zlib, lz77, huffman
    let compression_keywords = [
        "deflate",
        "inflate",
        "compress",
        "decompress",
        "lz77",
        "huffman",
        "zlib",
    ];
    let compression_fns: Vec<String> = compression_keywords
        .iter()
        .filter(|kw| source_lower.contains(*kw))
        .map(|kw| kw.to_string())
        .collect();
    if !compression_fns.is_empty() {
        hints.push(SemanticHint::Algorithm {
            kind: "compression".to_string(),
            functions: compression_fns,
            suggestion: "use flate2 crate or reimplement with byte slices and iterators"
                .to_string(),
        });
    }

    // --- Checksum/CRC detection ---
    // Heuristic: crc32, adler32, checksum, XOR accumulation with table lookup
    let checksum_keywords = ["crc32", "crc16", "adler32", "checksum"];
    let checksum_fns: Vec<String> = checksum_keywords
        .iter()
        .filter(|kw| source_lower.contains(*kw))
        .map(|kw| kw.to_string())
        .collect();
    if !checksum_fns.is_empty() {
        hints.push(SemanticHint::Algorithm {
            kind: "checksum".to_string(),
            functions: checksum_fns,
            suggestion: "wrapping arithmetic (wrapping_add, wrapping_shl) for bit-exact results"
                .to_string(),
        });
    }

    // --- Crypto detection ---
    let crypto_keywords = [
        "sha256", "sha1", "md5", "aes", "encrypt", "decrypt", "hmac", "pbkdf",
    ];
    let crypto_fns: Vec<String> = crypto_keywords
        .iter()
        .filter(|kw| source_lower.contains(*kw))
        .map(|kw| kw.to_string())
        .collect();
    if !crypto_fns.is_empty() {
        hints.push(SemanticHint::Algorithm {
            kind: "crypto".to_string(),
            functions: crypto_fns,
            suggestion: "use ring or sha2 crate for standard algorithms, or reimplement with wrapping ops for custom".to_string(),
        });
    }
}

/// Detect control flow patterns: state machines, recursive descent parsers, event loops, goto cleanup.
fn detect_control_flow(c_source: &str, hints: &mut Vec<SemanticHint>) {
    let source_lower = c_source.to_lowercase();

    // --- State machine detection ---
    // Heuristic: enum with STATE_* constants + switch on state variable + state transitions
    let has_state_enum = Regex::new(r"(?i)\bSTATE_\w+")
        .expect("static regex")
        .is_match(c_source)
        || Regex::new(r"enum\s+\w*[Ss]tate\w*")
            .expect("static regex")
            .is_match(c_source);
    let has_switch_state = Regex::new(r"switch\s*\(\s*\w*->?\s*state")
        .expect("static regex")
        .is_match(c_source);
    let has_state_assignment = Regex::new(r"\w*->?\s*state\s*=\s*STATE_")
        .expect("static regex")
        .is_match(c_source)
        || Regex::new(r"\w*->?\s*state\s*=\s*\w+_STATE")
            .expect("static regex")
            .is_match(c_source);

    if has_state_enum && (has_switch_state || has_state_assignment) {
        // Count states
        let state_re = Regex::new(r"STATE_(\w+)").expect("static regex");
        let states: std::collections::HashSet<String> = state_re
            .captures_iter(c_source)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();

        hints.push(SemanticHint::ControlFlow {
            kind: "state_machine".to_string(),
            functions: vec![format!("{} states detected", states.len())],
            suggestion: "enum State + match expression — one branch per state, no goto".to_string(),
        });
    }

    // --- Recursive descent parser detection ---
    // Heuristic: functions named parse_expression/parse_term/parse_factor or
    // parse_* calling other parse_* functions
    let parse_fn_re = Regex::new(r"\b(parse_\w+)\s*\(").expect("static regex");
    let parse_fns: std::collections::HashSet<String> = parse_fn_re
        .captures_iter(c_source)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();

    if parse_fns.len() >= 3 {
        // Check for mutual recursion: parse_expression calls parse_term, etc.
        let parser_keywords = [
            "expression",
            "term",
            "factor",
            "primary",
            "unary",
            "atom",
            "statement",
        ];
        let parser_fn_count = parse_fns
            .iter()
            .filter(|f| parser_keywords.iter().any(|kw| f.contains(kw)))
            .count();
        if parser_fn_count >= 2 {
            hints.push(SemanticHint::ControlFlow {
                kind: "recursive_descent_parser".to_string(),
                functions: parse_fns.into_iter().collect(),
                suggestion:
                    "Rust enum for AST nodes + recursive functions returning Result<Expr, ParseError>"
                        .to_string(),
            });
        }
    }

    // --- Event loop detection ---
    // Heuristic: select/poll/epoll/kqueue calls or while(1) with event dispatch
    let event_keywords = [
        "select(",
        "poll(",
        "epoll",
        "kqueue",
        "event_loop",
        "dispatch",
    ];
    let has_event_loop = event_keywords.iter().any(|kw| source_lower.contains(kw));
    if has_event_loop {
        hints.push(SemanticHint::ControlFlow {
            kind: "event_loop".to_string(),
            functions: vec!["(event dispatch)".to_string()],
            suggestion: "tokio/async or mio for event-driven I/O".to_string(),
        });
    }

    // --- Goto cleanup detection ---
    // Heuristic: goto + label with cleanup code (free, close)
    let has_goto = Regex::new(r"\bgoto\s+\w+")
        .expect("static regex")
        .is_match(c_source);
    let has_label_cleanup = Regex::new(r"(?m)^\w+:\s*$")
        .expect("static regex")
        .is_match(c_source);
    if has_goto && has_label_cleanup {
        hints.push(SemanticHint::ControlFlow {
            kind: "goto_cleanup".to_string(),
            functions: vec!["(goto-based resource cleanup)".to_string()],
            suggestion: "Drop trait for RAII cleanup + ? operator for early returns".to_string(),
        });
    }
}

/// Detect I/O patterns: file read/write, buffer management, serialization.
fn detect_io_patterns(c_source: &str, hints: &mut Vec<SemanticHint>) {
    let source_lower = c_source.to_lowercase();

    // --- File I/O detection ---
    let file_fns = [
        "fopen", "fclose", "fread", "fwrite", "fprintf", "fscanf", "fgets", "fputs",
    ];
    let detected_fns: Vec<String> = file_fns
        .iter()
        .filter(|f| c_source.contains(&format!("{f}(")))
        .map(|f| f.to_string())
        .collect();

    if detected_fns.len() >= 2 {
        hints.push(SemanticHint::IoPattern {
            kind: "file_readwrite".to_string(),
            functions: detected_fns,
            suggestion:
                "std::fs::read_to_string / std::io::BufReader / BufWriter with Read/Write traits"
                    .to_string(),
        });
    }

    // --- Buffer management ---
    let has_buffer = source_lower.contains("buffer") || source_lower.contains("buf_size");
    let has_read_write = (source_lower.contains("read(") || source_lower.contains("recv("))
        && (source_lower.contains("write(") || source_lower.contains("send("));
    if has_buffer && has_read_write {
        hints.push(SemanticHint::IoPattern {
            kind: "buffer_management".to_string(),
            functions: vec!["(buffered I/O)".to_string()],
            suggestion: "Vec<u8> as growable buffer + std::io::Cursor for in-memory I/O"
                .to_string(),
        });
    }

    // --- Serialization ---
    let has_serialize = source_lower.contains("serialize")
        || source_lower.contains("deserialize")
        || (source_lower.contains("json")
            && (source_lower.contains("parse") || source_lower.contains("print")));
    if has_serialize {
        hints.push(SemanticHint::IoPattern {
            kind: "serialization".to_string(),
            functions: vec!["(serialization/deserialization)".to_string()],
            suggestion: "serde with Serialize/Deserialize derives, or manual Display/FromStr"
                .to_string(),
        });
    }
}

/// Detect concurrency patterns: mutex/lock, thread creation, atomics.
fn detect_concurrency(c_source: &str, hints: &mut Vec<SemanticHint>) {
    let source_lower = c_source.to_lowercase();

    // --- Mutex/lock detection ---
    let mutex_fns = [
        "pthread_mutex_lock",
        "pthread_mutex_unlock",
        "pthread_mutex_init",
        "EnterCriticalSection",
        "LeaveCriticalSection",
    ];
    let detected_mutex: Vec<String> = mutex_fns
        .iter()
        .filter(|f| c_source.contains(*f))
        .map(|f| f.to_string())
        .collect();

    if !detected_mutex.is_empty() {
        hints.push(SemanticHint::Concurrency {
            kind: "mutex".to_string(),
            functions: detected_mutex,
            suggestion: "std::sync::Mutex<T> with RAII MutexGuard — lock scope = guard lifetime"
                .to_string(),
        });
    }

    // --- Thread creation detection ---
    let has_pthread_create = c_source.contains("pthread_create");
    let has_pthread_join = c_source.contains("pthread_join");
    let has_createthread = c_source.contains("CreateThread");
    if has_pthread_create || has_createthread {
        let mut fns = Vec::new();
        if has_pthread_create {
            fns.push("pthread_create".to_string());
        }
        if has_pthread_join {
            fns.push("pthread_join".to_string());
        }
        if has_createthread {
            fns.push("CreateThread".to_string());
        }
        hints.push(SemanticHint::Concurrency {
            kind: "thread_creation".to_string(),
            functions: fns,
            suggestion: "std::thread::spawn with JoinHandle, or tokio::spawn for async tasks"
                .to_string(),
        });
    }

    // --- Atomic operations detection ---
    let atomic_keywords = [
        "atomic_load",
        "atomic_store",
        "atomic_fetch_add",
        "__atomic",
        "__sync_fetch",
        "InterlockedIncrement",
    ];
    let detected_atomics: Vec<String> = atomic_keywords
        .iter()
        .filter(|kw| source_lower.contains(&kw.to_lowercase()))
        .map(|kw| kw.to_string())
        .collect();

    if !detected_atomics.is_empty() {
        hints.push(SemanticHint::Concurrency {
            kind: "atomic".to_string(),
            functions: detected_atomics,
            suggestion: "std::sync::atomic::{AtomicUsize, AtomicBool, etc.} with Ordering"
                .to_string(),
        });
    }
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
        assert!(
            hints.is_empty(),
            "simple arithmetic should produce no hints"
        );
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
        assert!(
            !mem_hints.is_empty(),
            "should detect realloc as memory management"
        );
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
            .filter(
                |h| matches!(h, SemanticHint::DataStructure { kind, .. } if kind == "linked_list"),
            )
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
            .filter(
                |h| matches!(h, SemanticHint::DataStructure { kind, .. } if kind == "hash_table"),
            )
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

    #[test]
    fn test_detect_sort_qsort() {
        let c_source = r#"
int compare(const void *a, const void *b) {
    return (*(int *)a - *(int *)b);
}

void sort_array(int *arr, int n) {
    qsort(arr, n, sizeof(int), compare);
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let alg_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(h, SemanticHint::Algorithm { kind, .. } if kind == "sort"))
            .collect();
        assert!(!alg_hints.is_empty(), "should detect qsort pattern");
    }

    #[test]
    fn test_detect_binary_search() {
        let c_source = r#"
int binary_search(int *arr, int n, int target) {
    int low = 0, high = n - 1;
    while (low <= high) {
        int mid = (low + high) / 2;
        if (arr[mid] == target) return mid;
        if (arr[mid] < target) low = mid + 1;
        else high = mid - 1;
    }
    return -1;
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let alg_hints: Vec<_> = hints
            .iter()
            .filter(
                |h| matches!(h, SemanticHint::Algorithm { kind, .. } if kind == "binary_search"),
            )
            .collect();
        assert!(!alg_hints.is_empty(), "should detect binary search pattern");
    }

    #[test]
    fn test_detect_checksum() {
        let c_source = r#"
unsigned long crc32(unsigned long crc, const unsigned char *buf, size_t len) {
    crc = crc ^ 0xFFFFFFFF;
    for (size_t i = 0; i < len; i++) {
        crc = crc32_table[(crc ^ buf[i]) & 0xFF] ^ (crc >> 8);
    }
    return crc ^ 0xFFFFFFFF;
}

unsigned long adler32(unsigned long adler, const unsigned char *buf, size_t len) {
    unsigned long s1 = adler & 0xFFFF;
    unsigned long s2 = (adler >> 16) & 0xFFFF;
    for (size_t i = 0; i < len; i++) {
        s1 = (s1 + buf[i]) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    return (s2 << 16) + s1;
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let alg_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(h, SemanticHint::Algorithm { kind, .. } if kind == "checksum"))
            .collect();
        assert!(!alg_hints.is_empty(), "should detect checksum/CRC pattern");
    }

    #[test]
    fn test_detect_compression() {
        let c_source = r#"
int deflate(z_stream *strm, int flush) {
    /* compression algorithm */
}
int inflate(z_stream *strm, int flush) {
    /* decompression */
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let alg_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(h, SemanticHint::Algorithm { kind, .. } if kind == "compression"))
            .collect();
        assert!(!alg_hints.is_empty(), "should detect compression pattern");
    }

    #[test]
    fn test_detect_state_machine() {
        let c_source = r#"
typedef enum { STATE_IDLE, STATE_HEADER, STATE_BODY, STATE_DONE } State;

void parse(Parser *p, const char *data, size_t len) {
    for (size_t i = 0; i < len; i++) {
        switch (p->state) {
            case STATE_IDLE:
                if (data[i] == '\n') p->state = STATE_HEADER;
                break;
            case STATE_HEADER:
                if (data[i] == '\r') p->state = STATE_BODY;
                break;
            case STATE_BODY:
                p->state = STATE_DONE;
                break;
        }
    }
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let cf_hints: Vec<_> = hints
            .iter()
            .filter(
                |h| matches!(h, SemanticHint::ControlFlow { kind, .. } if kind == "state_machine"),
            )
            .collect();
        assert!(!cf_hints.is_empty(), "should detect state machine pattern");
    }

    #[test]
    fn test_detect_recursive_descent_parser() {
        let c_source = r#"
double parse_expression(Parser *p);
double parse_term(Parser *p);
double parse_factor(Parser *p);

double parse_expression(Parser *p) {
    double left = parse_term(p);
    while (p->current == '+' || p->current == '-') {
        char op = p->current;
        advance(p);
        double right = parse_term(p);
        if (op == '+') left += right;
        else left -= right;
    }
    return left;
}

double parse_term(Parser *p) {
    double left = parse_factor(p);
    while (p->current == '*' || p->current == '/') {
        char op = p->current;
        advance(p);
        double right = parse_factor(p);
        if (op == '*') left *= right;
        else left /= right;
    }
    return left;
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let cf_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(h, SemanticHint::ControlFlow { kind, .. } if kind == "recursive_descent_parser"))
            .collect();
        assert!(
            !cf_hints.is_empty(),
            "should detect recursive descent parser"
        );
    }

    #[test]
    fn test_detect_goto_cleanup() {
        let c_source = r#"
int process(const char *path) {
    FILE *f = fopen(path, "r");
    if (!f) goto error;
    char *buf = malloc(1024);
    if (!buf) goto cleanup_file;
    /* work */
    free(buf);
    fclose(f);
    return 0;

cleanup_file:
    fclose(f);
error:
    return -1;
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let cf_hints: Vec<_> = hints
            .iter()
            .filter(
                |h| matches!(h, SemanticHint::ControlFlow { kind, .. } if kind == "goto_cleanup"),
            )
            .collect();
        assert!(!cf_hints.is_empty(), "should detect goto cleanup pattern");
    }

    #[test]
    fn test_detect_file_io() {
        let c_source = r#"
int read_file(const char *path, char *buf, size_t max) {
    FILE *f = fopen(path, "r");
    if (!f) return -1;
    size_t n = fread(buf, 1, max, f);
    fclose(f);
    return (int)n;
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let io_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(h, SemanticHint::IoPattern { .. }))
            .collect();
        assert!(!io_hints.is_empty(), "should detect file I/O pattern");
    }

    #[test]
    fn test_detect_mutex_concurrency() {
        let c_source = r#"
pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;

void safe_increment(int *counter) {
    pthread_mutex_lock(&lock);
    (*counter)++;
    pthread_mutex_unlock(&lock);
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let conc_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(h, SemanticHint::Concurrency { .. }))
            .collect();
        assert!(!conc_hints.is_empty(), "should detect mutex concurrency");
    }

    #[test]
    fn test_detect_thread_creation() {
        let c_source = r#"
void *worker(void *arg) {
    int id = *(int *)arg;
    printf("Thread %d\n", id);
    return NULL;
}

int main() {
    pthread_t threads[4];
    for (int i = 0; i < 4; i++) {
        pthread_create(&threads[i], NULL, worker, &i);
    }
    for (int i = 0; i < 4; i++) {
        pthread_join(threads[i], NULL);
    }
}
"#;
        let hints = detect_semantic_patterns(c_source);
        let conc_hints: Vec<_> = hints
            .iter()
            .filter(|h| matches!(h, SemanticHint::Concurrency { kind, .. } if kind == "thread_creation"))
            .collect();
        assert!(!conc_hints.is_empty(), "should detect thread creation");
    }

    #[test]
    fn test_hash_table_fixture_detection() {
        // Use a representative excerpt of hash_table.c
        let hash_table_c = r#"
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

HashTable *ht_create(int capacity) {
    HashTable *ht = (HashTable *)malloc(sizeof(HashTable));
    ht->buckets = (Entry **)calloc(capacity, sizeof(Entry *));
    ht->capacity = capacity;
    ht->size = 0;
    return ht;
}

void ht_destroy(HashTable *ht) {
    for (int i = 0; i < ht->capacity; i++) {
        Entry *entry = ht->buckets[i];
        while (entry) {
            Entry *next = entry->next;
            free(entry->key);
            free(entry);
            entry = next;
        }
    }
    free(ht->buckets);
    free(ht);
}
"#;
        let hints = detect_semantic_patterns(hash_table_c);

        // Should detect hash table
        assert!(
            hints.iter().any(
                |h| matches!(h, SemanticHint::DataStructure { kind, .. } if kind == "hash_table")
            ),
            "should detect hash table in hash_table.c fixture"
        );
        // Should detect linked list (collision chaining)
        assert!(
            hints.iter().any(
                |h| matches!(h, SemanticHint::DataStructure { kind, .. } if kind == "linked_list")
            ),
            "should detect linked list in hash_table.c (collision chains)"
        );
        // Should detect memory management
        assert!(
            hints
                .iter()
                .any(|h| matches!(h, SemanticHint::MemoryManagement { .. })),
            "should detect memory management (malloc/free)"
        );
    }

    #[test]
    fn test_expr_eval_fixture_detection() {
        // Representative excerpt of expr_eval.c
        let expr_eval_c = r#"
typedef enum {
    TOK_NUMBER, TOK_STRING, TOK_IDENT, TOK_PLUS, TOK_MINUS,
    TOK_STAR, TOK_SLASH, TOK_PERCENT, TOK_LPAREN, TOK_RPAREN
} TokenType;

double parse_expression(Parser *p);
double parse_term(Parser *p);
double parse_factor(Parser *p);
double parse_primary(Parser *p);

double parse_expression(Parser *p) {
    double left = parse_term(p);
    while (p->current.type == TOK_PLUS || p->current.type == TOK_MINUS) {
        TokenType op = p->current.type;
        advance(p);
        double right = parse_term(p);
        if (op == TOK_PLUS) left += right;
        else left -= right;
    }
    return left;
}

double parse_term(Parser *p) {
    double left = parse_factor(p);
    while (p->current.type == TOK_STAR || p->current.type == TOK_SLASH) {
        TokenType op = p->current.type;
        advance(p);
        double right = parse_factor(p);
        if (op == TOK_STAR) left *= right;
        else left /= right;
    }
    return left;
}
"#;
        let hints = detect_semantic_patterns(expr_eval_c);

        // Should detect recursive descent parser
        assert!(
            hints
                .iter()
                .any(|h| matches!(h, SemanticHint::ControlFlow { kind, .. } if kind == "recursive_descent_parser")),
            "should detect recursive descent parser in expr_eval.c fixture"
        );
    }

    #[test]
    fn test_format_all_hint_types() {
        // Ensure to_prompt_line works for all variants
        let hints = vec![
            SemanticHint::MemoryManagement {
                functions: vec!["alloc".to_string()],
                suggestion: "Box".to_string(),
            },
            SemanticHint::DataStructure {
                kind: "linked_list".to_string(),
                involved: vec!["Node".to_string()],
                rust_type: "Vec<T>".to_string(),
            },
            SemanticHint::Algorithm {
                kind: "sort".to_string(),
                functions: vec!["qsort".to_string()],
                suggestion: "sort_unstable".to_string(),
            },
            SemanticHint::ControlFlow {
                kind: "state_machine".to_string(),
                functions: vec!["parse".to_string()],
                suggestion: "enum + match".to_string(),
            },
            SemanticHint::IoPattern {
                kind: "file_readwrite".to_string(),
                functions: vec!["fopen".to_string()],
                suggestion: "std::fs".to_string(),
            },
            SemanticHint::Concurrency {
                kind: "mutex".to_string(),
                functions: vec!["lock".to_string()],
                suggestion: "Mutex<T>".to_string(),
            },
        ];

        for hint in &hints {
            let line = hint.to_prompt_line();
            assert!(!line.is_empty(), "prompt line should not be empty");
            assert!(line.starts_with("- "), "prompt line should start with '- '");
        }
    }

    #[test]
    fn test_real_hash_table_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/medium/hash_table.c");
        if !path.exists() {
            return; // Skip if fixture not available
        }
        let c_source = std::fs::read_to_string(&path).unwrap();
        let hints = detect_semantic_patterns(&c_source);

        // hash_table.c MUST detect: hash table, linked list, memory management
        let kinds: Vec<String> = hints
            .iter()
            .map(|h| match h {
                SemanticHint::MemoryManagement { .. } => "memory".to_string(),
                SemanticHint::DataStructure { kind, .. } => kind.clone(),
                SemanticHint::Algorithm { kind, .. } => kind.clone(),
                SemanticHint::ControlFlow { kind, .. } => kind.clone(),
                SemanticHint::IoPattern { kind, .. } => kind.clone(),
                SemanticHint::Concurrency { kind, .. } => kind.clone(),
            })
            .collect();

        assert!(
            kinds.contains(&"hash_table".to_string()),
            "hash_table.c: {kinds:?}"
        );
        assert!(
            kinds.contains(&"linked_list".to_string()),
            "hash_table.c: {kinds:?}"
        );
        assert!(
            kinds.contains(&"memory".to_string()),
            "hash_table.c: {kinds:?}"
        );
    }

    #[test]
    fn test_real_expr_eval_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/large/expr_eval.c");
        if !path.exists() {
            return; // Skip if fixture not available
        }
        let c_source = std::fs::read_to_string(&path).unwrap();
        let hints = detect_semantic_patterns(&c_source);

        // expr_eval.c MUST detect: recursive descent parser, memory management
        let kinds: Vec<String> = hints
            .iter()
            .map(|h| match h {
                SemanticHint::MemoryManagement { .. } => "memory".to_string(),
                SemanticHint::DataStructure { kind, .. } => kind.clone(),
                SemanticHint::Algorithm { kind, .. } => kind.clone(),
                SemanticHint::ControlFlow { kind, .. } => kind.clone(),
                SemanticHint::IoPattern { kind, .. } => kind.clone(),
                SemanticHint::Concurrency { kind, .. } => kind.clone(),
            })
            .collect();

        assert!(
            kinds.contains(&"recursive_descent_parser".to_string()),
            "expr_eval.c should detect parser: {kinds:?}"
        );
        assert!(
            kinds.contains(&"memory".to_string()),
            "expr_eval.c should detect memory: {kinds:?}"
        );
    }
}
