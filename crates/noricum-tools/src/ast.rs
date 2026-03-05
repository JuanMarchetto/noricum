/// Tree-sitter based AST analysis for C and Rust source code.
///
/// Provides accurate function extraction, call detection, difficulty classification,
/// and unsafe counting using tree-sitter parsers instead of regex heuristics.
/// Each function has a regex fallback if tree-sitter parsing fails.
use noricum_ir::Difficulty;
use tracing::debug;

/// A C function extracted from source code.
#[derive(Debug, Clone)]
pub struct CFunction {
    pub name: String,
    pub return_type: String,
    pub body: String,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Parse C source with tree-sitter-c and extract function definitions.
pub fn extract_c_functions(c_source: &str) -> Vec<CFunction> {
    match extract_c_functions_ast(c_source) {
        Some(funcs) if !funcs.is_empty() => funcs,
        _ => {
            debug!("tree-sitter C parse failed or empty, using regex fallback");
            extract_c_functions_regex(c_source)
        }
    }
}

fn extract_c_functions_ast(c_source: &str) -> Option<Vec<CFunction>> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_c::LANGUAGE.into()).ok()?;
    let tree = parser.parse(c_source, None)?;
    let root = tree.root_node();

    let mut functions = Vec::new();
    walk_tree(root, c_source, &mut |node, src| {
        if node.kind() == "function_definition" {
            if let Some(func) = parse_function_def(node, src) {
                functions.push(func);
            }
        }
    });

    Some(functions)
}

fn parse_function_def(node: tree_sitter::Node, source: &str) -> Option<CFunction> {
    let mut declarator = None;
    let mut return_type = String::new();
    let mut body_text = String::new();

    for i in 0..node.child_count() {
        let child = node.child(i)?;
        match child.kind() {
            "function_declarator" | "pointer_declarator" => {
                declarator = Some(child);
            }
            "compound_statement" => {
                body_text = child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            }
            _ if declarator.is_none() => {
                // Everything before the declarator is the return type
                let part = child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                if !return_type.is_empty() {
                    return_type.push(' ');
                }
                return_type.push_str(&part);
            }
            _ => {}
        }
    }

    let decl = declarator?;
    let name = find_identifier(decl, source)?;

    Some(CFunction {
        name,
        return_type,
        body: body_text,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

fn find_identifier(node: tree_sitter::Node, source: &str) -> Option<String> {
    if node.kind() == "identifier" {
        return node
            .utf8_text(source.as_bytes())
            .ok()
            .map(|s| s.to_string());
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if let Some(id) = find_identifier(child, source) {
                return Some(id);
            }
        }
    }
    None
}

fn extract_c_functions_regex(c_source: &str) -> Vec<CFunction> {
    let re = regex::Regex::new(
        r"(?m)^\s*(?:static\s+)?(?:inline\s+)?(?:const\s+)?(?:unsigned\s+)?(?:signed\s+)?(?:long\s+)?(?:short\s+)?(?:struct\s+\w+\s*\*?\s*|enum\s+\w+\s+)?(?:void|int|char|float|double|size_t|ssize_t|uint\d+_t|int\d+_t|bool|_Bool|\w+_t)\s*\*?\s*\*?\s*(\w+)\s*\("
    ).expect("invalid regex");

    let keywords = [
        "if",
        "while",
        "for",
        "switch",
        "return",
        "sizeof",
        "typeof",
        "defined",
        "main",
        "__attribute__",
    ];

    let mut functions = Vec::new();
    for cap in re.captures_iter(c_source) {
        let name = cap[1].to_string();
        if !keywords.contains(&name.as_str()) && !name.starts_with('_') {
            functions.push(CFunction {
                name,
                return_type: String::new(),
                body: String::new(),
                start_byte: 0,
                end_byte: 0,
            });
        }
    }
    functions
}

/// Extract function call sites from C source, filtering to known project functions.
pub fn extract_c_calls(c_source: &str, known: &[String]) -> Vec<String> {
    match extract_c_calls_ast(c_source, known) {
        Some(calls) => calls,
        None => {
            debug!("tree-sitter C call extraction failed, using regex fallback");
            extract_c_calls_regex(c_source, known)
        }
    }
}

fn extract_c_calls_ast(c_source: &str, known: &[String]) -> Option<Vec<String>> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_c::LANGUAGE.into()).ok()?;
    let tree = parser.parse(c_source, None)?;
    let root = tree.root_node();

    let known_set: std::collections::HashSet<&str> = known.iter().map(|s| s.as_str()).collect();
    let mut calls = Vec::new();

    walk_tree(root, c_source, &mut |node, src| {
        if node.kind() == "call_expression" {
            if let Some(func_node) = node.child(0) {
                if let Ok(name) = func_node.utf8_text(src.as_bytes()) {
                    if known_set.contains(name) {
                        calls.push(name.to_string());
                    }
                }
            }
        }
    });

    calls.sort();
    calls.dedup();
    Some(calls)
}

