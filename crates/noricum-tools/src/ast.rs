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
        if node.kind() == "function_definition"
            && let Some(func) = parse_function_def(node, src)
        {
            functions.push(func);
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
        if let Some(child) = node.child(i)
            && let Some(id) = find_identifier(child, source)
        {
            return Some(id);
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
        if node.kind() == "call_expression"
            && let Some(func_node) = node.child(0)
            && let Ok(name) = func_node.utf8_text(src.as_bytes())
            && known_set.contains(name)
        {
            calls.push(name.to_string());
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
                if let Ok(text) = node.utf8_text(src.as_bytes())
                    && text.contains("void")
                    && text.contains('*')
                {
                    void_ptr = true;
                }
            }
            "function_declarator" => {
                // Check for function pointer parameters (callbacks)
                if let Some(parent) = node.parent()
                    && parent.kind() == "pointer_declarator"
                    && let Some(grandparent) = parent.parent()
                    && grandparent.kind() == "parameter_declaration"
                {
                    callback = true;
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
                    if let Some(child) = node.child(i)
                        && (child.kind() == "type_identifier" || child.kind() == "identifier")
                        && let Ok(name) = child.utf8_text(src.as_bytes())
                    {
                        types.push(name.to_string());
                    }
                }
            }
            "type_definition" => {
                // typedef: the last identifier is the alias name
                if let Some(last_child) = node.child(node.child_count().saturating_sub(2))
                    && last_child.kind() == "type_identifier"
                    && let Ok(name) = last_child.utf8_text(src.as_bytes())
                {
                    types.push(name.to_string());
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

/// A chunk of C source for multi-pass translation.
///
/// Each chunk contains the shared context (types, globals, includes) plus a
/// subset of function bodies. The shared context is the same across all chunks.
#[derive(Debug, Clone)]
pub struct CChunk {
    /// Shared context: #includes, typedefs, struct/enum declarations, globals.
    pub shared_context: String,
    /// Concatenated function source code for this chunk.
    pub functions_source: String,
    /// Names of functions in this chunk.
    pub function_names: Vec<String>,
    /// Line count of functions_source.
    pub line_count: usize,
    /// P8: Whether this chunk is a data-only chunk (static arrays, lookup tables).
    /// Data chunks get specialized transcription instructions instead of translation prompts.
    pub is_data_chunk: bool,
}

/// Split a large C source file into chunks for multi-pass translation.
///
/// Each chunk carries the full shared context (includes, types, globals)
/// and a subset of function bodies, grouped to stay near `target_chunk_lines`.
pub fn chunk_c_source(c_source: &str, target_chunk_lines: usize) -> Vec<CChunk> {
    let functions = extract_c_functions(c_source);

    if functions.is_empty() {
        return vec![CChunk {
            shared_context: c_source.to_string(),
            functions_source: String::new(),
            function_names: Vec::new(),
            line_count: c_source.lines().count(),
            is_data_chunk: false,
        }];
    }

    // Build shared_context = everything NOT inside function bodies
    let mut shared_lines = Vec::new();
    let source_bytes = c_source.as_bytes();
    let mut pos = 0;
    for func in &functions {
        if func.start_byte > pos {
            let before = &c_source[pos..func.start_byte];
            for line in before.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    shared_lines.push(line.to_string());
                }
            }
        }
        // Add just the function signature (first line)
        let func_text = &c_source[func.start_byte..func.end_byte];
        if let Some(first_line) = func_text.lines().next() {
            shared_lines.push(format!(
                "{} // ...",
                first_line.trim().trim_end_matches('{')
            ));
        }
        pos = func.end_byte;
    }
    // Anything after the last function
    if pos < source_bytes.len() {
        let after = &c_source[pos..];
        for line in after.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                shared_lines.push(line.to_string());
            }
        }
    }
    let shared_context = shared_lines.join("\n");

    // Group functions into chunks of ~target_chunk_lines
    let mut chunks = Vec::new();
    let mut current_source = String::new();
    let mut current_names = Vec::new();
    let mut current_lines = 0usize;

    for func in &functions {
        let func_text = &c_source[func.start_byte..func.end_byte];
        let func_lines = func_text.lines().count();

        if !current_names.is_empty() && current_lines + func_lines > target_chunk_lines {
            chunks.push(CChunk {
                shared_context: shared_context.clone(),
                functions_source: current_source.clone(),
                function_names: current_names.clone(),
                line_count: current_lines,
                is_data_chunk: false,
            });
            current_source.clear();
            current_names.clear();
            current_lines = 0;
        }

        if !current_source.is_empty() {
            current_source.push_str("\n\n");
        }
        current_source.push_str(func_text);
        current_names.push(func.name.clone());
        current_lines += func_lines;
    }

    if !current_names.is_empty() {
        chunks.push(CChunk {
            shared_context,
            functions_source: current_source,
            function_names: current_names,
            line_count: current_lines,
            is_data_chunk: false,
        });
    }

    chunks
}

