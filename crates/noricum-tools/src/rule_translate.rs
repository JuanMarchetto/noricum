/// Rule-based C-to-Rust translator for simple functions.
///
/// This is the zero-dependency fallback: no c2rust, no LLM needed.
/// Handles simple patterns: arithmetic, control flow (if/else, while, for),
/// variable declarations, printf, and function calls.
/// For anything complex (pointers, structs, malloc, goto), returns None
/// and the caller should escalate to LLM.
use tracing::{debug, info};

/// Attempt to translate a C source file to Rust using rule-based patterns.
///
/// Returns `Some(rust_source)` if the function is simple enough to translate
/// deterministically, `None` if it requires LLM assistance.
pub fn try_translate(c_source: &str, function_name: &str) -> Option<String> {
    let functions = extract_functions(c_source);
    if functions.is_empty() {
        debug!(function = function_name, "no functions found in source");
        return None;
    }

    let mut rust_parts: Vec<String> = Vec::new();
    let mut main_body: Option<String> = None;

    for func in &functions {
        if func.name == "main" {
            main_body = translate_main(&func.body);
        } else if let Some(rust_fn) = translate_function(func) {
            rust_parts.push(rust_fn);
        } else {
            debug!(
                function = func.name,
                "function too complex for rule-based translation"
            );
            return None;
        }
    }

    if rust_parts.is_empty() && main_body.is_none() {
        return None;
    }

    let mut output = String::new();
    for part in &rust_parts {
        output.push_str(part);
        output.push_str("\n\n");
    }
    if let Some(main) = main_body {
        output.push_str(&main);
        output.push('\n');
    }

    info!(
        function = function_name,
        functions = functions.len(),
        "rule-based translation succeeded"
    );

    Some(output.trim().to_string())
}

// --- Data structures ---

#[derive(Debug)]
struct CFunction {
    name: String,
    return_type: String,
    params: Vec<(String, String)>, // (type, name)
    body: String,
}

// --- Function extraction ---

fn extract_functions(source: &str) -> Vec<CFunction> {
    let mut functions = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        if line.starts_with('#') || line.starts_with("//") || line.is_empty() {
            i += 1;
            continue;
        }

        if let Some(func) = try_parse_function_start(line)
            && let Some(start) = find_brace_line(&lines, i)
            && let Some((body, end)) = extract_braced_block(&lines, start)
        {
            functions.push(CFunction {
                name: func.0,
                return_type: func.1,
                params: func.2,
                body,
            });
            i = end + 1;
            continue;
        }

        i += 1;
    }

    functions
}

type FuncSignature = (String, String, Vec<(String, String)>);

fn try_parse_function_start(line: &str) -> Option<FuncSignature> {
    let paren_open = line.find('(')?;
    let paren_close = line.find(')')?;
    if paren_open >= paren_close {
        return None;
    }

    let before_paren = line[..paren_open].trim();
    let parts: Vec<&str> = before_paren.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let name = parts.last()?.trim_start_matches('*').to_string();
    let return_type = parts[..parts.len() - 1].join(" ");

    if [
        "typedef", "struct", "enum", "union", "if", "while", "for", "switch",
    ]
    .contains(&return_type.as_str())
    {
        return None;
    }

    let params_str = &line[paren_open + 1..paren_close];
    let params = parse_params(params_str);

    Some((name, return_type, params))
}

fn parse_params(params_str: &str) -> Vec<(String, String)> {
    let trimmed = params_str.trim();
    if trimmed.is_empty() || trimmed == "void" {
        return Vec::new();
    }

    trimmed
        .split(',')
        .filter_map(|p| {
            let p = p.trim();
            let parts: Vec<&str> = p.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts.last()?.trim_start_matches('*').to_string();
                let ty = parts[..parts.len() - 1].join(" ");
                Some((ty, name))
            } else {
                None
            }
        })
        .collect()
}

fn find_brace_line(lines: &[&str], start: usize) -> Option<usize> {
    (start..lines.len()).find(|&i| lines[i].contains('{'))
}

