/// Model router: selects the appropriate LLM model based on function characteristics.
///
/// Difficulty classification is based on heuristics about the C source code.
/// This will be refined with tree-sitter analysis in Phase 1.
use noricum_ir::Difficulty;

/// Classify the difficulty of a C function based on source code heuristics.
pub fn classify_difficulty(c_source: &str) -> Difficulty {
    let has_pointers = c_source.contains('*') && !c_source.contains("/*");
    let has_malloc = c_source.contains("malloc") || c_source.contains("calloc");
    let has_void_ptr = c_source.contains("void *") || c_source.contains("void*");
    let has_cast = c_source.contains(")(");
    let has_goto = c_source.contains("goto ");
    let has_union = c_source.contains("union ");
    let has_callback = c_source.contains("(*") && c_source.contains(")(");
    let line_count = c_source.lines().count();

    let hard_signals = [has_void_ptr, has_goto, has_union, has_callback]
        .iter()
        .filter(|&&x| x)
        .count();
    let medium_signals = [has_pointers, has_malloc, has_cast]
        .iter()
        .filter(|&&x| x)
        .count();

    if hard_signals >= 1 || (medium_signals >= 2 && line_count > 50) {
        Difficulty::Hard
    } else if medium_signals >= 1 || line_count > 30 {
        Difficulty::Medium
    } else {
        Difficulty::Easy
    }
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