/// Check if a C function is likely a data model function (constructor, getter, setter, type checker).
fn is_data_model_function(func: &CFunction, known_types: &[String]) -> bool {
    let name_lower = func.name.to_lowercase();
    let is_short = func.body.lines().count() < 20;
    let has_model_prefix = name_lower.contains("create")
        || name_lower.contains("new")
        || name_lower.contains("init")
        || name_lower.contains("is_")
        || name_lower.contains("get_")
        || name_lower.contains("set_");

    // Also match cJSON-style naming: cJSON_Create*, cJSON_Is*, cJSON_Get*
    let has_cstyle_prefix = known_types.iter().any(|t| {
        let prefix = t.to_lowercase();
        name_lower.starts_with(&prefix)
            && (name_lower.contains("create")
                || name_lower.contains("is")
                || name_lower.contains("get")
                || name_lower.contains("set"))
    });

    is_short && (has_model_prefix || has_cstyle_prefix)
}

/// P8: Detect large static data blocks (arrays, lookup tables) in C source.
///
/// Returns the byte ranges `(start, end)` of blocks that are large static const arrays
/// (e.g., glyph tables, sine lookup tables, CRC tables). These should be extracted
/// into their own data chunk with transcription-only instructions.
pub fn detect_static_data_blocks(c_source: &str) -> Vec<(usize, usize, String)> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = c_source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        // Match patterns like: static TYPE name[...] = {
        // or: const TYPE name[...] = {
        // or: static const TYPE name[...][...] = {
        let is_static_array = (trimmed.starts_with("static ") || trimmed.starts_with("const "))
            && trimmed.contains('[')
            && (trimmed.contains("= {") || trimmed.ends_with("= {"));

        if is_static_array {
            let start_line = i;
            // Find the name for labeling
            let name = trimmed
                .split('[')
                .next()
                .and_then(|s| s.split_whitespace().last())
                .unwrap_or("data")
                .to_string();

            // Count through to matching closing brace
            let mut brace_depth: i32 = 0;
            let mut end_line = i;
            for (j, line) in lines.iter().enumerate().skip(i) {
                let opens = line.chars().filter(|&c| c == '{').count() as i32;
                let closes = line.chars().filter(|&c| c == '}').count() as i32;
                brace_depth += opens - closes;
                end_line = j;
                if brace_depth <= 0 && opens + closes > 0 {
                    break;
                }
            }

            let block_lines = end_line - start_line + 1;
            // Only flag as data block if it's substantial (>30 lines)
            if block_lines > 30 {
                let start_byte = c_source
                    .lines()
                    .take(start_line)
                    .map(|l| l.len() + 1)
                    .sum::<usize>();
                let end_byte = c_source
                    .lines()
                    .take(end_line + 1)
                    .map(|l| l.len() + 1)
                    .sum::<usize>();
                blocks.push((
                    start_byte.min(c_source.len()),
                    end_byte.min(c_source.len()),
                    name,
                ));
                debug!(
                    name = %blocks.last().unwrap().2,
                    lines = block_lines,
                    "P8: detected static data block"
                );
            }
            i = end_line + 1;
        } else {
            i += 1;
        }
    }
    blocks
}