/// Extract the content between the first `{` and its matching `}`.
/// Returns the inner text (without outer braces) and the line index of the closing `}`.
fn extract_braced_block(lines: &[&str], start: usize) -> Option<(String, usize)> {
    // Flatten lines from start onward into a single string, tracking line boundaries
    let mut all_text = String::new();
    for line in lines.iter().skip(start) {
        if !all_text.is_empty() {
            all_text.push('\n');
        }
        all_text.push_str(line);
    }

    // Find the first { and its matching }
    let open_pos = all_text.find('{')?;
    let mut depth = 0;
    for (pos, ch) in all_text[open_pos..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let inner = &all_text[open_pos + 1..open_pos + pos];
                    // Count how many lines we consumed
                    let consumed = all_text[..open_pos + pos + 1].matches('\n').count();
                    return Some((inner.to_string(), start + consumed));
                }
            }
            _ => {}
        }
    }
    None
}

// --- Function translation ---

fn translate_function(func: &CFunction) -> Option<String> {
    let rust_ret = c_type_to_rust(&func.return_type)?;
    let mutated = find_mutated_vars(&func.body);
    let rust_params = translate_params_with_mut(&func.params, &mutated)?;
    let rust_body = translate_body(&func.body, "    ")?;

    let ret_str = if rust_ret == "()" {
        String::new()
    } else {
        format!(" -> {rust_ret}")
    };

    Some(format!(
        "pub fn {name}({params}){ret} {{\n{body}\n}}",
        name = func.name,
        params = rust_params,
        ret = ret_str,
        body = rust_body,
    ))
}

/// Find variables that are assigned to in the body (to mark params as `mut`).
fn find_mutated_vars(body: &str) -> Vec<String> {
    let is_ident = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_');
    let mut seen = std::collections::HashSet::new();
    let mut vars = Vec::new();

    let mut add = |name: &str| {
        if seen.insert(name.to_string()) {
            vars.push(name.to_string());
        }
    };

    for line in body.lines() {
        let trimmed = line.trim();

        // x = ...; or x += ...; etc.
        for op in &["=", "+=", "-=", "*=", "/=", "%="] {
            if let Some(pos) = trimmed.find(op) {
                if *op == "=" {
                    let prev_is_compound = pos > 0
                        && matches!(
                            trimmed.as_bytes()[pos - 1],
                            b'!' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%'
                        );
                    let next_is_eq = pos + 1 < trimmed.len() && trimmed.as_bytes()[pos + 1] == b'=';
                    if prev_is_compound || next_is_eq {
                        continue;
                    }
                }
                let lhs = trimmed[..pos].trim();
                if is_ident(lhs) {
                    add(lhs);
                }
                break;
            }
        }

        // x++, x--, ++x, --x
        let no_semi = trimmed.strip_suffix(';').unwrap_or(trimmed).trim();
        for var in [
            no_semi.strip_suffix("++"),
            no_semi.strip_suffix("--"),
            no_semi.strip_prefix("++"),
            no_semi.strip_prefix("--"),
        ]
        .into_iter()
        .flatten()
        {
            if is_ident(var) {
                add(var);
            }
        }
    }
    vars
}

fn translate_params_with_mut(params: &[(String, String)], mutated: &[String]) -> Option<String> {
    let mut rust_params = Vec::new();
    for (ty, name) in params {
        let rust_ty = c_type_to_rust(ty)?;
        if mutated.contains(name) {
            rust_params.push(format!("mut {name}: {rust_ty}"));
        } else {
            rust_params.push(format!("{name}: {rust_ty}"));
        }
    }
    Some(rust_params.join(", "))
}

fn c_type_to_rust(c_type: &str) -> Option<&'static str> {
    match c_type.trim() {
        "int" => Some("i32"),
        "unsigned int" | "unsigned" => Some("u32"),
        "long" | "long long" => Some("i64"),
        "unsigned long" | "unsigned long long" => Some("u64"),
        "short" => Some("i16"),
        "unsigned short" => Some("u16"),
        "char" => Some("i8"),
        "unsigned char" => Some("u8"),
        "float" => Some("f32"),
        "double" => Some("f64"),
        "void" => Some("()"),
        "size_t" => Some("usize"),
        "int32_t" => Some("i32"),
        "uint32_t" => Some("u32"),
        "int64_t" => Some("i64"),
        "uint64_t" => Some("u64"),
        "int16_t" => Some("i16"),
        "uint16_t" => Some("u16"),
        "int8_t" => Some("i8"),
        "uint8_t" => Some("u8"),
        "bool" | "_Bool" => Some("bool"),
        _ => None,
    }
}

