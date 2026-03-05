/// Model router: selects the appropriate LLM model based on function characteristics.
///
/// Uses tree-sitter AST analysis for accurate difficulty classification,
/// with regex heuristics as fallback.
use noricum_ir::Difficulty;

/// Classify the difficulty of a C function based on AST analysis.
///
/// Delegates to tree-sitter based classification from `noricum_tools::ast`,
/// which internally falls back to regex if tree-sitter parsing fails.
pub fn classify_difficulty(c_source: &str) -> Difficulty {
    noricum_tools::ast::classify_difficulty_ast(c_source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_function_is_easy() {
        let source = "int add(int a, int b) { return a + b; }";
        assert_eq!(classify_difficulty(source), Difficulty::Easy);
    }

    #[test]
    fn test_pointer_function_is_medium() {
        let source = r#"
void copy(char *dst, const char *src) {
    while (*src) {
        *dst++ = *src++;
    }
    *dst = '\0';
}
"#;
        assert_eq!(classify_difficulty(source), Difficulty::Medium);
    }

    #[test]
    fn test_void_ptr_is_hard() {
        let source = r#"
void* generic_alloc(void *ctx, size_t size) {
    void *ptr = malloc(size);
    if (!ptr) return NULL;
    memset(ptr, 0, size);
    return ptr;
}
"#;
        assert_eq!(classify_difficulty(source), Difficulty::Hard);
    }

    #[test]
    fn test_callback_is_hard() {
        let source = r#"
typedef int (*compare_fn)(const void *, const void *);
void sort(void *base, size_t n, size_t size, compare_fn cmp) {
    /* ... */
}
"#;
        assert_eq!(classify_difficulty(source), Difficulty::Hard);
    }
}
