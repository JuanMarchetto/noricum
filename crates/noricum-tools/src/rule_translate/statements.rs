//! Statement translation: body translation, individual statement dispatch.

use super::blocks::{translate_for_block, translate_if_chain, translate_while_block};
use super::expressions::{split_args, translate_expr};
use super::functions::{c_type_to_rust, default_for_type, find_mutated_vars};

const COMPLEX_MARKERS: &[&str] = &[
    "malloc", "free(", "->", "void *", "goto ", "switch ", "sizeof", "memcpy", "memset", "strcpy",
    "strcat",
];

fn is_complex(stmt: &str) -> bool {
    COMPLEX_MARKERS.iter().any(|marker| stmt.contains(marker))
}

/// Check if a C expression evaluates to a boolean in Rust but int in C.
pub(super) fn is_boolean_expr(expr: &str) -> bool {
    for op in &["==", "!=", "<=", ">=", "<", ">", "&&", "||"] {
        if expr.contains(op) {
            return true;
        }
    }
    false
}

pub(super) fn translate_body(body: &str, indent: &str) -> Option<String> {
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

pub(super) fn translate_statement(stmt: &str) -> Option<String> {
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
        return super::blocks::translate_printf(stmt);
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

pub(super) fn try_translate_inc_dec(stmt: &str) -> Option<String> {
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

pub(super) fn try_translate_compound_assign(stmt: &str) -> Option<String> {
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

pub(super) fn translate_var_decl(stmt: &str) -> Option<String> {
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