// --- Body translation (recursive, handles control flow) ---

fn translate_body(body: &str, indent: &str) -> Option<String> {
    let mut output = Vec::new();
    let lines: Vec<&str> = body.lines().collect();
    let mutated = find_mutated_vars(body);
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        if is_complex(trimmed) {
            return None;
        }

        // if / else if / else
        if trimmed.starts_with("if ") || trimmed.starts_with("if(") {
            let (block, consumed) = translate_if_chain(&lines, i, indent)?;
            output.push(block);
            i += consumed;
            continue;
        }

        // while
        if trimmed.starts_with("while ") || trimmed.starts_with("while(") {
            let (block, consumed) = translate_while_block(&lines, i, indent)?;
            output.push(block);
            i += consumed;
            continue;
        }

        // for
        if trimmed.starts_with("for ") || trimmed.starts_with("for(") {
            let (block, consumed) = translate_for_block(&lines, i, indent)?;
            output.push(block);
            i += consumed;
            continue;
        }

        // Skip lone closing braces (from nested blocks already handled)
        if trimmed == "}" || trimmed.starts_with("} else") {
            i += 1;
            continue;
        }

        let translated = translate_statement(trimmed)?;
        output.push(format!("{indent}{translated}"));
        i += 1;
    }

    let mut result = output.join("\n");

    // Post-process: add `mut` to variable declarations for mutated vars
    for var in &mutated {
        let immutable = format!("let {var}:");
        let mutable = format!("let mut {var}:");
        result = result.replace(&immutable, &mutable);
    }

    Some(result)
}

/// Check if a C expression evaluates to a boolean in Rust but int in C.
fn is_boolean_expr(expr: &str) -> bool {
    for op in &["==", "!=", "<=", ">=", "<", ">", "&&", "||"] {
        if expr.contains(op) {
            return true;
        }
    }
    false
}

const COMPLEX_MARKERS: &[&str] = &[
    "malloc", "free(", "->", "void *", "goto ", "switch ", "sizeof", "memcpy", "memset", "strcpy",
    "strcat",
];

fn is_complex(stmt: &str) -> bool {
    COMPLEX_MARKERS.iter().any(|marker| stmt.contains(marker))
}

// --- Statement translation ---

fn translate_statement(stmt: &str) -> Option<String> {
    let stmt = stmt.trim();

    if stmt == ";" || stmt.is_empty() {
        return Some(String::new());
    }

    // return expr;
    if let Some(rest) = stmt.strip_prefix("return ") {
        let expr = rest.strip_suffix(';')?.trim();
        let rust_expr = translate_expr(expr)?;
        // If expression is boolean but would be returned as int, cast
        if is_boolean_expr(expr) {
            return Some(format!("return ({rust_expr}) as i32;"));
        }
        return Some(format!("return {rust_expr};"));
    }

    if stmt == "return;" {
        return Some("return;".to_string());
    }

    // Variable declaration
    for type_prefix in &[
        "int ",
        "double ",
        "float ",
        "long ",
        "short ",
        "unsigned ",
        "char ",
        "size_t ",
        "bool ",
        "_Bool ",
        "int32_t ",
        "uint32_t ",
        "int64_t ",
        "uint64_t ",
    ] {
        if stmt.starts_with(type_prefix) {
            return translate_var_decl(stmt);
        }
    }

    // Increment/decrement
    if let Some(inc) = try_translate_inc_dec(stmt) {
        return Some(inc);
    }

    // Compound assignment: x += expr;
    if let Some(compound) = try_translate_compound_assign(stmt) {
        return Some(compound);
    }

    // Simple assignment: x = expr;
    if stmt.contains('=')
        && stmt.ends_with(';')
        && !stmt.contains("==")
        && !stmt.contains("!=")
        && !stmt.contains("+=")
        && !stmt.contains("-=")
        && !stmt.contains("*=")
        && !stmt.contains("/=")
        && !stmt.contains("%=")
        && !stmt.contains("<=")
        && !stmt.contains(">=")
    {
        let no_semi = stmt.strip_suffix(';')?.trim();
        let parts: Vec<&str> = no_semi.splitn(2, '=').collect();
        if parts.len() == 2 {
            let lhs = parts[0].trim();
            let rhs = translate_expr(parts[1].trim())?;
            return Some(format!("{lhs} = {rhs};"));
        }
    }

    // printf -> println!
    if stmt.starts_with("printf(") {
        return translate_printf(stmt);
    }

    // Standalone function call: name(args);
    if stmt.ends_with(");") {
        let call = stmt.strip_suffix(';')?.trim();
        if let Some(paren) = call.find('(')
            && call.ends_with(')')
        {
            let name = &call[..paren];
            if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.is_empty() {
                let args_str = &call[paren + 1..call.len() - 1];
                let args = split_args(args_str);
                let mut rust_args = Vec::new();
                for arg in &args {
                    let t = arg.trim();
                    if t.is_empty() {
                        continue;
                    }
                    rust_args.push(translate_expr(t)?);
                }
                return Some(format!("{name}({});", rust_args.join(", ")));
            }
        }
    }

    None
}