/// Split C source into chunks with data model (structs/enums + constructors/getters) in chunk 0.
///
/// Puts struct/enum/typedef definitions and their associated constructor/getter functions
/// into the first chunk, then groups remaining functions into subsequent chunks.
/// Falls back to regular `chunk_c_source` when no structs are detected.
/// P8: Also detects large static data blocks and puts them in separate data chunks.
pub fn chunk_c_source_structural(c_source: &str, target_chunk_lines: usize) -> Vec<CChunk> {
    let functions = extract_c_functions(c_source);
    let types = extract_c_types(c_source);

    if types.is_empty() || functions.is_empty() {
        return chunk_c_source(c_source, target_chunk_lines);
    }

    // Separate data model functions from logic functions
    let mut model_funcs: Vec<&CFunction> = Vec::new();
    let mut logic_funcs: Vec<&CFunction> = Vec::new();

    for func in &functions {
        if is_data_model_function(func, &types) {
            model_funcs.push(func);
        } else {
            logic_funcs.push(func);
        }
    }

    if model_funcs.is_empty() {
        return chunk_c_source(c_source, target_chunk_lines);
    }

    // Build shared context (same as chunk_c_source)
    let mut shared_lines = Vec::new();
    let mut pos = 0;
    for func in &functions {
        if func.start_byte > pos {
            let before = &c_source[pos..func.start_byte];
            for line in before.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    shared_lines.push(line.to_string());
                }
            }
        }
        let func_text = &c_source[func.start_byte..func.end_byte];
        if let Some(first_line) = func_text.lines().next() {
            shared_lines.push(format!(
                "{} // ...",
                first_line.trim().trim_end_matches('{')
            ));
        }
        pos = func.end_byte;
    }
    if pos < c_source.len() {
        let after = &c_source[pos..];
        for line in after.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                shared_lines.push(line.to_string());
            }
        }
    }
    let shared_context = shared_lines.join("\n");

    // Chunk 0: data model functions
    let mut chunks = Vec::new();
    let mut model_source = String::new();
    let mut model_names = Vec::new();
    let mut model_lines = 0;
    for func in &model_funcs {
        let func_text = &c_source[func.start_byte..func.end_byte];
        if !model_source.is_empty() {
            model_source.push_str("\n\n");
        }
        model_source.push_str(func_text);
        model_names.push(func.name.clone());
        model_lines += func_text.lines().count();
    }
    chunks.push(CChunk {
        shared_context: shared_context.clone(),
        functions_source: model_source,
        function_names: model_names,
        line_count: model_lines,
        is_data_chunk: false,
    });

    // P8: Detect large static data blocks and add as separate data chunk
    let data_blocks = detect_static_data_blocks(c_source);
    if !data_blocks.is_empty() {
        let mut data_source = String::new();
        let mut data_names = Vec::new();
        let mut data_lines = 0;
        for (start, end, name) in &data_blocks {
            let block_text = &c_source[*start..*end];
            if !data_source.is_empty() {
                data_source.push_str("\n\n");
            }
            data_source.push_str(block_text);
            data_names.push(name.clone());
            data_lines += block_text.lines().count();
        }
        if data_lines > 0 {
            debug!(
                blocks = data_blocks.len(),
                data_lines, "P8: adding data-only chunk"
            );
            chunks.push(CChunk {
                shared_context: shared_context.clone(),
                functions_source: data_source,
                function_names: data_names,
                line_count: data_lines,
                is_data_chunk: true,
            });
        }
    }

    // Remaining chunks: logic functions grouped by target size
    let mut current_source = String::new();
    let mut current_names = Vec::new();
    let mut current_lines = 0;
    for func in &logic_funcs {
        let func_text = &c_source[func.start_byte..func.end_byte];
        let func_lines = func_text.lines().count();

        if !current_names.is_empty() && current_lines + func_lines > target_chunk_lines {
            chunks.push(CChunk {
                shared_context: shared_context.clone(),
                functions_source: current_source.clone(),
                function_names: current_names.clone(),
                line_count: current_lines,
                is_data_chunk: false,
            });
            current_source.clear();
            current_names.clear();
            current_lines = 0;
        }

        if !current_source.is_empty() {
            current_source.push_str("\n\n");
        }
        current_source.push_str(func_text);
        current_names.push(func.name.clone());
        current_lines += func_lines;
    }
    if !current_names.is_empty() {
        chunks.push(CChunk {
            shared_context,
            functions_source: current_source,
            function_names: current_names,
            line_count: current_lines,
            is_data_chunk: false,
        });
    }

    chunks
}

/// P3: A logical module extracted from a large C source file.
///
/// Groups related functions together for incremental per-module migration.
#[derive(Debug, Clone)]
pub struct CModule {
    /// Module name (derived from common prefix or domain).
    pub name: String,
    /// The C source for this module (shared context + functions).
    pub source: String,
    /// Function names in this module.
    pub function_names: Vec<String>,
    /// Line count.
    pub line_count: usize,
}