fn extract_c_calls_regex(c_source: &str, known: &[String]) -> Vec<String> {
    let known_set: std::collections::HashSet<&str> = known.iter().map(|s| s.as_str()).collect();
    let call_re = regex::Regex::new(r"\b(\w+)\s*\(").expect("invalid regex");
    let keywords = [
        "if",
        "while",
        "for",
        "switch",
        "return",
        "sizeof",
        "typeof",
        "defined",
        "__attribute__",
    ];

    let mut calls = Vec::new();
    for cap in call_re.captures_iter(c_source) {
        let name = &cap[1];
        if known_set.contains(name) && !keywords.contains(&name) {
            calls.push(name.to_string());
        }
    }
    calls.sort();
    calls.dedup();
    calls
}

/// Classify C source difficulty using AST node analysis.
pub fn classify_difficulty_ast(c_source: &str) -> Difficulty {
    match classify_difficulty_ast_inner(c_source) {
        Some(d) => d,
        None => {
            debug!("tree-sitter difficulty classification failed, using regex fallback");
            classify_difficulty_regex(c_source)
        }
    }
}

fn classify_difficulty_ast_inner(c_source: &str) -> Option<Difficulty> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_c::LANGUAGE.into()).ok()?;
    let tree = parser.parse(c_source, None)?;
    let root = tree.root_node();

    let mut goto_count = 0u32;
    let mut union_count = 0u32;
    let mut pointer_decl_count = 0u32;
    let mut cast_count = 0u32;
    let mut void_ptr = false;
    let mut callback = false;
    let line_count = c_source.lines().count();

    walk_tree(root, c_source, &mut |node, src| {
        match node.kind() {
            "goto_statement" => goto_count += 1,
            "union_specifier" => union_count += 1,
            "pointer_declarator" => pointer_decl_count += 1,
            "cast_expression" => cast_count += 1,
            "type_descriptor" => {
                if let Ok(text) = node.utf8_text(src.as_bytes()) {
                    if text.contains("void") && text.contains('*') {
                        void_ptr = true;
                    }
                }
            }
            "function_declarator" => {
                // Check for function pointer parameters (callbacks)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "pointer_declarator" {
                        if let Some(grandparent) = parent.parent() {
                            if grandparent.kind() == "parameter_declaration" {
                                callback = true;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    });

    // Also check raw text for void* patterns the AST might miss
    if c_source.contains("void *") || c_source.contains("void*") {
        void_ptr = true;
    }

    let hard_signals = [goto_count > 0, union_count > 0, void_ptr, callback]
        .iter()
        .filter(|&&x| x)
        .count();
    let medium_signals = [pointer_decl_count > 0, cast_count > 0]
        .iter()
        .filter(|&&x| x)
        .count();

    if hard_signals >= 1 || (medium_signals >= 2 && line_count > 50) {
        Some(Difficulty::Hard)
    } else if medium_signals >= 1
        || pointer_decl_count > 2
        || line_count > 30
        || c_source.contains("malloc")
    {
        Some(Difficulty::Medium)
    } else {
        Some(Difficulty::Easy)
    }
}

fn classify_difficulty_regex(c_source: &str) -> Difficulty {
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

/// Count unsafe blocks in Rust source using tree-sitter-rust.
pub fn count_unsafe_blocks_ast(rust_source: &str) -> u32 {
    match count_unsafe_blocks_ts(rust_source) {
        Some(count) => count,
        None => {
            debug!("tree-sitter Rust parse failed, using regex fallback");
            count_unsafe_blocks_regex(rust_source)
        }
    }
}

fn count_unsafe_blocks_ts(rust_source: &str) -> Option<u32> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(rust_source, None)?;
    let root = tree.root_node();

    let mut count = 0u32;
    walk_tree(root, rust_source, &mut |node, src| {
        match node.kind() {
            "unsafe_block" => count += 1,
            "function_item" => {
                // Check if this function has unsafe modifier
                // tree-sitter-rust may use different node structure
                if let Ok(text) = node.utf8_text(src.as_bytes()) {
                    let trimmed = text.trim();
                    if trimmed.starts_with("unsafe ")
                        || trimmed.starts_with("pub unsafe ")
                        || trimmed.starts_with("pub(crate) unsafe ")
                    {
                        count += 1;
                    }
                }
            }
            _ => {}
        }
    });

    Some(count)
}

fn count_unsafe_blocks_regex(rust_source: &str) -> u32 {
    // Reuse the existing regex-based implementation
    crate::compiler::count_unsafe_blocks(rust_source)
}

/// Extract struct, typedef, and enum type definitions from C source.
pub fn extract_c_types(c_source: &str) -> Vec<String> {
    match extract_c_types_ast(c_source) {
        Some(types) if !types.is_empty() => types,
        _ => {
            debug!("tree-sitter type extraction failed, using regex fallback");
            extract_c_types_regex(c_source)
        }
    }
}

fn extract_c_types_ast(c_source: &str) -> Option<Vec<String>> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_c::LANGUAGE.into()).ok()?;
    let tree = parser.parse(c_source, None)?;
    let root = tree.root_node();

    let mut types = Vec::new();
    walk_tree(root, c_source, &mut |node, src| {
        match node.kind() {
            "struct_specifier" | "enum_specifier" | "union_specifier" => {
                // Find the tag name
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        if child.kind() == "type_identifier" || child.kind() == "identifier" {
                            if let Ok(name) = child.utf8_text(src.as_bytes()) {
                                types.push(name.to_string());
                            }
                        }
                    }
                }
            }
            "type_definition" => {
                // typedef: the last identifier is the alias name
                if let Some(last_child) = node.child(node.child_count().saturating_sub(2)) {
                    if last_child.kind() == "type_identifier" {
                        if let Ok(name) = last_child.utf8_text(src.as_bytes()) {
                            types.push(name.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    });

    types.sort();
    types.dedup();
    Some(types)
}

fn extract_c_types_regex(c_source: &str) -> Vec<String> {
    let re = regex::Regex::new(r"(?m)(?:struct|enum|union|typedef\s+\w+\s+)\s*(\w+)\s*[{;]")
        .expect("invalid regex");

    let mut types = Vec::new();
    for cap in re.captures_iter(c_source) {
        types.push(cap[1].to_string());
    }
    types.sort();
    types.dedup();
    types
}

/// Recursively walk a tree-sitter tree, calling the visitor on each node.
fn walk_tree<F>(node: tree_sitter::Node, source: &str, visitor: &mut F)
where
    F: FnMut(tree_sitter::Node, &str),
{
    visitor(node, source);
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_tree(child, source, visitor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_c_functions_simple() {
        let source = r#"
int add(int a, int b) {
    return a + b;
}

void greet(const char *name) {
    printf("Hello %s\n", name);
}
"#;
        let funcs = extract_c_functions(source);
        let names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"add"), "should find add, got {:?}", names);
        assert!(
            names.contains(&"greet"),
            "should find greet, got {:?}",
            names
        );
    }

    #[test]
    fn test_extract_c_functions_static() {
        let source = "static int helper(int x) { return x * 2; }\n";
        let funcs = extract_c_functions(source);
        assert!(funcs.iter().any(|f| f.name == "helper"));
    }

    #[test]
    fn test_extract_c_functions_pointer_return() {
        let source = "char *get_name(int id) { return names[id]; }\n";
        let funcs = extract_c_functions(source);
        assert!(funcs.iter().any(|f| f.name == "get_name"));
    }

    #[test]
    fn test_extract_c_calls() {
        let source = r#"
int compute(int x) {
    int a = helper(x);
    int b = transform(a);
    return a + b;
}
"#;
        let known = vec![
            "compute".to_string(),
            "helper".to_string(),
            "transform".to_string(),
        ];
        let calls = extract_c_calls(source, &known);
        assert!(calls.contains(&"helper".to_string()));
        assert!(calls.contains(&"transform".to_string()));
    }

    #[test]
    fn test_extract_c_calls_ignores_unknown() {
        let source = "int f() { return printf(\"hi\"); }";
        let known = vec!["f".to_string()];
        let calls = extract_c_calls(source, &known);
        assert!(!calls.contains(&"printf".to_string()));
    }

    #[test]
    fn test_classify_easy() {
        let source = "int add(int a, int b) { return a + b; }";
        assert_eq!(classify_difficulty_ast(source), Difficulty::Easy);
    }

    #[test]
    fn test_classify_medium_pointer() {
        let source = r#"
void copy(char *dst, const char *src) {
    while (*src) {
        *dst++ = *src++;
    }
    *dst = '\0';
}
"#;
        assert_eq!(classify_difficulty_ast(source), Difficulty::Medium);
    }

    #[test]
    fn test_classify_hard_void_ptr() {
        let source = r#"
void* generic_alloc(void *ctx, size_t size) {
    void *ptr = malloc(size);
    if (!ptr) return NULL;
    memset(ptr, 0, size);
    return ptr;
}
"#;
        assert_eq!(classify_difficulty_ast(source), Difficulty::Hard);
    }

    #[test]
    fn test_classify_hard_goto() {
        let source = r#"
int process(int x) {
    if (x < 0) goto error;
    return x * 2;
error:
    return -1;
}
"#;
        assert_eq!(classify_difficulty_ast(source), Difficulty::Hard);
    }

    #[test]
    fn test_classify_hard_union() {
        let source = r#"
union Value {
    int i;
    float f;
    char *s;
};
int use_union() { union Value v; v.i = 42; return v.i; }
"#;
        assert_eq!(classify_difficulty_ast(source), Difficulty::Hard);
    }

    #[test]
    fn test_count_unsafe_blocks_none() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        assert_eq!(count_unsafe_blocks_ast(source), 0);
    }

    #[test]
    fn test_count_unsafe_blocks_one_block() {
        let source = r#"
fn uses_unsafe() {
    unsafe {
        std::ptr::null_mut::<u8>();
    }
}
"#;
        assert_eq!(count_unsafe_blocks_ast(source), 1);
    }

    #[test]
    fn test_count_unsafe_fn() {
        let source = "unsafe fn dangerous() -> *mut u8 { std::ptr::null_mut() }";
        assert_eq!(count_unsafe_blocks_ast(source), 1);
    }

    #[test]
    fn test_count_unsafe_combined() {
        let source = r#"
unsafe fn dangerous() -> *mut u8 {
    std::ptr::null_mut()
}

fn uses_unsafe() {
    unsafe {
        dangerous();
    }
}
"#;
        assert_eq!(count_unsafe_blocks_ast(source), 2);
    }

    #[test]
    fn test_count_unsafe_in_comment_ignored() {
        let source = "fn safe() {} // unsafe { this is a comment }";
        assert_eq!(count_unsafe_blocks_ast(source), 0);
    }

    #[test]
    fn test_extract_c_types_struct() {
        let source = r#"
struct Node {
    int value;
    struct Node *next;
};
"#;
        let types = extract_c_types(source);
        assert!(types.contains(&"Node".to_string()), "got {:?}", types);
    }

    #[test]
    fn test_extract_c_types_enum() {
        let source = "enum Color { RED, GREEN, BLUE };\n";
        let types = extract_c_types(source);
        assert!(types.contains(&"Color".to_string()), "got {:?}", types);
    }

    #[test]
    fn test_extract_c_types_typedef() {
        let source = "typedef unsigned long size_t;\ntypedef struct Node Node;\n";
        let types = extract_c_types(source);
        // Should find at least one typedef name
        assert!(!types.is_empty(), "should find typedefs, got {:?}", types);
    }

    #[test]
    fn test_hash_table_fixture_functions() {
        let source =
            std::fs::read_to_string("tests/fixtures/medium/hash_table.c").unwrap_or_default();
        if source.is_empty() {
            return; // skip if fixture not available
        }
        let funcs = extract_c_functions(&source);
        let names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();
        // hash_table.c should have several functions
        assert!(!funcs.is_empty(), "should find functions in hash_table.c");
        // Common functions in a hash table implementation
        assert!(
            names
                .iter()
                .any(|n| n.contains("hash") || n.contains("create") || n.contains("insert")),
            "should find hash table functions, got {:?}",
            names
        );
    }

    #[test]
    fn test_difficulty_hash_table() {
        let source =
            std::fs::read_to_string("tests/fixtures/medium/hash_table.c").unwrap_or_default();
        if source.is_empty() {
            return;
        }
        let difficulty = classify_difficulty_ast(&source);
        // hash_table.c uses malloc, pointers extensively
        assert_ne!(
            difficulty,
            Difficulty::Easy,
            "hash_table should not be Easy"
        );
    }

    #[test]
    fn test_string_literal_not_confused_with_code() {
        // "goto" inside a string literal should not trigger Hard classification
        let source = r#"
int f() {
    printf("goto is a keyword\n");
    return 0;
}
"#;
        // This depends on tree-sitter correctly parsing string literals
        let difficulty = classify_difficulty_ast(source);
        assert_eq!(
            difficulty,
            Difficulty::Easy,
            "string content should not affect classification"
        );
    }
}