fn try_translate_inc_dec(stmt: &str) -> Option<String> {
    let no_semi = stmt.strip_suffix(';')?.trim();
    let is_ident = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_');

    let (var, op) = [
        no_semi.strip_suffix("++").map(|v| (v, "+=")),
        no_semi.strip_suffix("--").map(|v| (v, "-=")),
        no_semi.strip_prefix("++").map(|v| (v, "+=")),
        no_semi.strip_prefix("--").map(|v| (v, "-=")),
    ]
    .into_iter()
    .flatten()
    .find(|(v, _)| is_ident(v))?;

    Some(format!("{var} {op} 1;"))
}

fn try_translate_compound_assign(stmt: &str) -> Option<String> {
    let no_semi = stmt.strip_suffix(';')?.trim();
    for op in &["+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>="] {
        if let Some(pos) = no_semi.find(op) {
            let lhs = no_semi[..pos].trim();
            let rhs = translate_expr(no_semi[pos + op.len()..].trim())?;
            return Some(format!("{lhs} {op} {rhs};"));
        }
    }
    None
}

// --- Expression translation ---

/// Split arguments respecting parenthesized expressions.
fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut current = String::new();
    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        args.push(trimmed);
    }
    args
}

fn translate_expr(expr: &str) -> Option<String> {
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }

    // Boolean literals
    if expr == "true" || expr == "false" {
        return Some(expr.to_string());
    }

    // Simple identifiers
    if expr.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Some(expr.to_string());
    }

    // Numeric literals
    if is_numeric_literal(expr) {
        return Some(clean_numeric_literal(expr));
    }

    // Negative number
    if expr.starts_with('-') && expr.len() > 1 && is_numeric_literal(&expr[1..]) {
        return Some(format!("-{}", clean_numeric_literal(&expr[1..])));
    }

    // Ternary: cond ? a : b
    if let Some(result) = try_translate_ternary(expr) {
        return Some(result);
    }

    // Binary operators (ordered by precedence: logical → comparison → bitwise → arithmetic)
    const BINARY_OPS: &[&[&str]] = &[
        &[" && ", " || "],
        &[" == ", " != ", " <= ", " >= ", " < ", " > "],
        &[" << ", " >> ", " & ", " | ", " ^ "],
        &[" + ", " - ", " * ", " / ", " % "],
    ];
    for group in BINARY_OPS {
        for op in *group {
            if let Some(pos) = find_operator(expr, op) {
                let lhs = translate_expr(&expr[..pos])?;
                let rhs = translate_expr(&expr[pos + op.len()..])?;
                return Some(format!("{lhs}{op}{rhs}"));
            }
        }
    }

    // Unary not
    if let Some(rest) = expr.strip_prefix('!') {
        let inner = translate_expr(rest)?;
        return Some(format!("!{inner}"));
    }

    // Unary minus (expressions)
    if expr.starts_with('-') && expr.len() > 1 {
        let inner = translate_expr(&expr[1..])?;
        return Some(format!("-{inner}"));
    }

    // Cast: (type)expr
    if expr.starts_with('(')
        && let Some(close) = expr.find(')')
    {
        let maybe_type = expr[1..close].trim();
        if c_type_to_rust(maybe_type).is_some() && close + 1 < expr.len() {
            let inner = translate_expr(&expr[close + 1..])?;
            let rust_ty = c_type_to_rust(maybe_type)?;
            return Some(format!("{inner} as {rust_ty}"));
        }
    }

    // Function call: name(args)
    if let Some(paren) = expr.find('(')
        && expr.ends_with(')')
    {
        let name = &expr[..paren];
        if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.is_empty() {
            let args_str = &expr[paren + 1..expr.len() - 1];
            let args = split_args(args_str);
            let mut rust_args = Vec::new();
            for arg in &args {
                let t = arg.trim();
                if t.is_empty() {
                    continue;
                }
                rust_args.push(translate_expr(t)?);
            }
            return Some(format!("{name}({})", rust_args.join(", ")));
        }
    }

    // Parenthesized expression
    if expr.starts_with('(') && expr.ends_with(')') {
        let inner = &expr[1..expr.len() - 1];
        if parens_balanced(inner) {
            let translated = translate_expr(inner)?;
            return Some(format!("({translated})"));
        }
    }

    None
}