/// P3: Split a large C source into logical modules for incremental migration.
///
/// Groups functions by common prefix (e.g., `hash_` functions go in "hash" module,
/// `parse_` functions go in "parse" module). Functions without a common prefix
/// go into a "misc" module. Shared context (types, includes) is prepended to each.
pub fn split_into_modules(c_source: &str) -> Vec<CModule> {
    let functions = extract_c_functions(c_source);
    if functions.len() < 4 {
        // Too few functions to split into modules
        return vec![CModule {
            name: "main".to_string(),
            source: c_source.to_string(),
            function_names: functions.iter().map(|f| f.name.clone()).collect(),
            line_count: c_source.lines().count(),
        }];
    }

    // Extract shared context (everything outside function bodies)
    let mut shared_lines = Vec::new();
    let mut pos = 0;
    for func in &functions {
        if func.start_byte > pos {
            let before = &c_source[pos..func.start_byte];
            for line in before.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    shared_lines.push(line.to_string());
                }
            }
        }
        pos = func.end_byte;
    }
    if pos < c_source.len() {
        let after = &c_source[pos..];
        for line in after.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                shared_lines.push(line.to_string());
            }
        }
    }
    let shared_context = shared_lines.join("\n");

    // Group functions by prefix (first word before '_' or common pattern)
    let mut groups: std::collections::BTreeMap<String, Vec<&CFunction>> =
        std::collections::BTreeMap::new();

    for func in &functions {
        let prefix = extract_function_prefix(&func.name);
        groups.entry(prefix).or_default().push(func);
    }

    // Maximum LOC per module before sub-splitting
    const MAX_MODULE_LOC: usize = 1000;

    // Merge small groups (< 2 functions) into "misc"
    let mut modules = Vec::new();
    let mut misc_funcs: Vec<&CFunction> = Vec::new();

    for (prefix, funcs) in &groups {
        if funcs.len() < 2 || prefix == "main" {
            misc_funcs.extend(funcs);
        } else {
            // Check total LOC for this prefix group
            let total_func_lines: usize = funcs
                .iter()
                .map(|f| c_source[f.start_byte..f.end_byte].lines().count())
                .sum();

            if total_func_lines > MAX_MODULE_LOC {
                // Sub-split large prefix groups into ~600 LOC sub-modules
                let sub_modules =
                    sub_split_function_group(prefix, funcs, c_source, &shared_context, 600);
                modules.extend(sub_modules);
            } else {
                let mut source = shared_context.clone();
                source.push_str("\n\n// === Module functions ===\n");
                let mut names = Vec::new();
                for func in funcs {
                    let func_text = &c_source[func.start_byte..func.end_byte];
                    source.push('\n');
                    source.push_str(func_text);
                    names.push(func.name.clone());
                }
                let line_count = source.lines().count();
                modules.push(CModule {
                    name: prefix.clone(),
                    source,
                    function_names: names,
                    line_count,
                });
            }
        }
    }

    if !misc_funcs.is_empty() {
        // Also sub-split misc if it's too large
        let total_misc_lines: usize = misc_funcs
            .iter()
            .map(|f| c_source[f.start_byte..f.end_byte].lines().count())
            .sum();

        if total_misc_lines > MAX_MODULE_LOC {
            let sub_modules =
                sub_split_function_group("misc", &misc_funcs, c_source, &shared_context, 600);
            modules.extend(sub_modules);
        } else {
            let mut source = shared_context;
            source.push_str("\n\n// === Miscellaneous functions ===\n");
            let mut names = Vec::new();
            for func in &misc_funcs {
                let func_text = &c_source[func.start_byte..func.end_byte];
                source.push('\n');
                source.push_str(func_text);
                names.push(func.name.clone());
            }
            let line_count = source.lines().count();
            modules.push(CModule {
                name: "misc".to_string(),
                source,
                function_names: names,
                line_count,
            });
        }
    }

    modules
}

