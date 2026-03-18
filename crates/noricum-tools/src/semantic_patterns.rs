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

/// Detect algorithmic patterns: sort, search, compression, checksum, crypto.
fn detect_algorithms(c_source: &str, hints: &mut Vec<SemanticHint>) {
    let source_lower = c_source.to_lowercase();

    // --- Sort detection ---
    // Heuristic: qsort() call, or comparison functions (const void *a, const void *b)
    let has_qsort = c_source.contains("qsort(");
    let has_comparison_fn =
        Regex::new(r"const\s+void\s*\*\s*\w+\s*,\s*const\s+void\s*\*\s*\w+")
            .expect("static regex")
            .is_match(c_source);
    // Manual sort: swap + nested loops
    let has_swap = source_lower.contains("swap") || c_source.contains("temp =");
    let has_nested_loop = Regex::new(r"for\s*\([^)]*\)\s*\{[^}]*for\s*\(")
        .expect("static regex")
        .is_match(c_source);

    if has_qsort || (has_comparison_fn && has_qsort) {
        let mut functions = vec!["qsort".to_string()];
        // Find the comparison function name
        let cmp_re =
            Regex::new(r"int\s+(\w+)\s*\(\s*const\s+void").expect("static regex");
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
            suggestion: "slice::sort() or sort_unstable_by() — avoid manual swap loops"
                .to_string(),
        });
    }

    // --- Binary search detection ---
    // Heuristic: low/high/mid variables with halving logic
    let has_low_high = (source_lower.contains("low") || source_lower.contains("left"))
        && (source_lower.contains("high") || source_lower.contains("right"));
    let has_mid =
        Regex::new(r"\bmid\b\s*=.*[/+].*2")
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
            suggestion:
                "wrapping arithmetic (wrapping_add, wrapping_shl) for bit-exact results"
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
            .filter(|h| matches!(h, SemanticHint::Algorithm { kind, .. } if kind == "binary_search"))
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
}