fn is_numeric_literal(s: &str) -> bool {
    // Check hex first (before stripping suffixes, since F is a hex digit)
    if s.starts_with("0x") || s.starts_with("0X") {
        let hex = s.trim_end_matches(['u', 'U', 'l', 'L']);
        return hex.len() > 2 && hex[2..].chars().all(|c| c.is_ascii_hexdigit() || c == '_');
    }
    let s = s.trim_end_matches(['l', 'L', 'u', 'U', 'f', 'F']);
    s.parse::<i64>().is_ok() || s.parse::<f64>().is_ok()
}

fn clean_numeric_literal(s: &str) -> String {
    s.trim_end_matches(['l', 'L', 'u', 'U', 'f', 'F'])
        .to_string()
}

fn parens_balanced(s: &str) -> bool {
    let mut depth: i32 = 0;
    for ch in s.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Find an operator in an expression, respecting parenthesized sub-expressions.
fn find_operator(expr: &str, op: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    let bytes = expr.as_bytes();
    let op_bytes = op.as_bytes();
    if bytes.len() < op_bytes.len() {
        return None;
    }
    for i in 0..=bytes.len() - op_bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && &bytes[i..i + op_bytes.len()] == op_bytes {
            return Some(i);
        }
    }
    None
}

fn try_translate_ternary(expr: &str) -> Option<String> {
    let q_pos = find_operator(expr, " ? ")?;
    let rest = &expr[q_pos + 3..];
    let c_pos = find_operator(rest, " : ")?;

    let cond = translate_expr(expr[..q_pos].trim())?;
    let then_expr = translate_expr(rest[..c_pos].trim())?;
    let else_expr = translate_expr(rest[c_pos + 3..].trim())?;

    Some(format!(
        "if {cond} {{ {then_expr} }} else {{ {else_expr} }}"
    ))
}

// --- Variable declarations ---

fn translate_var_decl(stmt: &str) -> Option<String> {
    let no_semi = stmt.strip_suffix(';')?.trim();
    let (c_type, rest) = split_type_and_rest(no_semi)?;
    let rust_type = c_type_to_rust(&c_type)?;
    let rest = rest.trim();

    if let Some(eq_pos) = rest.find('=') {
        let name = rest[..eq_pos].trim();
        let expr = translate_expr(rest[eq_pos + 1..].trim())?;
        Some(format!("let {name}: {rust_type} = {expr};"))
    } else {
        let name = rest;
        let default = default_for_type(rust_type);
        Some(format!("let mut {name}: {rust_type} = {default};"))
    }
}

fn split_type_and_rest(decl: &str) -> Option<(String, String)> {
    let words: Vec<&str> = decl.split_whitespace().collect();
    if words.len() < 2 {
        return None;
    }
    // Try progressively longer type prefixes
    for len in (1..words.len()).rev() {
        let candidate = words[..len].join(" ");
        if c_type_to_rust(&candidate).is_some() {
            let rest = words[len..].join(" ");
            return Some((candidate, rest));
        }
    }
    None
}

fn default_for_type(rust_type: &str) -> &str {
    match rust_type {
        "f32" | "f64" => "0.0",
        "bool" => "false",
        _ => "0",
    }
}