/// Sub-split a large function group into smaller sub-modules of ~`target_lines` LOC each.
fn sub_split_function_group(
    prefix: &str,
    funcs: &[&CFunction],
    c_source: &str,
    shared_context: &str,
    target_lines: usize,
) -> Vec<CModule> {
    let mut sub_modules = Vec::new();
    let mut current_names = Vec::new();
    let mut current_source = String::new();
    let mut current_lines = 0;
    let mut part = 1;

    for func in funcs {
        let func_text = &c_source[func.start_byte..func.end_byte];
        let func_lines = func_text.lines().count();

        // Start a new sub-module if adding this function would exceed target
        if !current_names.is_empty() && current_lines + func_lines > target_lines {
            let mut source = shared_context.to_string();
            source.push_str(&format!(
                "\n\n// === Module functions ({prefix} part {part}) ===\n"
            ));
            source.push_str(&current_source);
            let line_count = source.lines().count();
            sub_modules.push(CModule {
                name: format!("{prefix}_p{part}"),
                source,
                function_names: current_names,
                line_count,
            });
            current_names = Vec::new();
            current_source = String::new();
            current_lines = 0;
            part += 1;
        }

        current_source.push('\n');
        current_source.push_str(func_text);
        current_names.push(func.name.clone());
        current_lines += func_lines;
    }

    // Flush remaining functions
    if !current_names.is_empty() {
        let mut source = shared_context.to_string();
        if part > 1 {
            source.push_str(&format!(
                "\n\n// === Module functions ({prefix} part {part}) ===\n"
            ));
        } else {
            source.push_str("\n\n// === Module functions ===\n");
        }
        source.push_str(&current_source);
        let line_count = source.lines().count();
        sub_modules.push(CModule {
            name: if part > 1 {
                format!("{prefix}_p{part}")
            } else {
                prefix.to_string()
            },
            source,
            function_names: current_names,
            line_count,
        });
    }

    sub_modules
}

/// Extract the prefix of a function name for module grouping.
///
/// For `hash_insert` → "hash", `cJSON_Parse` → "cjson", `main` → "main".
fn extract_function_prefix(name: &str) -> String {
    // Try underscore-separated prefix
    if let Some(idx) = name.find('_') {
        let prefix = &name[..idx];
        if !prefix.is_empty() && prefix.len() > 1 {
            return prefix.to_lowercase();
        }
    }
    // Try camelCase prefix (e.g., cJSON_Parse → cJSON → cjson)
    let mut prefix_end = 0;
    let chars: Vec<char> = name.chars().collect();
    for i in 1..chars.len() {
        if chars[i].is_uppercase() && chars[i - 1].is_lowercase() {
            prefix_end = i;
            break;
        }
    }
    if prefix_end > 1 {
        return name[..prefix_end].to_lowercase();
    }
    name.to_lowercase()
}