// --- Printf translation ---

fn translate_printf(stmt: &str) -> Option<String> {
    let inner = stmt.strip_prefix("printf(")?.strip_suffix(");")?.trim();

    let fmt_start = inner.find('"')?;
    let fmt_end = inner[fmt_start + 1..].find('"')? + fmt_start + 1;
    let fmt_str = &inner[fmt_start + 1..fmt_end];

    // Longest specifiers first to avoid partial matches (e.g. %lld before %d)
    const FMT_MAP: &[(&str, &str)] = &[
        ("%lld", "{}"),
        ("%llu", "{}"),
        ("%ld", "{}"),
        ("%lu", "{}"),
        ("%lf", "{}"),
        ("%zu", "{}"),
        ("%d", "{}"),
        ("%i", "{}"),
        ("%u", "{}"),
        ("%f", "{}"),
        ("%s", "{}"),
        ("%c", "{}"),
        ("%x", "{:x}"),
        ("%X", "{:X}"),
        ("%o", "{:o}"),
        ("%p", "{:p}"),
        ("%%", "%"),
        ("\\n", ""),
    ];
    let mut rust_fmt = fmt_str.to_string();
    for &(from, to) in FMT_MAP {
        rust_fmt = rust_fmt.replace(from, to);
    }

    let after_fmt = &inner[fmt_end + 1..];
    let args_str = after_fmt.trim().strip_prefix(',').unwrap_or("").trim();

    if args_str.is_empty() {
        Some(format!("println!(\"{rust_fmt}\");"))
    } else {
        let args = split_args(args_str);
        let mut rust_args = Vec::new();
        for arg in &args {
            rust_args.push(translate_expr(arg.trim())?);
        }
        Some(format!(
            "println!(\"{rust_fmt}\", {});",
            rust_args.join(", ")
        ))
    }
}

// --- Control flow ---

/// Translate an if/else-if/else chain from lines starting at `start`.
/// Returns the translated block and the number of lines consumed.
fn translate_if_chain(lines: &[&str], start: usize, indent: &str) -> Option<(String, usize)> {
    let mut result = String::new();
    let mut i = start;
    let inner_indent = format!("{indent}    ");

    // First "if"
    let trimmed = lines[i].trim();
    let cond = extract_paren_condition(trimmed)?;
    let rust_cond = translate_expr(&cond)?;

    let (body_str, body_end) = extract_block_body(lines, i)?;
    let body = translate_body(&body_str, &inner_indent)?;
    result.push_str(&format!("{indent}if {rust_cond} {{\n{body}\n{indent}}}"));

    // Check if the closing brace line also has "else"
    let closing_line = lines[body_end].trim();
    if closing_line.contains("else") {
        i = body_end; // stay on this line for else processing
    } else {
        i = body_end + 1;
    }

    // Else-if and else
    while i < lines.len() {
        let t = lines[i].trim();

        if t.contains("else if") {
            let cond = extract_paren_condition(t)?;
            let rust_cond = translate_expr(&cond)?;
            let (body_str, body_end) = extract_block_body(lines, i)?;
            let body = translate_body(&body_str, &inner_indent)?;
            result.push_str(&format!(" else if {rust_cond} {{\n{body}\n{indent}}}"));
            let closing = lines[body_end].trim();
            i = if closing.contains("else") {
                body_end
            } else {
                body_end + 1
            };
        } else if t.contains("else") {
            let (body_str, body_end) = extract_block_body(lines, i)?;
            let body = translate_body(&body_str, &inner_indent)?;
            result.push_str(&format!(" else {{\n{body}\n{indent}}}"));
            i = body_end + 1;
            break;
        } else {
            break;
        }
    }

    result.push('\n');
    Some((result, i - start))
}

fn translate_while_block(lines: &[&str], start: usize, indent: &str) -> Option<(String, usize)> {
    let trimmed = lines[start].trim();
    let cond = extract_paren_condition(trimmed)?;
    let rust_cond = translate_expr(&cond)?;
    let inner_indent = format!("{indent}    ");

    let (body_str, body_end) = extract_block_body(lines, start)?;
    let body = translate_body(&body_str, &inner_indent)?;

    let result = format!("{indent}while {rust_cond} {{\n{body}\n{indent}}}\n");
    Some((result, body_end + 1 - start))
}

fn translate_for_block(lines: &[&str], start: usize, indent: &str) -> Option<(String, usize)> {
    let trimmed = lines[start].trim();
    let inner_indent = format!("{indent}    ");

    // Extract for(...) content
    let for_header = extract_paren_condition(trimmed)?;
    let parts: Vec<&str> = for_header.splitn(3, ';').collect();
    if parts.len() != 3 {
        return None;
    }

    let init = parts[0].trim();
    let cond = parts[1].trim();
    let step = parts[2].trim();

    let rust_init = if init.is_empty() {
        String::new()
    } else if init.contains(' ') {
        // Variable declaration in init — always mut since the step mutates it
        let decl = translate_var_decl(&format!("{init};"))?;
        decl.replacen("let ", "let mut ", 1)
    } else {
        translate_statement(&format!("{init};"))?
    };

    let rust_cond = if cond.is_empty() {
        "true".to_string()
    } else {
        translate_expr(cond)?
    };

    let rust_step = if step.is_empty() {
        String::new()
    } else {
        translate_statement(&format!("{step};"))?
    };

    let (body_str, body_end) = extract_block_body(lines, start)?;
    let body = translate_body(&body_str, &inner_indent)?;

    let mut result = String::new();
    if !rust_init.is_empty() {
        result.push_str(&format!("{indent}{rust_init}\n"));
    }
    result.push_str(&format!("{indent}while {rust_cond} {{\n{body}\n"));
    if !rust_step.is_empty() {
        result.push_str(&format!("{inner_indent}{rust_step}\n"));
    }
    result.push_str(&format!("{indent}}}\n"));

    Some((result, body_end + 1 - start))
}

/// Extract the parenthesized condition from a line like `if (x < 0) {` or `} else if (x > 0) {`.
fn extract_paren_condition(line: &str) -> Option<String> {
    let paren_start = line.find('(')?;
    extract_balanced_parens(line, paren_start)
}