/// Extract function signatures from Rust source code for dependency context.
///
/// Looks for `pub fn` and `fn` lines, returning them as context strings
/// that can be injected into translation prompts for dependent files.
pub fn extract_rust_signatures(rust_source: &str) -> Vec<String> {
    rust_source
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            (trimmed.starts_with("pub fn ") || trimmed.starts_with("fn ")) && trimmed.contains('(')
        })
        .map(|line| {
            let trimmed = line.trim();
            if let Some(brace) = trimmed.find('{') {
                trimmed[..brace].trim().to_string()
            } else {
                trimmed.to_string()
            }
        })
        .collect()
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

    #[test]
    fn test_chunk_short_source_single_chunk() {
        let source = "int add(int a, int b) { return a + b; }\n";
        let chunks = chunk_c_source(source, 500);
        assert_eq!(chunks.len(), 1, "short source should produce 1 chunk");
        assert_eq!(chunks[0].function_names, vec!["add"]);
    }

    #[test]
    fn test_chunk_multiple_functions() {
        let source = "\
int f1(int x) {
    int a = 1;
    int b = 2;
    int c = 3;
    int d = 4;
    int e = 5;
    int ff = 6;
    int g = 7;
    int h = 8;
    return x + a + b + c + d + e + ff + g + h;
}
int f2(int x) {
    int a = 1;
    int b = 2;
    int c = 3;
    int d = 4;
    int e = 5;
    int ff = 6;
    int g = 7;
    int h = 8;
    return x + a + b + c + d + e + ff + g + h;
}
int f3(int x) {
    int a = 1;
    int b = 2;
    int c = 3;
    int d = 4;
    int e = 5;
    int ff = 6;
    int g = 7;
    int h = 8;
    return x + a + b + c + d + e + ff + g + h;
}
";
        // Each function is 11 lines. With target=25, f1+f2 fit (22), f3 goes to chunk 2
        let chunks = chunk_c_source(source, 25);
        assert_eq!(
            chunks.len(),
            2,
            "should produce 2 chunks, got {}",
            chunks.len()
        );
        assert_eq!(chunks[0].function_names.len(), 2);
        assert_eq!(chunks[1].function_names.len(), 1);
        // All chunks share the same context
        assert_eq!(chunks[0].shared_context, chunks[1].shared_context);
    }

    #[test]
    fn test_extract_rust_signatures_basic() {
        let rust = "\
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn helper(x: i32) -> i32 {
    x * 2
}
";
        let sigs = extract_rust_signatures(rust);
        assert_eq!(sigs.len(), 2);
        assert!(sigs[0].contains("pub fn add"));
        assert!(sigs[1].contains("fn helper"));
    }

    #[test]
    fn test_extract_rust_signatures_empty() {
        let sigs = extract_rust_signatures("let x = 5;\nstruct Foo { bar: i32 }");
        assert!(sigs.is_empty());
    }

    #[test]
    fn test_chunk_structural_puts_data_model_first() {
        let source = "\
typedef struct Node Node;
struct Node {
    int value;
    struct Node *next;
};

Node *create_node(int val) {
    Node *n = malloc(sizeof(Node));
    n->value = val;
    n->next = NULL;
    return n;
}

int get_value(Node *n) {
    return n->value;
}

void process_list(Node *head) {
    int sum = 0;
    while (head) {
        sum += head->value;
        head = head->next;
    }
    printf(\"%d\\n\", sum);
}

void complex_transform(Node *head, int factor) {
    while (head) {
        head->value *= factor;
        head = head->next;
    }
}
";
        let chunks = chunk_c_source_structural(source, 20);
        assert!(
            chunks.len() >= 2,
            "structural chunking should produce 2+ chunks, got {}",
            chunks.len()
        );
        // Chunk 0 should contain data model functions
        let chunk0_names = &chunks[0].function_names;
        assert!(
            chunk0_names.iter().any(|n| n == "create_node"),
            "chunk 0 should contain create_node, got {:?}",
            chunk0_names
        );
        assert!(
            chunk0_names.iter().any(|n| n == "get_value"),
            "chunk 0 should contain get_value, got {:?}",
            chunk0_names
        );
        // Logic functions should be in later chunks
        let later_names: Vec<&str> = chunks[1..]
            .iter()
            .flat_map(|c| c.function_names.iter().map(|s| s.as_str()))
            .collect();
        assert!(
            later_names.contains(&"process_list") || later_names.contains(&"complex_transform"),
            "later chunks should contain logic functions, got {:?}",
            later_names
        );
    }

    #[test]
    fn test_chunk_structural_no_structs_falls_back() {
        let source = "\
int add(int a, int b) { return a + b; }
int mul(int a, int b) { return a * b; }
";
        let structural = chunk_c_source_structural(source, 500);
        let regular = chunk_c_source(source, 500);
        assert_eq!(
            structural.len(),
            regular.len(),
            "no structs should produce same as regular chunking"
        );
    }

    #[test]
    fn test_extract_function_prefix() {
        assert_eq!(extract_function_prefix("hash_insert"), "hash");
        assert_eq!(extract_function_prefix("hash_delete"), "hash");
        assert_eq!(extract_function_prefix("parse_expr"), "parse");
        assert_eq!(extract_function_prefix("main"), "main");
        assert_eq!(extract_function_prefix("getValue"), "get");
    }

    #[test]
    fn test_split_into_modules_groups_by_prefix() {
        let source = "\
void hash_insert(int k, int v) { }
void hash_delete(int k) { }
void hash_lookup(int k) { }
void parse_expr(const char *s) { }
void parse_stmt(const char *s) { }
int main() { return 0; }
";
        let modules = split_into_modules(source);
        let names: Vec<&str> = modules.iter().map(|m| m.name.as_str()).collect();
        assert!(
            names.contains(&"hash"),
            "should have hash module, got {:?}",
            names
        );
        assert!(
            names.contains(&"parse"),
            "should have parse module, got {:?}",
            names
        );

        let hash_mod = modules.iter().find(|m| m.name == "hash").unwrap();
        assert_eq!(hash_mod.function_names.len(), 3);
    }

    #[test]
    fn test_split_into_modules_few_functions() {
        let source = "\
int add(int a, int b) { return a + b; }
int sub(int a, int b) { return a - b; }
";
        let modules = split_into_modules(source);
        assert_eq!(
            modules.len(),
            1,
            "too few functions should produce 1 module"
        );
        assert_eq!(modules[0].name, "main");
    }

    #[test]
    fn test_split_into_modules_merges_small_groups() {
        let source = "\
void hash_insert(int k, int v) { }
void hash_delete(int k) { }
void parse_expr(const char *s) { }
void parse_stmt(const char *s) { }
void unique_function(int x) { }
int main() { return 0; }
";
        let modules = split_into_modules(source);
        // unique_function and main should be in misc
        let misc = modules.iter().find(|m| m.name == "misc");
        assert!(misc.is_some(), "should have misc module for singletons");
    }

    #[test]
    fn test_split_into_modules_miniz_zip_fixture() {
        let source =
            std::fs::read_to_string("../../tests/fixtures/miniz/miniz_zip.c").unwrap_or_default();
        if source.is_empty() {
            return; // skip if fixture not available
        }
        let modules = split_into_modules(&source);
        // miniz_zip.c is ~4895 LOC — should produce multiple modules
        assert!(
            modules.len() > 1,
            "miniz_zip.c should produce >1 module, got {}",
            modules.len()
        );
        // Build debug info for assertion messages
        let debug_info: Vec<String> = modules
            .iter()
            .map(|m| format!("{}: {} LOC, {} fns", m.name, m.line_count, m.function_names.len()))
            .collect();
        // No single module should exceed ~2500 LOC (shared context + functions)
        for m in &modules {
            assert!(
                m.line_count < 2500,
                "module {} has {} LOC (too large for single-pass LLM), has {} functions.\nAll modules: {:?}",
                m.name,
                m.line_count,
                m.function_names.len(),
                debug_info
            );
        }
    }

    #[test]
    fn test_split_into_modules_sub_splits_large_groups() {
        // Generate a large prefix group that exceeds MAX_MODULE_LOC (1000 LOC)
        let mut source = String::from("#include <stdio.h>\n\n");
        // Create 30 functions with "mz_" prefix, each ~50 lines
        for i in 0..30 {
            source.push_str(&format!("int mz_func_{i}(int x) {{\n"));
            for j in 0..48 {
                source.push_str(&format!("    int v{j} = x + {j};\n"));
            }
            source.push_str("    return x;\n}\n\n");
        }
        // Add a few functions with "zip_" prefix (small group)
        for i in 0..4 {
            source.push_str(&format!("int zip_func_{i}(int x) {{ return x + {i}; }}\n"));
        }

        let modules = split_into_modules(&source);
        let names: Vec<&str> = modules.iter().map(|m| m.name.as_str()).collect();

        // The "mz" group (30 * 50 = ~1500 LOC) should be sub-split into multiple sub-modules
        let mz_modules: Vec<_> = modules.iter().filter(|m| m.name.starts_with("mz")).collect();
        assert!(
            mz_modules.len() > 1,
            "mz group should be sub-split into multiple modules, got {} module(s): {:?}",
            mz_modules.len(),
            names
        );

        // Each sub-module should have reasonable LOC
        for m in &mz_modules {
            let func_lines: usize = m.function_names.len() * 50; // approximate
            assert!(
                func_lines <= 1200,
                "sub-module {} has ~{} function LOC, should be <= ~1200",
                m.name,
                func_lines
            );
        }

        // "zip" group should remain as a single module (small enough)
        let zip_modules: Vec<_> = modules.iter().filter(|m| m.name.starts_with("zip")).collect();
        assert_eq!(
            zip_modules.len(),
            1,
            "zip group should remain as single module"
        );
    }

    #[test]
    fn test_detect_static_data_blocks_finds_large_array() {
        let mut source = String::from("static const int lookup[100] = {\n");
        for i in 0..50 {
            source.push_str(&format!("    {i}, {i},\n")); // 50 lines of data
        }
        source.push_str("};\n");
        let blocks = detect_static_data_blocks(&source);
        assert_eq!(blocks.len(), 1, "should detect one data block");
        assert_eq!(blocks[0].2, "lookup");
    }

    #[test]
    fn test_detect_static_data_blocks_ignores_small_array() {
        let source = "static const int small[3] = {\n    1, 2, 3\n};\n";
        let blocks = detect_static_data_blocks(source);
        assert!(blocks.is_empty(), "small arrays should be ignored");
    }

    #[test]
    fn test_chunk_has_is_data_chunk_field() {
        let source = "int add(int a, int b) { return a + b; }\n";
        let chunks = chunk_c_source(source, 400);
        assert!(!chunks.is_empty());
        assert!(!chunks[0].is_data_chunk, "normal chunk should not be data");
    }
}