fn extract_balanced_parens(s: &str, start: usize) -> Option<String> {
    if s.as_bytes().get(start)? != &b'(' {
        return None;
    }
    let mut depth = 0;
    for (i, ch) in s[start..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start + 1..start + i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract the body of a braced block from `lines[start..]`.
/// The `{` may be on the same line as the control statement or on the next line.
/// Returns (body_content, last_line_index).
fn extract_block_body(lines: &[&str], start: usize) -> Option<(String, usize)> {
    // Find the opening brace
    let brace_line = if lines[start].contains('{') {
        start
    } else if start + 1 < lines.len() && lines[start + 1].trim().starts_with('{') {
        start + 1
    } else {
        return None;
    };

    extract_braced_block(lines, brace_line)
}

// --- Main function ---

fn translate_main(body: &str) -> Option<String> {
    let cleaned = body.replace("return 0;", "");
    let body = translate_body(&cleaned, "    ")?;
    Some(format!("fn main() {{\n{body}\n}}"))
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_add() {
        let c_source = r#"
int add(int a, int b) {
    return a + b;
}

int main(void) {
    printf("%d\n", add(2, 3));
    printf("%d\n", add(-1, 1));
    printf("%d\n", add(0, 0));
    return 0;
}
"#;
        let result = try_translate(c_source, "add").unwrap();
        assert!(result.contains("pub fn add(a: i32, b: i32) -> i32"));
        assert!(result.contains("a + b"));
        assert!(result.contains("fn main()"));
        assert!(result.contains("println!"));
    }

    #[test]
    fn test_translate_simple_arithmetic() {
        let c_source = r#"
int multiply(int x, int y) {
    return x * y;
}
"#;
        let result = try_translate(c_source, "multiply").unwrap();
        assert!(result.contains("pub fn multiply(x: i32, y: i32) -> i32"));
        assert!(result.contains("x * y"));
    }

    #[test]
    fn test_translate_rejects_pointers() {
        let c_source = r#"
void copy(char *dst, const char *src) {
    while (*src) { *dst++ = *src++; }
}
"#;
        assert!(try_translate(c_source, "copy").is_none());
    }

    #[test]
    fn test_translate_rejects_malloc() {
        let c_source = r#"
int* create(int n) {
    int *arr = (int *)malloc(n * sizeof(int));
    return arr;
}
"#;
        assert!(try_translate(c_source, "create").is_none());
    }

    #[test]
    fn test_c_type_to_rust() {
        assert_eq!(c_type_to_rust("int"), Some("i32"));
        assert_eq!(c_type_to_rust("double"), Some("f64"));
        assert_eq!(c_type_to_rust("void"), Some("()"));
        assert_eq!(c_type_to_rust("bool"), Some("bool"));
        assert_eq!(c_type_to_rust("long long"), Some("i64"));
        assert_eq!(c_type_to_rust("const char *"), None);
    }

    #[test]
    fn test_translate_printf() {
        let stmt = r#"printf("%d\n", add(2, 3));"#;
        let result = translate_printf(stmt).unwrap();
        assert_eq!(result, "println!(\"{}\", add(2, 3));");
    }

    #[test]
    fn test_translate_if_else() {
        let c_source = r#"
int abs_val(int x) {
    if (x < 0) {
        return -x;
    } else {
        return x;
    }
}
"#;
        let result = try_translate(c_source, "abs_val").unwrap();
        assert!(result.contains("if x < 0"));
        assert!(result.contains("-x"));
        assert!(result.contains("else"));
    }

    #[test]
    fn test_translate_while_loop() {
        let c_source = r#"
int sum_to(int n) {
    int total = 0;
    int i = 1;
    while (i <= n) {
        total += i;
        i++;
    }
    return total;
}
"#;
        let result = try_translate(c_source, "sum_to").unwrap();
        assert!(result.contains("while i <= n"));
        assert!(result.contains("total += i;"));
        assert!(result.contains("i += 1;"));
    }

    #[test]
    fn test_translate_for_loop() {
        let c_source = r#"
int factorial(int n) {
    int result = 1;
    for (int i = 2; i <= n; i++) {
        result *= i;
    }
    return result;
}
"#;
        let result = try_translate(c_source, "factorial").unwrap();
        assert!(result.contains("let mut i: i32 = 2;"));
        assert!(result.contains("while i <= n"));
        assert!(result.contains("result *= i;"));
        assert!(result.contains("i += 1;"));
    }

    #[test]
    fn test_translate_ternary() {
        let c_source = r#"
int max(int a, int b) {
    return a > b ? a : b;
}
"#;
        let result = try_translate(c_source, "max").unwrap();
        assert!(result.contains("if a > b { a } else { b }"));
    }

    #[test]
    fn test_translate_compound_assign() {
        let result = try_translate_compound_assign("x += 5;").unwrap();
        assert_eq!(result, "x += 5;");
        let result = try_translate_compound_assign("y *= 2;").unwrap();
        assert_eq!(result, "y *= 2;");
    }

    #[test]
    fn test_translate_inc_dec() {
        assert_eq!(try_translate_inc_dec("i++;"), Some("i += 1;".to_string()));
        assert_eq!(try_translate_inc_dec("--j;"), Some("j -= 1;".to_string()));
    }

    #[test]
    fn test_find_operator_respects_parens() {
        // `(a + b) * c`: the ` * ` starts at index 7
        assert_eq!(find_operator("(a + b) * c", " * "), Some(7));
        assert_eq!(find_operator("a + (b * c)", " + "), Some(1));
        // Should NOT find the + inside parens
        assert_eq!(find_operator("(a + b)", " + "), None);
    }

    #[test]
    fn test_numeric_literals() {
        assert!(is_numeric_literal("42"));
        assert!(is_numeric_literal("42L"));
        assert!(is_numeric_literal("3.14f"));
        assert!(is_numeric_literal("0xFF"));
        assert_eq!(clean_numeric_literal("42L"), "42");
        assert_eq!(clean_numeric_literal("3.14f"), "3.14");
    }

    #[test]
    fn test_translate_else_if_chain() {
        let c_source = r#"
int classify(int x) {
    if (x > 0) {
        return 1;
    } else if (x < 0) {
        return -1;
    } else {
        return 0;
    }
}
"#;
        let result = try_translate(c_source, "classify").unwrap();
        assert!(result.contains("if x > 0"));
        assert!(result.contains("else if x < 0"));
        assert!(result.contains("else"));
    }
}
